//! Step 4 of the spec (docs/specs/v0.2-addon.md §4): one decoded file into
//! the db, in one transaction, idempotent.
//!
//! The same data arrives more than once (live files, fallback scans, and
//! gap-fill replaying older backups after newer files were already read), so
//! nothing older ever overwrites something newer:
//! - snapshots, gold points and items are keyed and inserted once;
//! - gear, bags, bank, mail, professions and lockouts are replaced only by a
//!   newer `as_of`;
//! - a session's events are replaced only by a list at least as long (an
//!   older copy of a session saved mid-play has fewer);
//! - the user's adventure note is never touched.

use rusqlite::{params, OptionalExtension, Transaction};

use crate::error::AppResult;
use crate::ingest::file::{AddonFile, SlotItem, Snapshot};

/// Which character a file belongs to: its folder, never its contents (the
/// folder wins, spec §4 step 3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub flavor: String,
    pub account: String,
    pub group_dir: String,
    pub char_dir: String,
}

/// Applies `file` for `target`. Returns the character's id.
pub fn apply(tx: &Transaction<'_>, target: &Target, file: &AddonFile) -> AppResult<i64> {
    let at = file.at().unwrap_or(0);
    let id = upsert_character(tx, target, file, at)?;
    if let Some(snap) = &file.snapshot {
        apply_snapshot(tx, id, snap)?;
    }
    for item in &file.items {
        tx.execute(
            "INSERT INTO items (item_id, name, quality, ilvl, icon_file_id, class_id,
                                subclass_id, sell_price, seen_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT (item_id) DO UPDATE SET
               name = coalesce(excluded.name, name), quality = coalesce(excluded.quality, quality),
               ilvl = coalesce(excluded.ilvl, ilvl),
               icon_file_id = coalesce(excluded.icon_file_id, icon_file_id),
               class_id = coalesce(excluded.class_id, class_id),
               subclass_id = coalesce(excluded.subclass_id, subclass_id),
               sell_price = coalesce(excluded.sell_price, sell_price),
               seen_at = max(seen_at, excluded.seen_at)
             WHERE excluded.seen_at >= items.seen_at",
            params![
                item.item_id,
                item.name,
                item.quality,
                item.ilvl,
                item.icon_file_id,
                item.class_id,
                item.subclass_id,
                item.sell_price,
                at
            ],
        )?;
    }
    for session in &file.sessions {
        apply_session(tx, id, &target.flavor, session)?;
    }
    Ok(id)
}

fn upsert_character(tx: &Transaction<'_>, t: &Target, file: &AddonFile, at: i64) -> AppResult<i64> {
    let c = &file.character;
    // The display name falls back to the folder when the addon couldn't
    // read it; the folder is the identity either way.
    let name = c.name.clone().unwrap_or_else(|| t.char_dir.clone());
    let level = file.snapshot.as_ref().and_then(|s| s.level).or(c.level);
    // Display fields only move forward: an older file (a backup replay)
    // doesn't overwrite what a newer one said.
    tx.execute(
        "INSERT INTO characters (flavor, account, group_dir, char_dir, name, surname, realm,
                                 guid, class, race, sex, faction, level, guild, guild_rank,
                                 first_seen, last_seen)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?16)
         ON CONFLICT (flavor, account, group_dir, char_dir) DO UPDATE SET
           first_seen = min(first_seen, excluded.first_seen),
           last_seen = max(last_seen, excluded.last_seen),
           name = CASE WHEN excluded.last_seen >= last_seen THEN excluded.name ELSE name END,
           surname = CASE WHEN excluded.last_seen >= last_seen
                          THEN coalesce(excluded.surname, surname) ELSE surname END,
           realm = CASE WHEN excluded.last_seen >= last_seen
                        THEN coalesce(excluded.realm, realm) ELSE realm END,
           guid = coalesce(guid, excluded.guid),
           class = coalesce(excluded.class, class),
           race = coalesce(excluded.race, race),
           sex = coalesce(excluded.sex, sex),
           faction = coalesce(excluded.faction, faction),
           level = CASE WHEN excluded.last_seen >= last_seen
                        THEN coalesce(excluded.level, level) ELSE level END,
           guild = CASE WHEN excluded.last_seen >= last_seen THEN excluded.guild ELSE guild END,
           guild_rank = CASE WHEN excluded.last_seen >= last_seen
                             THEN excluded.guild_rank ELSE guild_rank END",
        params![
            t.flavor,
            t.account,
            t.group_dir,
            t.char_dir,
            name,
            c.surname,
            c.realm,
            c.guid,
            c.class,
            c.race,
            c.sex,
            c.faction,
            level,
            c.guild,
            c.guild_rank,
            at
        ],
    )?;
    Ok(tx.query_row(
        "SELECT id FROM characters
         WHERE flavor = ?1 AND account = ?2 AND group_dir = ?3 AND char_dir = ?4",
        params![t.flavor, t.account, t.group_dir, t.char_dir],
        |r| r.get(0),
    )?)
}

