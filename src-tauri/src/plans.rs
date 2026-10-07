//! Quest plans (P1, INGAME §7): "Tonight's plan" for a character, a short
//! list of steps the player ticks off in game.
//!
//! Plans arrive as proposals in P2's queue and become active here only when
//! the player approves one (`set_plan`, called by the queue's apply step).
//! Active plans, one per character, go to the game in the bridge's Plan
//! slot (`slot_entries`).
//!
//! A producer sends only text, a quest id and a zone per step
//! (docs/specs/agent-mcp.md §3). Where to go comes from the app's own quest
//! log (Q1b): the giver and position recorded when that quest was taken or
//! handed in, so a waypoint is never a position an agent made up. Every
//! string is checked for length here and shown as plain text in game.
//!
//! `set_plan`'s only caller is P2c's approval step, which comes next; until
//! then only tests call it.
#![cfg_attr(not(test), allow(dead_code))]

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::sv::{LuaTable, LuaValue};

/// Limits from the P2 spec (§3).
const MAX_STEPS: usize = 50;
const MAX_STEP_TEXT: usize = 200;
const MAX_TITLE: usize = 120;

/// What finishes a step in game. Without one, a step with a quest id is
/// done when that quest is handed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    Accept,
    TurnIn,
    /// Done by hand: the player ticks it.
    Objective,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct Step {
    pub text: String,
    pub quest_id: Option<u32>,
    pub zone: Option<String>,
    #[serde(default)]
    pub kind: Option<StepKind>,
}

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct Plan {
    pub id: u32,
    pub character_id: u32,
    pub character: String,
    pub title: String,
    pub steps: Vec<Step>,
    /// "app" or "agent:<client name>", as a claim (the client names itself).
    pub producer: String,
    /// When it was approved (RFC 3339, UTC).
    pub created_at: String,
}

fn invalid(why: &str) -> AppError {
    AppError::InvalidSettings(format!("plan: {why}"))
}

fn fits(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.chars().count() <= max
}

fn validate(title: &str, steps: &[Step]) -> AppResult<()> {
    if !fits(title, MAX_TITLE) {
        return Err(invalid("a title of 1 to 120 characters"));
    }
    if steps.is_empty() || steps.len() > MAX_STEPS {
        return Err(invalid("1 to 50 steps"));
    }
    for s in steps {
        if !fits(&s.text, MAX_STEP_TEXT) || !s.zone.as_deref().is_none_or(|z| fits(z, MAX_TITLE)) {
            return Err(invalid("step text of 1 to 200 characters"));
        }
        if matches!(s.kind, Some(StepKind::Accept | StepKind::TurnIn)) && s.quest_id.is_none() {
            return Err(invalid("accept and turn-in steps need a quest id"));
        }
    }
    Ok(())
}

/// Makes this the character's active plan, replacing any earlier one. The
/// approval step calls it; nothing else should.
pub fn set_plan(
    db: &Db,
    character_id: u32,
    title: &str,
    steps: &[Step],
    producer: &str,
) -> AppResult<u32> {
    validate(title, steps)?;
    let json = serde_json::to_string(steps).map_err(|e| invalid(&e.to_string()))?;
    db.with_conn(|c| {
        let tx = c.transaction()?;
        let known: bool = tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM characters WHERE id = ?1)",
            [character_id],
            |r| r.get(0),
        )?;
        if !known {
            return Err(AppError::NotFound(format!("character {character_id}")));
        }
        tx.execute(
            "UPDATE quest_plans SET status = 'replaced'
             WHERE character_id = ?1 AND status = 'active'",
            [character_id],
        )?;
        tx.execute(
            "INSERT INTO quest_plans (character_id, title, steps, producer, status, created_at)
             VALUES (?1, ?2, ?3, ?4, 'active', ?5)",
            params![
                character_id,
                title,
                json,
                producer,
                chrono::Utc::now().to_rfc3339()
            ],
        )?;
        let id = tx.last_insert_rowid() as u32;
        tx.commit()?;
        Ok(id)
    })
}

