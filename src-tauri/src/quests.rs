//! A character's quest log (Q1b): how many quests it has completed (from
//! `char_quests_done`) and its recent accepts and turn-ins, with where and
//! from whom (the addon's `quest_accepted` and `quest` events, kept in
//! `adventure_events`). The app shows it only when there's something in it.

use serde::Serialize;

use crate::db::Db;
use crate::error::AppResult;

/// How many log entries come back at most.
const MAX_ENTRIES: u32 = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum QuestKind {
    Accepted,
    TurnedIn,
}

/// One accept or turn-in. Everything but the time and kind is optional:
/// older addons recorded only the id and title, and the place is missing
/// inside instances.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct QuestEntry {
    /// RFC 3339, UTC.
    pub at: String,
    pub kind: QuestKind,
    pub quest_id: Option<u32>,
    pub title: Option<String>,
    pub zone: Option<String>,
    /// The NPC it was taken from or handed to; never another player.
    pub giver: Option<String>,
    /// uiMapID, and the position on it (0 to 1).
    pub map: Option<u32>,
    pub x: Option<f64>,
    pub y: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct QuestLog {
    /// Quests completed, as of `done_as_of`; 0 before an addon that reports it.
    pub done: u32,
    /// When the addon last listed them (RFC 3339, UTC).
    pub done_as_of: Option<String>,
    /// Newest first.
    pub entries: Vec<QuestEntry>,
}

fn rfc3339(t: i64) -> String {
    chrono::DateTime::from_timestamp(t, 0)
        .unwrap_or_default()
        .to_rfc3339()
}

pub fn log(db: &Db, character_id: u32) -> AppResult<QuestLog> {
    db.with_conn(|c| {
        let (done, as_of): (i64, Option<i64>) = c.query_row(
            "SELECT count(*), max(as_of) FROM char_quests_done WHERE character_id = ?1",
            [character_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let mut stmt = c.prepare(
            "SELECT e.at, e.kind, e.data FROM adventure_events e
             JOIN adventures a ON a.id = e.adventure_id
             WHERE a.character_id = ?1 AND e.kind IN ('quest_accepted', 'quest')
             ORDER BY e.at DESC, e.seq DESC LIMIT ?2",
        )?;
        let entries = stmt
            .query_map(rusqlite::params![character_id, MAX_ENTRIES], |r| {
                let at: i64 = r.get(0)?;
                let kind: String = r.get(1)?;
                let data: String = r.get(2)?;
                Ok((at, kind, data))
            })?
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|(at, kind, data)| {
                let d: serde_json::Value = serde_json::from_str(&data).unwrap_or_default();
                let text = |k: &str| d.get(k).and_then(|v| v.as_str()).map(str::to_string);
                let id = |k: &str| {
                    d.get(k)
                        .and_then(|v| v.as_u64())
                        .and_then(|n| u32::try_from(n).ok())
                };
                let num = |k: &str| d.get(k).and_then(|v| v.as_f64());
                QuestEntry {
                    at: rfc3339(at),
                    kind: if kind == "quest" {
                        QuestKind::TurnedIn
                    } else {
                        QuestKind::Accepted
                    },
                    quest_id: id("id"),
                    title: text("title"),
                    zone: text("zone"),
                    giver: text("giver"),
                    map: id("map"),
                    x: num("x"),
                    y: num("y"),
                }
            })
            .collect();
        Ok(QuestLog {
            done: done as u32,
            done_as_of: as_of.map(rfc3339),
            entries,
        })
    })
}

/// Whether any character of `flavor` has quest data yet: the sheet's Quests
/// tab ships dark until then (IMPLEMENTING §14), then shows for everyone.
pub fn any(db: &Db, flavor: &str) -> AppResult<bool> {
    db.with_conn(|c| {
        Ok(c.query_row(
            "SELECT EXISTS (SELECT 1 FROM char_quests_done q
                            JOIN characters ch ON ch.id = q.character_id WHERE ch.flavor = ?1)
                 OR EXISTS (SELECT 1 FROM adventure_events e
                            JOIN adventures a ON a.id = e.adventure_id
                            JOIN characters ch ON ch.id = a.character_id
                            WHERE ch.flavor = ?1 AND e.kind IN ('quest_accepted', 'quest'))",
            [flavor],
            |r| r.get(0),
        )?)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::{ingest_bytes, target_for};

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/addon")
                .join(name),
        )
        .unwrap()
    }

    #[test]
    fn the_log_from_the_addon_file() {
        let db = Db::open_in_memory().unwrap();
        let t = target_for(
            "_classic_beta_",
            "WTF/Account/ACCOUNT1/70/Thrandor-Vargur/SavedVariables/ForeverBuddy.lua",
        )
        .unwrap();
        ingest_bytes(&db, &t, &fixture("quests.lua")).unwrap();
        let log = log(&db, 1).unwrap();
        assert_eq!(log.done, 3);
        assert!(log.done_as_of.is_some());
        assert_eq!(log.entries.len(), 2);
        // Newest first: the turn-in, then the accept.
        let turned_in = &log.entries[0];
        assert_eq!(turned_in.kind, QuestKind::TurnedIn);
        assert_eq!(turned_in.quest_id, Some(176));
        assert_eq!(turned_in.giver.as_deref(), Some("Marshal Dughan"));
        let accepted = &log.entries[1];
        assert_eq!(accepted.kind, QuestKind::Accepted);
        assert_eq!(accepted.title.as_deref(), Some("Wanted: Hogger"));
        assert_eq!(accepted.zone.as_deref(), Some("Elwynn Forest"));
        assert_eq!(
            (accepted.map, accepted.x, accepted.y),
            (Some(1429), Some(0.412), Some(0.657))
        );
    }

    #[test]
    fn an_older_list_never_replaces_a_newer_one() {
        let db = Db::open_in_memory().unwrap();
        let t = target_for(
            "_classic_beta_",
            "WTF/Account/ACCOUNT1/70/Thrandor-Vargur/SavedVariables/ForeverBuddy.lua",
        )
        .unwrap();
        // first_login.lua is the newer snapshot (2 done); quests.lua is
        // older (3 done), as if a backup were replayed after it.
        ingest_bytes(&db, &t, &fixture("first_login.lua")).unwrap();
        assert_eq!(log(&db, 1).unwrap().done, 2);
        ingest_bytes(&db, &t, &fixture("quests.lua")).unwrap();
        assert_eq!(log(&db, 1).unwrap().done, 2, "the newer list stays");
    }

    #[test]
    fn empty_before_the_addon_reports_any() {
        let db = Db::open_in_memory().unwrap();
        let log = log(&db, 1).unwrap();
        assert_eq!((log.done, log.done_as_of, log.entries.len()), (0, None, 0));
        assert!(!any(&db, "_classic_beta_").unwrap(), "the tab stays dark");
    }

    #[test]
    fn any_turns_on_with_the_first_data() {
        let db = Db::open_in_memory().unwrap();
        let t = target_for(
            "_classic_beta_",
            "WTF/Account/ACCOUNT1/70/Thrandor-Vargur/SavedVariables/ForeverBuddy.lua",
        )
        .unwrap();
        ingest_bytes(&db, &t, &fixture("quests.lua")).unwrap();
        assert!(any(&db, "_classic_beta_").unwrap());
        assert!(!any(&db, "_retail_").unwrap(), "per flavor");
    }
}
