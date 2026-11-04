//! UI1: a whole UI document Claude proposes (`propose_ui`). It waits in
//! Approvals with its picture; "Use this" makes it the account's UI, which
//! the app sends to the game in the `UI` slot (`slot_entries`), and the
//! addon draws it at the next login instead of the standard HUD. "Go back to
//! the standard HUD" clears it. One document is in use at a time, per game
//! folder flavor; per character is a later step.
//!
//! The document is checked twice, by the tool and again on pickup: the
//! render limits and the language's own check (`preview::ui::check`). The
//! stored body is exactly what's drawn and sent.

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::Db;
use crate::error::AppResult;
use crate::sv::{LuaTable, LuaValue};

/// A document's name: a few words, "Halo".
pub const MAX_NAME: usize = 40;

/// The `propose_ui` tool's arguments, in the inbox file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UiDocProposal {
    pub name: String,
    pub why: String,
    /// What the player asked for, in their words.
    #[serde(default)]
    pub words: Option<String>,
    pub doc: Value,
}

/// After checking: what's stored, drawn and sent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct UiDocBody {
    pub name: String,
    pub why: String,
    pub words: Option<String>,
    pub doc: Value,
}

/// A document as Approvals shows it (its picture comes from
/// `ui_doc_picture`).
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct UiDocView {
    pub name: String,
    pub why: String,
    pub words: Option<String>,
    /// It's the UI in use now.
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

/// The proposal's lines, and the document through the limits and the
/// language's check. A document with problems is refused with them, by
/// JSON pointer, so the agent can fix and send it again.
pub(crate) fn check(p: &UiDocProposal) -> Result<UiDocBody, String> {
    let name = line("name", &p.name, MAX_NAME, true)?.unwrap_or_default();
    let why = line("why", &p.why, super::MAX_REASON, true)?.unwrap_or_default();
    let words = match &p.words {
        Some(w) => line("player's words", w, super::MAX_REASON, false)?,
        None => None,
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
        doc: p.doc.clone(),
    })
}

fn key(flavor: &str) -> String {
    format!("ui_doc:{flavor}")
}

/// The proposal in use, if any (its id in `meta`).
fn active_id(db: &Db, flavor: &str) -> AppResult<Option<i64>> {
    db.with_conn(|c| {
        Ok(c.query_row(
            "SELECT value FROM meta WHERE key = ?1",
            params![key(flavor)],
            |r| r.get::<_, String>(0),
        )
        .optional()?
        .and_then(|v| v.parse().ok()))
    })
}

/// Makes proposal `id` the UI in use, or (`None`) goes back to the
/// standard HUD.
pub(crate) fn set_active(db: &Db, flavor: &str, id: Option<i64>) -> AppResult<()> {
    db.with_conn(|c| {
        match id {
            Some(id) => c.execute(
                "INSERT INTO meta (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key(flavor), id.to_string()],
            )?,
            None => c.execute("DELETE FROM meta WHERE key = ?1", params![key(flavor)])?,
        };
        Ok(())
    })
}

/// The document in use and its proposal's id: an applied `ui_doc` row.
pub(crate) fn active(db: &Db, flavor: &str) -> AppResult<Option<(i64, UiDocBody)>> {
    let Some(id) = active_id(db, flavor)? else {
        return Ok(None);
    };
    let body: Option<String> = db.with_conn(|c| {
        Ok(c.query_row(
            "SELECT body FROM proposals WHERE id = ?1 AND flavor = ?2 AND kind = ?3 AND status = 'applied'",
            params![id, flavor, super::UI_DOC],
            |r| r.get(0),
        )
        .optional()?
        .flatten())
    })?;
    Ok(body
        .and_then(|b| serde_json::from_str(&b).ok())
        .map(|b| (id, b)))
}

/// The document of `ui_doc` proposal `id`, whatever became of it, for its
/// picture.
pub fn doc(db: &Db, flavor: &str, id: u32) -> AppResult<Option<Value>> {
    let body: Option<String> = db.with_conn(|c| {
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

pub fn active_ui(db: &Db, flavor: &str) -> AppResult<Option<ActiveUi>> {
    Ok(active(db, flavor)?.map(|(id, b)| ActiveUi {
        id: id as u32,
        name: b.name,
    }))
}

pub(super) fn view(b: &UiDocBody, active: bool) -> UiDocView {
    UiDocView {
        name: b.name.clone(),
        why: b.why.clone(),
        words: b.words.clone(),
        active,
    }
}

pub(super) fn is_active(db: &Db, flavor: &str, id: i64) -> AppResult<bool> {
    Ok(active_id(db, flavor)? == Some(id))
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

/// The `UI` slot's entries: `name` and `doc` while a document is in use;
/// nothing (the standard HUD) otherwise.
pub fn slot_entries(db: &Db, flavor: &str) -> AppResult<Vec<(LuaValue, LuaValue)>> {
    Ok(match active(db, flavor)? {
        Some((_, b)) => vec![
            (LuaValue::str("name"), LuaValue::str(&b.name)),
            (LuaValue::str("doc"), lua(&b.doc)),
        ],
        None => Vec::new(),
    })
}