/// Removes the character's active plan (the sheet's "Clear plan").
pub fn clear_plan(db: &Db, character_id: u32) -> AppResult<()> {
    db.with_conn(|c| {
        c.execute(
            "UPDATE quest_plans SET status = 'cleared'
             WHERE character_id = ?1 AND status = 'active'",
            [character_id],
        )?;
        Ok(())
    })
}

/// The active plans of `flavor`'s characters.
pub fn active(db: &Db, flavor: &str) -> AppResult<Vec<Plan>> {
    db.with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT p.id, p.character_id, ch.name, p.title, p.steps, p.producer, p.created_at
             FROM quest_plans p JOIN characters ch ON ch.id = p.character_id
             WHERE ch.flavor = ?1 AND p.status = 'active'
             ORDER BY p.id",
        )?;
        let rows = stmt
            .query_map([flavor], |r| {
                let steps: String = r.get(4)?;
                Ok(Plan {
                    id: r.get(0)?,
                    character_id: r.get(1)?,
                    character: r.get(2)?,
                    title: r.get(3)?,
                    steps: serde_json::from_str(&steps).unwrap_or_default(),
                    producer: r.get(5)?,
                    created_at: r.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

/// Where a quest was taken or handed in, from the quest log: the newest
/// recorded event of that kind for any of `flavor`'s characters.
struct Place {
    giver: Option<String>,
    map: Option<i64>,
    x: Option<f64>,
    y: Option<f64>,
}

fn place(
    c: &rusqlite::Connection,
    flavor: &str,
    quest: u32,
    event: &str,
) -> AppResult<Option<Place>> {
    let mut stmt = c.prepare(
        "SELECT e.data FROM adventure_events e
         JOIN adventures a ON a.id = e.adventure_id
         JOIN characters ch ON ch.id = a.character_id
         WHERE ch.flavor = ?1 AND e.kind = ?2 AND json_extract(e.data, '$.id') = ?3
         ORDER BY json_extract(e.data, '$.x') IS NULL, e.at DESC LIMIT 1",
    )?;
    let data: Option<String> = stmt
        .query_row(params![flavor, event, quest], |r| r.get(0))
        .optional()?;
    Ok(data.map(|d| {
        let v: serde_json::Value = serde_json::from_str(&d).unwrap_or_default();
        Place {
            giver: v.get("giver").and_then(|g| g.as_str()).map(str::to_string),
            map: v.get("map").and_then(|m| m.as_i64()),
            x: v.get("x").and_then(|n| n.as_f64()),
            y: v.get("y").and_then(|n| n.as_f64()),
        }
    }))
}

fn key(k: &str) -> LuaValue {
    LuaValue::str(k)
}

fn table(array: Vec<LuaValue>, hash: Vec<(LuaValue, LuaValue)>) -> LuaValue {
    LuaValue::Table(Box::new(LuaTable { array, hash }))
}

/// The Plan slot's body: `plans = { … }`, every active plan of `flavor`.
/// The addon picks its character's by name and surname.
pub fn slot_entries(db: &Db, flavor: &str) -> AppResult<Vec<(LuaValue, LuaValue)>> {
    let plans = active(db, flavor)?;
    let rendered = db.with_conn(|c| {
        let mut out = Vec::with_capacity(plans.len());
        for p in &plans {
            let surname: String = c.query_row(
                "SELECT coalesce(surname, '') FROM characters WHERE id = ?1",
                [p.character_id],
                |r| r.get(0),
            )?;
            let mut steps = Vec::with_capacity(p.steps.len());
            for s in &p.steps {
                let kind = s.kind.unwrap_or(if s.quest_id.is_some() {
                    StepKind::TurnIn
                } else {
                    StepKind::Objective
                });
                let mut h = vec![
                    (key("text"), LuaValue::str(&s.text)),
                    (
                        key("kind"),
                        LuaValue::str(match kind {
                            StepKind::Accept => "accept",
                            StepKind::TurnIn => "turn_in",
                            StepKind::Objective => "objective",
                        }),
                    ),
                ];
                if let Some(q) = s.quest_id {
                    h.push((key("quest"), LuaValue::Int(q.into())));
                }
                if let Some(z) = &s.zone {
                    h.push((key("zone"), LuaValue::str(z)));
                }
                // The waypoint, from where we saw this quest taken or handed in.
                let event = match kind {
                    StepKind::Accept => Some("quest_accepted"),
                    StepKind::TurnIn => Some("quest"),
                    StepKind::Objective => None,
                };
                if let (Some(q), Some(event)) = (s.quest_id, event) {
                    if let Some(pl) = place(c, flavor, q, event)? {
                        if let Some(g) = pl.giver {
                            h.push((key("giver"), LuaValue::str(g)));
                        }
                        if let (Some(m), Some(x), Some(y)) = (pl.map, pl.x, pl.y) {
                            h.push((key("map"), LuaValue::Int(m)));
                            h.push((key("x"), LuaValue::Num(x)));
                            h.push((key("y"), LuaValue::Num(y)));
                        }
                    }
                }
                steps.push(table(Vec::new(), h));
            }
            out.push(table(
                Vec::new(),
                vec![
                    (key("id"), LuaValue::Int(p.id.into())),
                    (key("name"), LuaValue::str(&p.character)),
                    (key("surname"), LuaValue::str(surname)),
                    (key("title"), LuaValue::str(&p.title)),
                    (key("steps"), table(steps, Vec::new())),
                ],
            ));
        }
        Ok(out)
    })?;
    Ok(vec![(key("plans"), table(rendered, Vec::new()))])
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLAVOR: &str = "_classic_beta_";

    fn db() -> Db {
        let db = Db::open_in_memory().unwrap();
        db.with_conn(|c| {
            c.execute(
                "INSERT INTO characters (id, flavor, account, group_dir, char_dir, name, surname,
                                         first_seen, last_seen)
                 VALUES (1, ?1, 'ACCOUNT1', '70', 'Thrandor-Vargur', 'Thrandor', 'Vargur', 1, 1)",
                [FLAVOR],
            )?;
            // Q1b's log: The Active Agent (5149) taken from Betina.
            c.execute(
                "INSERT INTO adventures (id, character_id, login) VALUES (1, 1, 100)",
                [],
            )?;
            c.execute(
                "INSERT INTO adventure_events (adventure_id, seq, at, kind, data)
                 VALUES (1, 1, 150, 'quest_accepted',
                         '{\"id\":5149,\"giver\":\"Betina Bigglezink\",\"map\":1423,\"x\":0.811,\"y\":0.594}')",
                [],
            )?;
            Ok(())
        })
        .unwrap();
        db
    }

    fn step(text: &str, quest: Option<u32>, kind: Option<StepKind>) -> Step {
        Step {
            text: text.into(),
            quest_id: quest,
            zone: Some("Eastern Plaguelands".into()),
            kind,
        }
    }

    fn steps() -> Vec<Step> {
        vec![
            step("Take The Active Agent", Some(5149), Some(StepKind::Accept)),
            step("Clear the Scourge camp", None, None),
            step("Hand in The Active Agent", Some(5149), None),
        ]
    }

    fn body(db: &Db) -> LuaTable {
        let mut t = crate::bridge::header(1);
        t.hash.extend(slot_entries(db, FLAVOR).unwrap());
        match crate::bridge::check(
            crate::bridge::Slot::Plan,
            &crate::bridge::render(crate::bridge::Slot::Plan, t).unwrap(),
        )
        .unwrap()
        {
            LuaValue::Table(t) => *t,
            _ => unreachable!(),
        }
    }

    #[test]
    fn an_approved_plan_goes_to_the_slot_with_our_own_waypoints() {
        let db = db();
        set_plan(&db, 1, "Plaguelands loop", &steps(), "agent:Claude Desktop").unwrap();
        let t = body(&db);
        let plans = t.get("plans").unwrap().as_table().unwrap();
        assert_eq!(plans.array.len(), 1);
        let plan = plans.get_index(1).unwrap().as_table().unwrap();
        assert_eq!(
            plan.get("surname").and_then(|v| v.as_bytes()),
            Some(&b"Vargur"[..])
        );
        let steps = plan.get("steps").unwrap().as_table().unwrap();
        let take = steps.get_index(1).unwrap().as_table().unwrap();
        // Giver and position come from the recorded accept, not the producer.
        assert_eq!(
            take.get("giver").and_then(|v| v.as_bytes()),
            Some(&b"Betina Bigglezink"[..])
        );
        assert_eq!(take.get("map"), Some(&LuaValue::Int(1423)));
        let objective = steps.get_index(2).unwrap().as_table().unwrap();
        assert_eq!(
            objective.get("kind").and_then(|v| v.as_bytes()),
            Some(&b"objective"[..])
        );
        // No kind plus a quest id: done on hand-in. No hand-in recorded, so
        // no waypoint rather than a guess.
        let hand_in = steps.get_index(3).unwrap().as_table().unwrap();
        assert_eq!(
            hand_in.get("kind").and_then(|v| v.as_bytes()),
            Some(&b"turn_in"[..])
        );
        assert_eq!(hand_in.get("map"), None);
    }

    #[test]
    fn one_active_plan_per_character() {
        let db = db();
        set_plan(&db, 1, "First", &steps(), "app").unwrap();
        let second = set_plan(&db, 1, "Second", &steps(), "app").unwrap();
        let current = active(&db, FLAVOR).unwrap();
        assert_eq!(current.len(), 1);
        assert_eq!(current[0].id, second);
        clear_plan(&db, 1).unwrap();
        assert!(active(&db, FLAVOR).unwrap().is_empty());
        assert_eq!(
            body(&db)
                .get("plans")
                .unwrap()
                .as_table()
                .unwrap()
                .array
                .len(),
            0
        );
    }

    #[test]
    fn plans_are_checked() {
        let db = db();
        let long = "x".repeat(201);
        assert!(
            set_plan(&db, 1, "", &steps(), "app").is_err(),
            "empty title"
        );
        assert!(set_plan(&db, 1, "T", &[], "app").is_err(), "no steps");
        assert!(
            set_plan(&db, 1, "T", &[step(&long, None, None)], "app").is_err(),
            "long text"
        );
        let many: Vec<Step> = (0..51).map(|i| step(&format!("{i}"), None, None)).collect();
        assert!(
            set_plan(&db, 1, "T", &many, "app").is_err(),
            "too many steps"
        );
        assert!(
            set_plan(
                &db,
                1,
                "T",
                &[step("Take it", None, Some(StepKind::Accept))],
                "app"
            )
            .is_err(),
            "an accept step needs its quest"
        );
        assert!(
            set_plan(&db, 9, "T", &steps(), "app").is_err(),
            "unknown character"
        );
    }

    /// Hostile text is just data in the slot (the bridge's data-only check
    /// passes it) and the addon escapes it at display.
    #[test]
    fn plan_text_is_only_data() {
        let db = db();
        let hostile = vec![step(
            "|Hitem:19019|h[Thunderfury]|h ]] --[[ /run x()",
            None,
            None,
        )];
        set_plan(&db, 1, "\"]] --", &hostile, "agent:x").unwrap();
        let t = body(&db);
        let plan = t
            .get("plans")
            .unwrap()
            .as_table()
            .unwrap()
            .get_index(1)
            .unwrap();
        let text = plan
            .as_table()
            .unwrap()
            .get("steps")
            .unwrap()
            .as_table()
            .unwrap();
        let first = text.get_index(1).unwrap().as_table().unwrap();
        assert_eq!(
            first.get("text").and_then(|v| v.as_bytes()),
            Some(&b"|Hitem:19019|h[Thunderfury]|h ]] --[[ /run x()"[..])
        );
    }
}
