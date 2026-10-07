//! Bag mark proposals (B3b, IMPLEMENTING §18 and §17): an agent suggests
//! marking some of one character's items to sell, or to send to another of
//! the player's characters. Applied through B3's own `cleanup::put`, with
//! producer "agent:<client>". The reason is one of the app's fixed codes,
//! and the app checks it against its own rules: an agent can't call
//! something a grey that isn't, or an upgrade the app doesn't see.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::characters;
use crate::cleanup::{self, Mark, Marked, Reason};
use crate::db::Db;
use crate::error::{AppError, AppResult};

const MAX_MARKS: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposedAction {
    Sell,
    Send,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposedReason {
    Grey,
    Outgrown,
    Upgrade,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposedMark {
    pub item_id: u32,
    pub action: ProposedAction,
    /// For send: the character's name.
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default)]
    pub reason: Option<ProposedReason>,
}

/// The `propose_bag_marks` tool's arguments, in the inbox file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BagProposal {
    pub character: String,
    pub marks: Vec<ProposedMark>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BagMark {
    item_id: u32,
    mark: Mark,
    reason: Option<Reason>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BagBody {
    character_id: u32,
    marks: Vec<BagMark>,
}

/// "Bag marks for Thrandor": the same rows the panel shows, with reasons.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct BagMarksView {
    pub character_id: u32,
    pub character: String,
    /// File token, lowercase, for the class colour.
    pub class: Option<String>,
    pub rows: Vec<Marked>,
    /// Items the character no longer holds: left out when approved.
    pub gone: Vec<u32>,
}

fn err(e: AppError) -> String {
    match e {
        AppError::NotFound(m) | AppError::InvalidSettings(m) => m,
        e => e.to_string(),
    }
}

