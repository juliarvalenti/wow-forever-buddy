//! UI drafts (ED1): a look for the HUD and the action bars that Claude made
//! for the player to try on. A draft is a named patch of display-only
//! `hud.*` and `actionbars.*` settings, with why and the player's own words.
//! It's kept in the app and drawn on the Your UI tab, never sent to the
//! game: "Use this" is the approval, and applies it through the normal
//! settings write (`addon_settings::approve_all`, one profile); "Throw away"
//! declines it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::addon_settings::{self, Opt, SettingBody, SettingView};
use crate::db::Db;
use crate::error::AppResult;

/// A draft's name: a few words, "Stacked, blue".
pub const MAX_NAME: usize = 40;
/// Drafts waiting at once; past this, new ones are refused.
pub const MAX_STAGED: i64 = 10;

/// The `propose_ui_draft` tool's arguments, in the inbox file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DraftProposal {
    pub name: String,
    pub why: String,
    /// What the player asked for, in their words.
    #[serde(default)]
    pub words: Option<String>,
    /// Option id → value.
    pub settings: BTreeMap<String, Value>,
}

/// After checking: what's stored, drawn and applied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DraftBody {
    pub name: String,
    pub why: String,
    pub words: Option<String>,
    /// In the schema's order.
    pub changes: Vec<SettingBody>,
}

/// A draft as the Your UI tab shows it.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct UiDraft {
    pub id: u32,
    pub name: String,
    pub why: String,
    /// What the player asked for, in their words, as the agent passed them on.
    pub words: Option<String>,
    /// Exactly as the client reported it. Shown quoted, as a claim.
    pub producer: String,
    /// RFC 3339.
    pub created_at: String,
    /// Each change: our label, and what it is now and would be.
    pub changes: Vec<SettingView>,
}

/// Whether a draft may change this option: the HUD's or the bars', and
/// display only. Everything else goes through `propose_settings`.
pub(crate) fn in_draft(o: &Opt) -> Result<(), String> {
    if !(o.id.starts_with("hud.") || o.id.starts_with("actionbars.")) {
        return Err(format!(
            "{} isn't part of the HUD or the action bars: a draft only changes how those look. propose_settings suggests the rest.",
            o.id
        ));
    }
    if o.acts {
        return Err(format!(
            "{} lets the addon act in the game, so it can't be in a draft.",
            o.id
        ));
    }
    Ok(())
}

/// A line of the agent's, plain and within `max` characters.
fn line(what: &str, s: &str, max: usize, required: bool) -> Result<Option<String>, String> {
    let s = super::plain_line(s);
    if s.is_empty() {
        return if required {
            Err(format!("The draft's {what} is empty."))
        } else {
            Ok(None)
        };
    }
    if s.chars().count() > max {
        return Err(format!(
            "The draft's {what} is longer than {max} characters."
        ));
    }
    Ok(Some(s))
}

pub(crate) fn check(db: &Db, flavor: &str, p: &DraftProposal) -> Result<DraftBody, String> {
    let name = line("name", &p.name, MAX_NAME, true)?.unwrap_or_default();
    let why = line("why", &p.why, super::MAX_REASON, true)?.unwrap_or_default();
    let words = match &p.words {
        Some(w) => line("player's words", w, super::MAX_REASON, false)?,
        None => None,
    };
    if p.settings.is_empty() || p.settings.len() > addon_settings::MAX_CHANGES {
        return Err(format!(
            "A draft changes 1 to {} settings.",
            addon_settings::MAX_CHANGES
        ));
    }
    let mut changes = Vec::new();
    for (id, value) in &p.settings {
        let o = addon_settings::option(id)
            .ok_or_else(|| format!("No setting called {id:?}: get_settings lists them."))?;
        in_draft(o)?;
        if !addon_settings::fits(o, value) {
            return Err(format!("{} takes {}", o.id, addon_settings::allowed(o)));
        }
        changes.push(SettingBody {
            option: o.id.clone(),
            value: value.clone(),
        });
    }
    // An option only works while its parent is on: on now, or in the draft.
    let now = addon_settings::values_now(db, flavor).map_err(|e| e.to_string())?;
    for b in &changes {
        let Some(parent) = addon_settings::option(&b.option).and_then(|o| o.parent.as_deref())
        else {
            continue;
        };
        let turned = changes
            .iter()
            .find(|x| x.option == parent)
            .map(|x| &x.value);
        if turned.or(now.get(parent)) != Some(&Value::Bool(true)) {
            return Err(format!(
                "{} only works while {parent} is on: turn that on in the same draft",
                b.option
            ));
        }
    }
    // In the schema's order, as the Your UI tab lists them.
    let order = |id: &str| {
        addon_settings::schema()
            .options
            .iter()
            .position(|o| o.id == id)
    };
    changes.sort_by_key(|b| order(&b.option));
    Ok(DraftBody {
        name,
        why,
        words,
        changes,
    })
}

pub(super) fn view(
    db: &Db,
    flavor: &str,
    id: u32,
    producer: &str,
    created_at: String,
    b: &DraftBody,
) -> UiDraft {
    UiDraft {
        id,
        name: b.name.clone(),
        why: b.why.clone(),
        words: b.words.clone(),
        producer: producer.to_string(),
        created_at,
        // A setting this version no longer declares isn't listed (and Use
        // this refuses it).
        changes: b
            .changes
            .iter()
            .filter_map(|c| addon_settings::view(db, flavor, c).ok())
            .collect(),
    }
}

/// "Use this": every change in one new profile, through the normal write.
pub(super) fn apply(db: &Db, flavor: &str, b: &DraftBody, now: i64) -> AppResult<()> {
    addon_settings::approve_all(db, flavor, &b.changes, now)
}
