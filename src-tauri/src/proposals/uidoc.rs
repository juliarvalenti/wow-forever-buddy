//! UI1: a whole UI document Claude proposes (`propose_ui`). It waits in
//! Approvals with its picture; "Use this" makes it a UI in use, which the
//! app sends to the game in the `UI` slot (`slot_entries`), and the addon
//! draws it at the next login instead of the standard HUD. "Go back to the
//! standard HUD" clears it.
//!
//! UI3: each document in use is for someone (`for`): everyone, some classes
//! or specs, or some characters, matched by GUID because Forever's names
//! carry surnames. One document per `for`, at most `MAX_IN_USE`, per game
//! folder flavor. The slot lists them most specific first and the addon
//! draws the first that matches its character; no match is the standard HUD.
//!
//! The document is checked twice, by the tool and again on pickup: the
//! render limits and the language's own check (`preview::ui::check`). The
//! stored body is exactly what's drawn and sent.

use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::sv::{LuaTable, LuaValue};

/// A document's name: a few words, "Halo".
pub const MAX_NAME: usize = 40;
/// Documents in use at once: 8 of 64 KB, and the old addons' copy of
/// everyone's, stay inside the bridge's 1 MB slot.
pub const MAX_IN_USE: usize = 8;
/// Entries in one `for` list.
const MAX_TARGETS: usize = 40;
/// The class file tokens there are test characters for (`fixtures.lua`).
pub const CLASSES: [&str; 9] = [
    "WARRIOR", "PALADIN", "HUNTER", "ROGUE", "PRIEST", "SHAMAN", "MAGE", "WARLOCK", "DRUID",
];

/// Whom a proposal is for, as the agent names them: characters by name,
/// classes by file token (`ROGUE`), specs by id. Leave it out for everyone.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiForRequest {
    #[serde(default)]
    pub characters: Vec<String>,
    #[serde(default)]
    pub classes: Vec<String>,
    #[serde(default)]
    pub specs: Vec<u32>,
}

/// Whom a document is for, as stored and sent: characters by GUID. Every
/// list sorted and without repeats, so one `for` has one form. Empty is
/// everyone.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiFor {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub characters: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub classes: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub specs: Vec<u32>,
}

impl UiFor {
    pub fn is_everyone(&self) -> bool {
        self.characters.is_empty() && self.classes.is_empty() && self.specs.is_empty()
    }

    /// The slot's order: characters, then specs, then classes, then everyone.
    fn rank(&self) -> u8 {
        if !self.characters.is_empty() {
            0
        } else if !self.specs.is_empty() {
            1
        } else if !self.classes.is_empty() {
            2
        } else {
            3
        }
    }

    fn key(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".into())
    }
}

/// The `propose_ui` tool's arguments, in the inbox file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiDocProposal {
    pub name: String,
    pub why: String,
    /// What the player asked for, in their words.
    #[serde(default)]
    pub words: Option<String>,
    /// UI3: whom it's for; everyone when left out.
    #[serde(default, rename = "for", skip_serializing_if = "Option::is_none")]
    pub target: Option<UiForRequest>,
    pub doc: Value,
}

/// After checking: what's stored, drawn and sent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UiDocBody {
    pub name: String,
    pub why: String,
    pub words: Option<String>,
    /// UI3: whom the agent proposed it for; UI1's bodies have none
    /// (everyone).
    #[serde(default, rename = "for", skip_serializing_if = "UiFor::is_everyone")]
    pub target: UiFor,
    pub doc: Value,
}

/// A document as Approvals shows it (its picture comes from
/// `ui_doc_picture`).
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct UiDocView {
    pub name: String,
    pub why: String,
    pub words: Option<String>,
    /// UI3: whom the agent proposed it for, the card's For row to start
    /// with (all empty: everyone).
    #[serde(rename = "for")]
    pub target: UiForPick,
    /// It's a UI in use now.
    pub active: bool,
}

/// The UI in use, for "Go back to the standard HUD".
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct ActiveUi {
    pub id: u32,
    pub name: String,
}