fn apply_snapshot(tx: &Transaction<'_>, id: i64, s: &Snapshot) -> AppResult<()> {
    tx.execute(
        "INSERT OR IGNORE INTO char_snapshots (character_id, at, money, xp, xp_max, rested, level,
            ilvl_avg, ilvl_equipped, played_total, played_level, zone, subzone)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            id,
            s.at,
            s.money.unwrap_or(0),
            s.xp,
            s.xp_max,
            s.rested,
            s.level,
            s.ilvl_avg,
            s.ilvl_equipped,
            s.played_total,
            s.played_level,
            s.zone,
            s.subzone
        ],
    )?;
    if let Some(money) = s.money {
        tx.execute(
            "INSERT OR IGNORE INTO gold_points (character_id, at, money) VALUES (?1, ?2, ?3)",
            params![id, s.at, money],
        )?;
    }
    if let Some(items) = &s.equipped {
        replace_items(tx, id, "equipped", s.at, items)?;
    }
    if let Some(items) = &s.bags {
        if replace_items(tx, id, "bag", s.at, items)? {
            replace_containers(tx, id, "bag", s.at, &s.bag_info)?;
        }
    }
    // Bank and mail carry their own time: the last visit, carried forward
    // by the addon when this session had none.
    if let Some(bank) = &s.bank {
        if replace_items(tx, id, "bank", bank.at, &bank.items)? {
            replace_containers(tx, id, "bank", bank.at, &bank.tabs)?;
        }
    }
    if let Some(mail) = &s.mail {
        if replace_items(tx, id, "mail", mail.at, &mail.items)? {
            tx.execute("DELETE FROM char_mail WHERE character_id = ?1", [id])?;
            for m in &mail.messages {
                tx.execute(
                    "INSERT OR REPLACE INTO char_mail (character_id, idx, sender, subject, money,
                                                       cod, days_left, as_of)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        id,
                        m.idx,
                        m.sender,
                        m.subject,
                        m.money,
                        m.cod,
                        m.days_left,
                        mail.at
                    ],
                )?;
            }
        }
    }
    if let Some(professions) = &s.professions {
        if newer(tx, "professions", id, s.at)? {
            tx.execute("DELETE FROM professions WHERE character_id = ?1", [id])?;
            for p in professions {
                tx.execute(
                    "INSERT OR REPLACE INTO professions (character_id, name, skill, max, spec, as_of)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![id, p.name, p.skill, p.max, p.spec, s.at],
                )?;
            }
        }
    }
    if let Some(lockouts) = &s.lockouts {
        if newer(tx, "lockouts", id, s.at)? {
            tx.execute("DELETE FROM lockouts WHERE character_id = ?1", [id])?;
            for l in lockouts {
                tx.execute(
                    "INSERT OR REPLACE INTO lockouts (character_id, name, difficulty, reset_at, as_of)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![id, l.name, l.difficulty, l.reset_at, s.at],
                )?;
            }
        }
    }
    Ok(())
}

/// `as_of` is at least as new as anything stored in `table` for `id`.
fn newer(tx: &Transaction<'_>, table: &str, id: i64, as_of: i64) -> AppResult<bool> {
    let stored: Option<i64> = tx.query_row(
        &format!("SELECT max(as_of) FROM {table} WHERE character_id = ?1"),
        [id],
        |r| r.get(0),
    )?;
    Ok(stored.is_none_or(|s| as_of >= s))
}