pub(crate) fn check(db: &Db, flavor: &str, p: &BagProposal) -> Result<BagBody, String> {
    if p.marks.is_empty() || p.marks.len() > MAX_MARKS {
        return Err(format!("1 to {MAX_MARKS} marks"));
    }
    let who = characters::by_name(db, flavor, &p.character).map_err(err)?;
    // Recipients by name, looked up before the connection is held (by_name
    // takes it too).
    let mut recipients = std::collections::HashMap::new();
    for name in p.marks.iter().filter_map(|m| m.to.as_ref()) {
        if !recipients.contains_key(name) {
            let to = characters::by_name(db, flavor, name).map_err(err)?;
            recipients.insert(name.clone(), to.id);
        }
    }
    let mut seen = HashSet::new();
    let mut marks = Vec::new();
    db.with_conn(|c| {
        // The app's own reading of every bag item, marked or not.
        let rules = cleanup::suggest_from(c, who.id, true)?;
        for m in &p.marks {
            if !seen.insert(m.item_id) {
                return Ok(Err(format!("item {} is listed twice", m.item_id)));
            }
            let held: (i64, i64) = c.query_row(
                "SELECT count(*), coalesce(max(bound), 0) FROM char_items
                 WHERE character_id = ?1 AND item_id = ?2 AND location IN ('bag', 'bank')",
                rusqlite::params![who.id, m.item_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            if held.0 == 0 {
                return Ok(Err(format!("{} doesn't hold item {}", who.name, m.item_id)));
            }
            let mark = match (m.action, &m.to) {
                (ProposedAction::Sell, None) => Mark::Sell,
                (ProposedAction::Sell, Some(_)) => return Ok(Err("a sell mark has no 'to'".into())),
                (ProposedAction::Send, None) => return Ok(Err("a send mark needs 'to'".into())),
                (ProposedAction::Send, Some(name)) => {
                    let to = recipients[name];
                    if to == who.id {
                        return Ok(Err("send to another character".into()));
                    }
                    if held.1 != 0 {
                        return Ok(Err(format!("item {} is soulbound, so it can't be mailed", m.item_id)));
                    }
                    Mark::Send { to }
                }
            };
            // A reason must be what the app itself would say for this item.
            let app = rules.iter().find(|(id, _, _)| *id == m.item_id);
            let reason = match m.reason {
                None => None,
                Some(r) => match (r, app) {
                    (ProposedReason::Grey, Some((_, Mark::Sell, Reason::Grey))) if mark == Mark::Sell => {
                        Some(Reason::Grey)
                    }
                    (ProposedReason::Outgrown, Some((_, Mark::Sell, Reason::Outgrown))) if mark == Mark::Sell => {
                        Some(Reason::Outgrown)
                    }
                    (ProposedReason::Upgrade, Some((_, to, Reason::Upgrade { gain }))) if *to == mark => {
                        Some(Reason::Upgrade { gain: *gain })
                    }
                    _ => {
                        return Ok(Err(format!(
                            "the app doesn't see item {} as that; leave the reason out or use the one it gives",
                            m.item_id
                        )))
                    }
                },
            };
            marks.push(BagMark {
                item_id: m.item_id,
                mark,
                reason,
            });
        }
        Ok(Ok(()))
    })
    .map_err(err)??;
    Ok(BagBody {
        character_id: who.id,
        marks,
    })
}

/// The preview. Its rows aren't marks yet, so they carry no producer line
/// ("from …, approved" is for applied marks).
pub(super) fn view(db: &Db, b: &BagBody) -> AppResult<BagMarksView> {
    db.with_conn(|c| {
        let (name, class): (String, Option<String>) = c.query_row(
            "SELECT name, lower(class) FROM characters WHERE id = ?1",
            [b.character_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let mut rows = Vec::new();
        let mut gone = Vec::new();
        for m in &b.marks {
            let row = cleanup::row(c, b.character_id, m.item_id, m.mark, m.reason, "proposed")?;
            if row.count == 0 {
                gone.push(m.item_id);
            }
            rows.push(row);
        }
        Ok(BagMarksView {
            character_id: b.character_id,
            character: name,
            class,
            rows,
            gone,
        })
    })
}

/// Approve: each mark through B3's own path. Items the character no longer
/// holds are left out.
pub(super) fn apply(db: &Db, b: &BagBody, producer: &str) -> AppResult<()> {
    let producer = format!("agent:{producer}");
    db.with_conn(|c| {
        let tx = c.transaction()?;
        for m in &b.marks {
            let held: i64 = tx.query_row(
                "SELECT count(*) FROM char_items
                 WHERE character_id = ?1 AND item_id = ?2 AND location IN ('bag', 'bank')",
                rusqlite::params![b.character_id, m.item_id],
                |r| r.get(0),
            )?;
            if held > 0 {
                cleanup::put(&tx, b.character_id, m.item_id, m.mark, m.reason, &producer)?;
            }
        }
        tx.commit()?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLAVOR: &str = "_classic_beta_";

    fn mark(
        item: u32,
        action: ProposedAction,
        to: Option<&str>,
        reason: Option<ProposedReason>,
    ) -> ProposedMark {
        ProposedMark {
            item_id: item,
            action,
            to: to.map(str::to_string),
            reason,
        }
    }

    fn proposal(marks: Vec<ProposedMark>) -> BagProposal {
        BagProposal {
            character: "Thrandor".into(),
            marks,
        }
    }

    /// cleanup's fixture: the Broken Fangs are grey, the shoulders a +43
    /// for Kaelor, the plate helm outgrown, the Hearthstone soulbound.
    #[test]
    fn checked_against_the_apps_own_rules() {
        let db = crate::cleanup::tests::db();
        use ProposedAction::*;
        use ProposedReason::*;
        let ok = check(
            &db,
            FLAVOR,
            &proposal(vec![
                mark(3299, Sell, None, Some(Grey)),
                mark(16000, Send, Some("Kaelor"), Some(Upgrade)),
                mark(2001, Sell, None, None),
            ]),
        )
        .unwrap();
        assert_eq!(
            ok.marks[1].reason,
            Some(Reason::Upgrade { gain: 43 }),
            "the app's gain, not the agent's"
        );

        for (bad, why) in [
            (vec![mark(16000, Sell, None, Some(Grey))], "not a grey"),
            (
                vec![mark(16000, Send, Some("Sela"), Some(Upgrade))],
                "not an upgrade for Sela",
            ),
            (vec![mark(6948, Send, Some("Sela"), None)], "soulbound"),
            (vec![mark(16000, Send, Some("Thrandor"), None)], "to itself"),
            (vec![mark(12345, Sell, None, None)], "not held"),
            (
                vec![mark(3299, Sell, Some("Sela"), None)],
                "a sell with a 'to'",
            ),
            (vec![mark(3299, Send, None, None)], "a send without one"),
            (
                vec![mark(3299, Sell, None, None), mark(3299, Sell, None, None)],
                "twice",
            ),
            (vec![], "none"),
        ] {
            assert!(check(&db, FLAVOR, &proposal(bad)).is_err(), "{why}");
        }
    }

    /// Approve goes through cleanup::put: the marks carry the reasons and
    /// "agent:<client>", and the preview is those same rows.
    #[test]
    fn approve_marks_with_the_agent_as_producer() {
        let db = crate::cleanup::tests::db();
        let body = check(
            &db,
            FLAVOR,
            &proposal(vec![mark(
                16000,
                ProposedAction::Send,
                Some("Kaelor"),
                Some(ProposedReason::Upgrade),
            )]),
        )
        .unwrap();
        let v = view(&db, &body).unwrap();
        assert_eq!(v.character, "Thrandor");
        assert_eq!(
            v.rows[0].producer, "proposed",
            "not 'approved' before it is"
        );
        assert_eq!(v.rows[0].to.as_ref().unwrap().name, "Kaelor");
        assert!(v.gone.is_empty());
        apply(&db, &body, "Claude Desktop").unwrap();
        let marks = crate::cleanup::view(&db, 1).unwrap().marks;
        assert_eq!(marks[0].producer, "agent:Claude Desktop");
        assert_eq!(marks[0].reason, Some(Reason::Upgrade { gain: 43 }));
    }
}