fn line(what: &str, s: &str, max: usize, required: bool) -> Result<Option<String>, String> {
    let s = super::plain_line(s);
    if s.is_empty() {
        return if required {
            Err(format!("The UI's {what} is empty."))
        } else {
            Ok(None)
        };
    }
    if s.chars().count() > max {
        return Err(format!("The UI's {what} is longer than {max} characters."));
    }
    Ok(Some(s))
}

/// The agent's `for` as stored: each character found by name and given by
/// GUID, each class a known token. Refused in plain words, with the names to
/// use.
fn resolve(db: &Db, flavor: &str, r: &UiForRequest) -> Result<UiFor, String> {
    if r.characters.len() > MAX_TARGETS
        || r.classes.len() > MAX_TARGETS
        || r.specs.len() > MAX_TARGETS
    {
        return Err(format!("A UI's `for` lists at most {MAX_TARGETS} of each."));
    }
    let mut characters = Vec::new();
    for name in &r.characters {
        let card = crate::characters::by_name(db, flavor, name).map_err(|e| match e {
            AppError::NotFound(m) => m,
            e => e.to_string(),
        })?;
        let guid: Option<String> = db
            .with_conn(|c| {
                Ok(c.query_row(
                    "SELECT guid FROM characters WHERE id = ?1",
                    [card.id],
                    |r| r.get(0),
                )?)
            })
            .map_err(|e| e.to_string())?;
        match guid.filter(|g| !g.is_empty()) {
            Some(g) => characters.push(g),
            None => {
                return Err(format!(
                    "{} hasn't logged in with Forever Buddy's addon yet, so the game can't tell it's them. Log in on them once, or make the UI for their class.",
                    crate::characters::full_name(&card)
                ))
            }
        }
    }
    let mut classes = Vec::new();
    for class in &r.classes {
        let token = class
            .trim()
            .to_ascii_uppercase()
            .replace([' ', '_', '-'], "");
        if !CLASSES.contains(&token.as_str()) {
            return Err(format!(
                "No class {class:?}. The classes are {}.",
                CLASSES.join(", ")
            ));
        }
        classes.push(token);
    }
    let mut specs = r.specs.clone();
    if specs.iter().any(|&s| s == 0 || s > 10_000) {
        return Err("A spec is the game's specialization id, e.g. 260 for Outlaw.".into());
    }
    characters.sort();
    characters.dedup();
    classes.sort();
    classes.dedup();
    specs.sort_unstable();
    specs.dedup();
    Ok(UiFor {
        characters,
        classes,
        specs,
    })
}

/// The proposal's lines, its `for`, and the document through the limits
/// and the language's check. A document with problems is refused with them,
/// by JSON pointer, so the agent can fix and send it again.
pub(crate) fn check(db: &Db, flavor: &str, p: &UiDocProposal) -> Result<UiDocBody, String> {
    let name = line("name", &p.name, MAX_NAME, true)?.unwrap_or_default();
    let why = line("why", &p.why, super::MAX_REASON, true)?.unwrap_or_default();
    let words = match &p.words {
        Some(w) => line("player's words", w, super::MAX_REASON, false)?,
        None => None,
    };
    let target = match &p.target {
        Some(r) => resolve(db, flavor, r)?,
        None => UiFor::default(),
    };
    let problems = crate::preview::ui::check(&p.doc)?;
    if !problems.is_empty() {
        return Err(format!(
            "The document has {} problem{}; render_ui shows the same. Nothing was queued.\n{}",
            problems.len(),
            if problems.len() == 1 { "" } else { "s" },
            crate::preview::ui::problem_lines(&problems)
        ));
    }
    Ok(UiDocBody {
        name,
        why,
        words,
        target,
        doc: p.doc.clone(),
    })
}

/// One document in use.
pub(crate) struct InUse {
    pub id: i64,
    pub target: UiFor,
    pub set_at: i64,
    pub body: UiDocBody,
}

/// The slot's order: most specific first, the newest first within that.
fn order(all: &mut [InUse]) {
    all.sort_by(|a, b| (a.target.rank(), b.set_at, b.id).cmp(&(b.target.rank(), a.set_at, a.id)));
}

