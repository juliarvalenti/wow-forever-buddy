//! Shopping list proposals (P2c, IMPLEMENTING §15 and §17): a new list, or
//! items added to or changed on one of the player's lists. Applied through
//! B2's own `lists::create_list` and `lists::add_item`. An agent never
//! removes anything: a change only adds an item or sets its need.

use std::collections::HashSet;

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::characters;
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::lists::{self, NewItem, Who};

const MAX_NAME: usize = 60;
const MAX_ITEMS: usize = 100;
const MAX_NEED: u32 = 9999;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposedItem {
    pub item_id: u32,
    pub need: u32,
}

/// The `propose_list_change` tool's arguments, in the inbox file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListProposal {
    /// One of the player's lists by name, or the name of a new one.
    pub list: String,
    /// For a new list: the character it's for.
    #[serde(default)]
    pub for_character: Option<String>,
    pub items: Vec<ProposedItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ListBody {
    /// `None`: a new list.
    list_id: Option<u32>,
    name: String,
    for_character_id: Option<u32>,
    items: Vec<ProposedItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct ListChange {
    pub item_id: u32,
    pub name: String,
    pub quality: Option<u8>,
    pub icon_file_id: Option<u32>,
    pub need: u32,
    /// The need on the list now; `None` means it's added.
    pub was: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct ListView {
    pub list_id: Option<u32>,
    pub name: String,
    /// A new list's character.
    pub for_character: Option<Who>,
    /// The list was deleted after the agent proposed changes to it; Approve
    /// refuses.
    pub gone: bool,
    /// Only the items that change: unchanged needs are left out.
    pub changes: Vec<ListChange>,
}

fn err(e: AppError) -> String {
    match e {
        AppError::NotFound(m) | AppError::InvalidSettings(m) => m,
        e => e.to_string(),
    }
}

/// An item's name, quality and icon, from what the characters have seen.
type SeenItem = (String, Option<u8>, Option<u32>);

fn item_row(db: &Db, id: u32) -> AppResult<Option<SeenItem>> {
    db.with_conn(|c| {
        Ok(c.query_row(
            "SELECT name, quality, icon_file_id FROM items WHERE item_id = ?1 AND name IS NOT NULL",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?)
    })
}

pub(crate) fn check(db: &Db, flavor: &str, p: &ListProposal) -> Result<ListBody, String> {
    let name = p.list.trim().to_string();
    if name.is_empty() || name.chars().count() > MAX_NAME || name.contains(|c: char| c.is_control())
    {
        return Err(format!("a list name is 1 to {MAX_NAME} characters"));
    }
    if p.items.is_empty() || p.items.len() > MAX_ITEMS {
        return Err(format!("1 to {MAX_ITEMS} items"));
    }
    let mut seen = HashSet::new();
    for i in &p.items {
        if !(1..=MAX_NEED).contains(&i.need) {
            return Err(format!("a need is 1 to {MAX_NEED}"));
        }
        if !seen.insert(i.item_id) {
            return Err(format!("item {} is listed twice", i.item_id));
        }
        if item_row(db, i.item_id).map_err(err)?.is_none() {
            return Err(format!("item {} isn't one we know", i.item_id));
        }
    }
    let existing = lists::lists(db, flavor)
        .map_err(err)?
        .into_iter()
        .find(|l| l.name.eq_ignore_ascii_case(&name));
    let for_character_id = match (&existing, &p.for_character) {
        (_, None) => None,
        (Some(_), Some(_)) => {
            return Err("who a list is for can only be set on a new list".into());
        }
        (None, Some(who)) => Some(characters::by_name(db, flavor, who).map_err(err)?.id),
    };
    Ok(ListBody {
        list_id: existing.as_ref().map(|l| l.id),
        name: existing.map(|l| l.name).unwrap_or(name),
        for_character_id,
        items: p.items.clone(),
    })
}

pub(super) fn view(db: &Db, flavor: &str, b: &ListBody) -> AppResult<ListView> {
    let all = lists::lists(db, flavor)?;
    let current = b
        .list_id
        .and_then(|id| all.into_iter().find(|l| l.id == id));
    let for_character = match b.for_character_id {
        None => None,
        Some(id) => characters::overview(db, flavor)?
            .characters
            .into_iter()
            .find(|c| c.id == id)
            .map(|c| Who {
                id: c.id,
                name: characters::full_name(&c),
                class: c.class.unwrap_or_default(),
            }),
    };
    let mut changes = Vec::new();
    for i in &b.items {
        let was = current
            .as_ref()
            .and_then(|l| l.items.iter().find(|x| x.item_id == Some(i.item_id)))
            .map(|x| x.need);
        if was == Some(i.need) {
            continue;
        }
        let (name, quality, icon_file_id) =
            item_row(db, i.item_id)?.unwrap_or((format!("Item {}", i.item_id), None, None));
        changes.push(ListChange {
            item_id: i.item_id,
            name,
            quality,
            icon_file_id,
            need: i.need,
            was,
        });
    }
    Ok(ListView {
        list_id: b.list_id,
        name: current
            .as_ref()
            .map(|l| l.name.clone())
            .unwrap_or_else(|| b.name.clone()),
        for_character,
        gone: b.list_id.is_some() && current.is_none(),
        changes,
    })
}

/// Approve: create the list if it's new, then add or set each item.
pub(super) fn apply(db: &Db, flavor: &str, b: &ListBody, producer: &str) -> AppResult<()> {
    let view = view(db, flavor, b)?;
    if view.gone {
        return Err(AppError::NotFound(
            "that list: it was deleted after the agent suggested this".into(),
        ));
    }
    let list_id = match b.list_id {
        Some(id) => id,
        None => lists::create_list(
            db,
            flavor,
            &b.name,
            b.for_character_id,
            &format!("agent:{producer}"),
        )?,
    };
    for c in &view.changes {
        lists::add_item(db, list_id, &NewItem::Id(c.item_id), c.need)?;
    }
    Ok(())
}