/// Replaces one location's bags or tabs, together with its items (the caller
/// only calls this when `replace_items` did), so the two never disagree.
fn replace_containers(
    tx: &Transaction<'_>,
    id: i64,
    location: &str,
    as_of: i64,
    containers: &[crate::ingest::file::Container],
) -> AppResult<()> {
    tx.execute(
        "DELETE FROM char_bags WHERE character_id = ?1 AND location = ?2",
        params![id, location],
    )?;
    for c in containers {
        tx.execute(
            "INSERT OR REPLACE INTO char_bags (character_id, location, container, name, size,
                                               free, as_of)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, location, c.container, c.name, c.size, c.free, as_of],
        )?;
    }
    Ok(())
}

/// Replaces one location's items, unless what's stored is newer. Returns
/// whether it replaced them.
fn replace_items(
    tx: &Transaction<'_>,
    id: i64,
    location: &str,
    as_of: i64,
    items: &[SlotItem],
) -> AppResult<bool> {
    let stored: Option<i64> = tx.query_row(
        "SELECT max(as_of) FROM char_items WHERE character_id = ?1 AND location = ?2",
        params![id, location],
        |r| r.get(0),
    )?;
    if stored.is_some_and(|s| as_of < s) {
        return Ok(false);
    }
    tx.execute(
        "DELETE FROM char_items WHERE character_id = ?1 AND location = ?2",
        params![id, location],
    )?;
    for i in items {
        tx.execute(
            "INSERT INTO char_items (character_id, location, container, slot, item_id, link,
                                     count, as_of)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id,
                location,
                i.container,
                i.slot,
                i.item_id,
                i.link,
                i.count,
                as_of
            ],
        )?;
    }
    Ok(true)
}

fn apply_session(
    tx: &Transaction<'_>,
    id: i64,
    flavor: &str,
    s: &crate::ingest::file::Session,
) -> AppResult<()> {
    let last = |kind: &str, field: &str| {
        s.events
            .iter()
            .rev()
            .filter(|e| e.kind == kind)
            .find_map(|e| e.data.get(field).and_then(serde_json::Value::as_i64))
    };
    let end_money = last("money", "money").or(s.start_money);
    let end_level = last("level", "level").or(s.start_level);
    tx.execute(
        "INSERT INTO adventures (character_id, login, logout, start_money, end_money, start_xp,
                                 start_level, end_level)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT (character_id, login) DO UPDATE SET
           logout = coalesce(excluded.logout, logout),
           start_money = coalesce(start_money, excluded.start_money),
           start_xp = coalesce(start_xp, excluded.start_xp),
           start_level = coalesce(start_level, excluded.start_level),
           end_money = coalesce(excluded.end_money, end_money),
           end_level = coalesce(excluded.end_level, end_level)",
        params![
            id,
            s.login,
            s.logout,
            s.start_money,
            end_money,
            s.start_xp,
            s.start_level,
            end_level
        ],
    )?;
    let adventure: i64 = tx.query_row(
        "SELECT id FROM adventures WHERE character_id = ?1 AND login = ?2",
        params![id, s.login],
        |r| r.get(0),
    )?;
    let stored: i64 = tx.query_row(
        "SELECT count(*) FROM adventure_events WHERE adventure_id = ?1",
        [adventure],
        |r| r.get(0),
    )?;
    if s.events.len() as i64 >= stored {
        tx.execute(
            "DELETE FROM adventure_events WHERE adventure_id = ?1",
            [adventure],
        )?;
        for e in &s.events {
            tx.execute(
                "INSERT INTO adventure_events (adventure_id, seq, at, kind, data)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![adventure, e.seq, e.at, e.kind, e.data.to_string()],
            )?;
        }
    }
    for e in s.events.iter().filter(|e| e.kind == "money") {
        if let Some(money) = e.data.get("money").and_then(serde_json::Value::as_i64) {
            tx.execute(
                "INSERT OR IGNORE INTO gold_points (character_id, at, money) VALUES (?1, ?2, ?3)",
                params![id, e.at, money],
            )?;
        }
    }
    // The T14 process session this login falls in, if any, so Recent
    // sessions can name its characters for certain.
    let linked: Option<i64> = tx
        .query_row(
            "SELECT id FROM play_sessions
             WHERE flavor = ?1 AND unixepoch(started_at) <= ?2 + 60
               AND (ended_at IS NULL OR unixepoch(ended_at) >= ?2)
             ORDER BY started_at DESC LIMIT 1",
            params![flavor, s.login],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(play) = linked {
        tx.execute(
            "UPDATE adventures SET play_session_id = ?1 WHERE id = ?2 AND play_session_id IS NULL",
            params![play, adventure],
        )?;
    }
    Ok(())
}