/// The documents in use, in the slot's order. A row whose proposal no
/// longer reads is left out.
pub(crate) fn in_use(db: &Db, flavor: &str) -> AppResult<Vec<InUse>> {
    let rows: Vec<(String, i64, i64, Option<String>)> = db.with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT u.target, u.proposal, u.set_at, p.body FROM ui_in_use u
             JOIN proposals p ON p.id = u.proposal AND p.flavor = u.flavor
                              AND p.kind = ?2 AND p.status = 'applied'
             WHERE u.flavor = ?1",
        )?;
        let rows = stmt
            .query_map(params![flavor, super::UI_DOC], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })?;
    let mut all: Vec<InUse> = rows
        .into_iter()
        .filter_map(|(target, id, set_at, body)| {
            Some(InUse {
                id,
                target: serde_json::from_str(&target).ok()?,
                set_at,
                body: serde_json::from_str(body.as_deref()?).ok()?,
            })
        })
        .collect();
    order(&mut all);
    Ok(all)
}

/// Makes proposal `id` the UI in use for `target`, in place of whatever was
/// for the same `target`. Refused past `MAX_IN_USE`.
pub(crate) fn use_for(db: &Db, flavor: &str, id: i64, target: &UiFor, now: i64) -> AppResult<()> {
    let key = target.key();
    db.with_conn(|c| {
        let (count, same): (i64, i64) = c.query_row(
            "SELECT count(*), coalesce(sum(target = ?2), 0) FROM ui_in_use WHERE flavor = ?1",
            params![flavor, key],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if same == 0 && count as usize >= MAX_IN_USE {
            return Err(AppError::InvalidSettings(format!(
                "{MAX_IN_USE} UIs are in use already. Stop using one of them first."
            )));
        }
        c.execute(
            "INSERT INTO ui_in_use (flavor, target, proposal, set_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(flavor, target) DO UPDATE SET proposal = excluded.proposal, set_at = excluded.set_at",
            params![flavor, key, id, now],
        )?;
        Ok(())
    })
}

/// "Go back to the standard HUD": for one `target`, or (`None`) for
/// everyone and every document in use.
pub(crate) fn go_back(db: &Db, flavor: &str, target: Option<&UiFor>) -> AppResult<()> {
    db.with_conn(|c| {
        match target {
            Some(t) => c.execute(
                "DELETE FROM ui_in_use WHERE flavor = ?1 AND target = ?2",
                params![flavor, t.key()],
            )?,
            None => c.execute("DELETE FROM ui_in_use WHERE flavor = ?1", [flavor])?,
        };
        Ok(())
    })
}

/// A `for` as the app shows and picks it (the Approvals card's For row):
/// characters by the app's own id.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct UiForPick {
    pub characters: Vec<u32>,
    pub classes: Vec<String>,
    /// Kept from the agent's proposal; the card doesn't pick specs.
    pub specs: Vec<u32>,
}

/// A character a UI can be for: one the addon has seen, so the game can
/// tell it's them (by GUID).
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct UiCharacter {
    pub id: u32,
    pub name: String,
    /// The class file token, `ROGUE`.
    pub class: Option<String>,
    #[serde(skip)]
    guid: String,
}

/// The characters a UI can be for, by name.
pub fn characters(db: &Db, flavor: &str) -> AppResult<Vec<UiCharacter>> {
    db.with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT id, name, surname, class, guid FROM characters
             WHERE flavor = ?1 AND guid IS NOT NULL AND guid <> '' AND hidden_at IS NULL
             ORDER BY last_seen DESC, id",
        )?;
        let rows = stmt
            .query_map([flavor], |r| {
                let name: String = r.get(1)?;
                let surname: Option<String> = r.get(2)?;
                Ok(UiCharacter {
                    id: r.get(0)?,
                    name: match surname {
                        Some(s) if !s.is_empty() => format!("{name} {s}"),
                        _ => name,
                    },
                    class: r.get(3)?,
                    guid: r.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

/// The card's pick as stored. A character the game can't tell apart, or an
/// unknown class, is refused.
pub fn from_pick(db: &Db, flavor: &str, p: &UiForPick) -> AppResult<UiFor> {
    let known = characters(db, flavor)?;
    let characters = p
        .characters
        .iter()
        .map(|id| {
            known
                .iter()
                .find(|c| c.id == *id)
                .map(|c| c.guid.clone())
                .ok_or_else(|| AppError::NotFound("that character".into()))
        })
        .collect::<AppResult<Vec<_>>>()?;
    resolve(
        db,
        flavor,
        &UiForRequest {
            characters: Vec::new(),
            classes: p.classes.clone(),
            specs: p.specs.clone(),
        },
    )
    .map(|mut t| {
        t.characters = characters;
        t.characters.sort();
        t.characters.dedup();
        t
    })
    .map_err(AppError::InvalidSettings)
}

/// A stored `for` as the card shows it. A character no longer known is left
/// out.
pub fn to_pick(db: &Db, flavor: &str, t: &UiFor) -> AppResult<UiForPick> {
    let known = characters(db, flavor)?;
    Ok(UiForPick {
        characters: t
            .characters
            .iter()
            .filter_map(|g| known.iter().find(|c| &c.guid == g).map(|c| c.id))
            .collect(),
        classes: t.classes.clone(),
        specs: t.specs.clone(),
    })
}

/// One document in use as "Your UIs in the game" lists it, in the order
/// the game picks from.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct UiInUse {
    /// Which entry, for Stop using and its effect.
    pub key: String,
    /// The proposal: its picture is `ui_doc_picture`'s.
    pub id: u32,
    pub name: String,
    #[serde(rename = "for")]
    pub target: UiForPick,
    pub since: String,
}

pub fn listed(db: &Db, flavor: &str) -> AppResult<Vec<UiInUse>> {
    in_use(db, flavor)?
        .into_iter()
        .map(|u| {
            Ok(UiInUse {
                key: u.target.key(),
                id: u.id as u32,
                name: u.body.name,
                target: to_pick(db, flavor, &u.target)?,
                since: super::rfc3339(u.set_at),
            })
        })
        .collect()
}

/// A change to what's in use, to see what it does before doing it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UiChange {
    /// Use proposal `id` for `for`.
    Use {
        id: u32,
        #[serde(rename = "for")]
        target: UiForPick,
    },
    /// Stop using one in use, by its `key`.
    Stop { key: String },
}

/// A `key` from `listed` back to its `for`.
fn by_key(key: &str) -> AppResult<UiFor> {
    serde_json::from_str(key).map_err(|_| AppError::NotFound("that UI in use".into()))
}

/// Stop using: the entry `key` from `listed`.
pub fn stop(db: &Db, flavor: &str, key: &str) -> AppResult<()> {
    go_back(db, flavor, Some(&by_key(key)?))
}

/// What a change does to one character: the UI it has at its next login
/// now and after (`None` is the standard HUD), and whether the change's
/// `for` takes it in (so "your rogues keep Rogue timers" can be said).
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct UiEffect {
    pub character: UiCharacter,
    pub now: Option<String>,
    pub then: Option<String>,
    pub in_for: bool,
}

/// Whether a `for` takes in a character. The app doesn't know a
/// character's spec, so a `for` with specs takes in no one here (the game
/// decides those).
fn takes(t: &UiFor, c: &UiCharacter) -> bool {
    t.specs.is_empty()
        && (t.characters.is_empty() || t.characters.contains(&c.guid))
        && (t.classes.is_empty() || c.class.as_ref().is_some_and(|k| t.classes.contains(k)))
}

/// The UI a character gets from `all`, as the addon picks: the first that
/// takes it in.
fn picks(all: &[InUse], c: &UiCharacter) -> Option<String> {
    all.iter()
        .find(|u| takes(&u.target, c))
        .map(|u| u.body.name.clone())
}

/// Every character the change touches or whose `for` takes in, before and
/// after, for the card's line in names.
pub fn effect(db: &Db, flavor: &str, change: &UiChange, now: i64) -> AppResult<Vec<UiEffect>> {
    let before = in_use(db, flavor)?;
    let mut after = in_use(db, flavor)?;
    let target = match change {
        UiChange::Use { id, target } => {
            let t = from_pick(db, flavor, target)?;
            let body: UiDocBody = serde_json::from_str(&db.with_conn(|c| {
                Ok(c.query_row(
                    "SELECT body FROM proposals WHERE id = ?1 AND flavor = ?2 AND kind = ?3",
                    params![id, flavor, super::UI_DOC],
                    |r| r.get::<_, String>(0),
                )?)
            })?)
            .map_err(|_| AppError::NotFound("that UI".into()))?;
            after.retain(|u| u.target != t);
            after.push(InUse {
                id: i64::from(*id),
                target: t.clone(),
                set_at: now,
                body,
            });
            t
        }
        UiChange::Stop { key } => {
            let t = by_key(key)?;
            after.retain(|u| u.target != t);
            t
        }
    };
    order(&mut after);
    Ok(characters(db, flavor)?
        .into_iter()
        .filter_map(|c| {
            let (was, will) = (picks(&before, &c), picks(&after, &c));
            let in_for = takes(&target, &c);
            (in_for || was != will).then_some(UiEffect {
                now: was,
                then: will,
                in_for,
                character: c,
            })
        })
        .collect())
}

/// The document of `ui_doc` proposal `id`, whatever became of it, for its
/// picture.
pub fn doc(db: &Db, flavor: &str, id: u32) -> AppResult<Option<Value>> {
    let body: Option<String> = db.with_conn(|c| {
        use rusqlite::OptionalExtension;
        Ok(c.query_row(
            "SELECT body FROM proposals WHERE id = ?1 AND flavor = ?2 AND kind = ?3",
            params![id, flavor, super::UI_DOC],
            |r| r.get(0),
        )
        .optional()?
        .flatten())
    })?;
    Ok(body
        .and_then(|b| serde_json::from_str::<UiDocBody>(&b).ok())
        .map(|b| b.doc))
}

/// The UI in use for everyone, else the first in use, for the Approvals
/// bar. (Per-entry Go back comes with its design.)
pub fn active_ui(db: &Db, flavor: &str) -> AppResult<Option<ActiveUi>> {
    let all = in_use(db, flavor)?;
    let pick = all
        .iter()
        .find(|u| u.target.is_everyone())
        .or_else(|| all.first());
    Ok(pick.map(|u| ActiveUi {
        id: u.id as u32,
        name: u.body.name.clone(),
    }))
}

/// A `for` as the agent reads it: characters by name again.
fn described(db: &Db, flavor: &str, t: &UiFor) -> AppResult<Value> {
    let mut names = Vec::new();
    for guid in &t.characters {
        let name: Option<(String, Option<String>)> = db.with_conn(|c| {
            use rusqlite::OptionalExtension;
            Ok(c.query_row(
                "SELECT name, surname FROM characters WHERE flavor = ?1 AND guid = ?2",
                params![flavor, guid],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?)
        })?;
        names.push(match name {
            Some((n, Some(s))) if !s.is_empty() => format!("{n} {s}"),
            Some((n, _)) => n,
            None => guid.clone(),
        });
    }
    Ok(if t.is_everyone() {
        Value::String("everyone".into())
    } else {
        serde_json::json!({ "characters": names, "classes": t.classes, "specs": t.specs })
    })
}

/// UI3's `get_ui`: the documents in use (`id`, `name`, `why`, `for`,
/// `doc`), in the order the game tries them, and every UI proposed in the
/// last 30 days, newest first, each with its status in the player's words
/// and, when it was queued, its document.
pub fn history(db: &Db, flavor: &str, now: i64) -> AppResult<(Vec<Value>, Vec<Value>)> {
    let using = in_use(db, flavor)?;
    let mut in_use_now = Vec::new();
    for u in &using {
        in_use_now.push(serde_json::json!({
            "id": u.id, "name": u.body.name, "why": u.body.why,
            "for": described(db, flavor, &u.target)?, "doc": u.body.doc,
        }));
    }
    let since = now - super::DECIDED_DAYS * 86_400;
    let mut recent = Vec::new();
    for s in super::stored(
        db,
        flavor,
        "kind = ? AND (status = 'staged' OR coalesce(decided_at, received_at) >= ?)",
        &[&super::UI_DOC, &since],
    )?
    .into_iter()
    .take(20)
    {
        let b = super::body::<UiDocBody>(&s, super::UI_DOC);
        let target = match &b {
            Some(b) => described(db, flavor, &b.target)?,
            None => Value::Null,
        };
        recent.push(serde_json::json!({
            "id": s.id,
            "status": match s.status.as_str() {
                "staged" => "waiting",
                "applied" if using.iter().any(|u| u.id == s.id) => "in use",
                "applied" => "used before",
                "discarded" => "not this one",
                _ => "not queued",
            },
            "why_not_queued": s.status_reason,
            "proposed_at": super::rfc3339(s.created_at),
            "decided_at": s.decided_at.map(super::rfc3339),
            "name": b.as_ref().map(|b| b.name.clone()),
            "why": b.as_ref().map(|b| b.why.clone()),
            "for": target,
            "doc": b.map(|b| b.doc),
        }));
    }
    Ok((in_use_now, recent))
}

pub(super) fn view(db: &Db, flavor: &str, b: &UiDocBody, active: bool) -> AppResult<UiDocView> {
    Ok(UiDocView {
        name: b.name.clone(),
        why: b.why.clone(),
        words: b.words.clone(),
        target: to_pick(db, flavor, &b.target)?,
        active,
    })
}

pub(super) fn is_active(db: &Db, flavor: &str, id: i64) -> AppResult<bool> {
    db.with_conn(|c| {
        Ok(c.query_row(
            "SELECT count(*) FROM ui_in_use WHERE flavor = ?1 AND proposal = ?2",
            params![flavor, id],
            |r| r.get::<_, i64>(0),
        )? > 0)
    })
}

/// A JSON value as slot data: objects keyed by their own names, lists as
/// sequences. The bridge's own check (`bridge::render`) holds the result to
/// its caps and reads it back.
fn lua(v: &Value) -> LuaValue {
    match v {
        Value::Null => LuaValue::Nil,
        Value::Bool(b) => LuaValue::Bool(*b),
        Value::Number(n) => match n.as_i64() {
            Some(i) => LuaValue::Int(i),
            None => LuaValue::Num(n.as_f64().unwrap_or(0.0)),
        },
        Value::String(s) => LuaValue::str(s),
        Value::Array(items) => LuaValue::Table(Box::new(LuaTable {
            array: items.iter().map(lua).collect(),
            hash: Vec::new(),
        })),
        Value::Object(map) => LuaValue::Table(Box::new(LuaTable {
            array: Vec::new(),
            hash: map
                .iter()
                .map(|(k, v)| (LuaValue::str(k), lua(v)))
                .collect(),
        })),
    }
}

fn table(hash: Vec<(LuaValue, LuaValue)>) -> LuaValue {
    LuaValue::Table(Box::new(LuaTable {
        array: Vec::new(),
        hash,
    }))
}

/// The `UI` slot's entries while any document is in use (nothing, the
/// standard HUD, otherwise): `docs`, each `{ name, for, doc }` in the order
/// the addon tries them (`for` left out for everyone), and for addons
/// before UI3 everyone's document again as `name` and `doc`.
pub fn slot_entries(db: &Db, flavor: &str) -> AppResult<Vec<(LuaValue, LuaValue)>> {
    let all = in_use(db, flavor)?;
    if all.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    if let Some(u) = all.iter().find(|u| u.target.is_everyone()) {
        out.push((LuaValue::str("name"), LuaValue::str(&u.body.name)));
        out.push((LuaValue::str("doc"), lua(&u.body.doc)));
    }
    let docs = all
        .iter()
        .map(|u| {
            let mut e = vec![(LuaValue::str("name"), LuaValue::str(&u.body.name))];
            if !u.target.is_everyone() {
                let t = serde_json::to_value(&u.target).unwrap_or(Value::Null);
                e.push((LuaValue::str("for"), lua(&t)));
            }
            e.push((LuaValue::str("doc"), lua(&u.body.doc)));
            table(e)
        })
        .collect();
    out.push((
        LuaValue::str("docs"),
        LuaValue::Table(Box::new(LuaTable {
            array: docs,
            hash: Vec::new(),
        })),
    ));
    Ok(out)
}
