//! The addon's own settings from the app's side (P4b,
//! context/specs/p4-claude-settings §4).
//!
//! - **The schema** is the addon's declaration (`Settings.lua`), generated
//!   into `resources/settings-schema.json` by the harness (P4a) and bundled
//!   here. Nothing outside it can be proposed or sent.
//! - **What the game has** comes from each logout's file (`settings`): every
//!   option's value and the last profile the addon applied.
//! - **Profiles.** Approving a change makes a new numbered profile: every
//!   option's value. The newest goes to the game in the `Settings` slot, and
//!   the addon applies each profile once, at login, by its number.
//! - **Keep or go back.** Once the game has applied a profile, the app asks
//!   once. Going back sends what the game had before as a new profile.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::sv::{LuaTable, LuaValue};

/// The most changes one proposal can carry (spec §6).
pub const MAX_CHANGES: usize = 50;

#[derive(Debug, Clone, Deserialize)]
pub struct Schema {
    pub version: i64,
    pub options: Vec<Opt>,
}

/// One option as `Settings.lua` declares it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Opt {
    pub id: String,
    pub module: String,
    pub section: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub default: Value,
    pub label: String,
    pub help: String,
    #[serde(default)]
    pub parent: Option<String>,
    /// Turning it on lets the addon do something in the game for you
    /// (§8): never approved together with others.
    pub acts: bool,
    /// A number's range.
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    /// A choice's values.
    #[serde(default)]
    pub values: Option<Vec<String>>,
    /// Text's length in bytes (the addon counts bytes too); 200 if unset.
    #[serde(default)]
    pub max_length: Option<usize>,
}

/// Text's length when the declaration doesn't say.
const MAX_TEXT: usize = 200;

pub fn schema() -> &'static Schema {
    static SCHEMA: OnceLock<Schema> = OnceLock::new();
    SCHEMA.get_or_init(|| {
        serde_json::from_str(include_str!("../resources/settings-schema.json"))
            .expect("the bundled settings schema reads")
    })
}

pub fn option(id: &str) -> Option<&'static Opt> {
    schema().options.iter().find(|o| o.id == id)
}

/// Option id → value.
pub type Values = BTreeMap<String, Value>;

/// Whether `v` is a value the declaration allows: its type, and for a
/// number its range, for a choice one of its values, for text its length
/// and no control characters. The addon's `U.prefs.fits` checks the same
/// before applying anything.
fn fits(o: &Opt, v: &Value) -> bool {
    match o.kind.as_str() {
        "bool" => v.is_boolean(),
        "number" => v.as_f64().is_some_and(|n| {
            n.is_finite() && o.min.is_none_or(|m| n >= m) && o.max.is_none_or(|m| n <= m)
        }),
        "enum" => v
            .as_str()
            .is_some_and(|s| o.values.iter().flatten().any(|x| x == s)),
        "string" => v.as_str().is_some_and(|s| {
            s.len() <= o.max_length.unwrap_or(MAX_TEXT) && !s.contains(|c: char| c.is_control())
        }),
        // Any other type: nothing is accepted until it has its check here.
        _ => false,
    }
}

/// What an option takes, in words for the agent.
fn allowed(o: &Opt) -> String {
    let n = |v: Option<f64>| v.map_or_else(|| "any".to_string(), |v| v.to_string());
    match o.kind.as_str() {
        "bool" => "true or false".into(),
        "number" => format!("a number from {} to {}", n(o.min), n(o.max)),
        "enum" => format!(
            "one of: {}",
            o.values.clone().unwrap_or_default().join(", ")
        ),
        "string" => format!(
            "text of at most {} bytes, on one line",
            o.max_length.unwrap_or(MAX_TEXT)
        ),
        _ => "a value this version can't set".into(),
    }
}

/// A value in plain words, for Approvals: "on", "off".
pub fn word(v: &Value) -> String {
    match v {
        Value::Bool(true) => "on".into(),
        Value::Bool(false) => "off".into(),
        Value::String(s) => s.clone(),
        v => v.to_string(),
    }
}

/// Only the options the schema knows, with values of their type.
fn known(vals: Values) -> Values {
    vals.into_iter()
        .filter(|(k, v)| option(k).is_some_and(|o| fits(o, v)))
        .collect()
}

fn read_vals(s: &str) -> Values {
    known(serde_json::from_str(s).unwrap_or_default())
}

fn json(vals: &Values) -> String {
    serde_json::to_string(vals).expect("values serialize")
}

pub fn defaults() -> Values {
    schema()
        .options
        .iter()
        .map(|o| (o.id.clone(), o.default.clone()))
        .collect()
}

struct Seen {
    profile: Option<i64>,
    vals: Values,
}

struct Profile {
    id: i64,
    vals: Values,
    base: Option<Values>,
    decision: Option<String>,
}

fn seen(c: &Connection, flavor: &str) -> AppResult<Option<Seen>> {
    Ok(c.query_row(
        "SELECT profile, vals FROM addon_settings_seen WHERE flavor = ?1",
        [flavor],
        |r| {
            Ok(Seen {
                profile: r.get(0)?,
                vals: read_vals(&r.get::<_, String>(1)?),
            })
        },
    )
    .optional()?)
}

fn newest(c: &Connection, flavor: &str) -> AppResult<Option<Profile>> {
    Ok(c.query_row(
        "SELECT id, vals, base, decision FROM addon_profiles WHERE flavor = ?1
         ORDER BY id DESC LIMIT 1",
        [flavor],
        |r| {
            Ok(Profile {
                id: r.get(0)?,
                vals: read_vals(&r.get::<_, String>(1)?),
                base: r.get::<_, Option<String>>(2)?.map(|b| read_vals(&b)),
                decision: r.get(3)?,
            })
        },
    )
    .optional()?)
}

/// What the game has: the defaults, then the last logout's values.
fn in_game(seen: Option<&Seen>) -> Values {
    let mut v = defaults();
    if let Some(s) = seen {
        v.extend(s.vals.clone());
    }
    v
}

/// The newest profile, if the game hasn't applied it yet.
fn pending<'a>(seen: Option<&Seen>, newest: Option<&'a Profile>) -> Option<&'a Profile> {
    let applied = seen.and_then(|s| s.profile).unwrap_or(0);
    newest.filter(|p| p.id > applied)
}

/// What each option is about to be: the newest approved profile if the game
/// hasn't applied it yet, else what the game has.
fn effective(c: &Connection, flavor: &str) -> AppResult<(Values, bool)> {
    let s = seen(c, flavor)?;
    let n = newest(c, flavor)?;
    Ok(match pending(s.as_ref(), n.as_ref()) {
        Some(p) => {
            let mut v = in_game(s.as_ref());
            v.extend(p.vals.clone());
            (v, true)
        }
        None => (in_game(s.as_ref()), false),
    })
}

/// Ingest: a logout's `settings`, kept when it's the newest file.
pub fn record_seen(
    tx: &Transaction<'_>,
    flavor: &str,
    profile: Option<i64>,
    vals: &Values,
    at: i64,
) -> AppResult<()> {
    tx.execute(
        "INSERT INTO addon_settings_seen (flavor, profile, vals, seen_at) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (flavor) DO UPDATE SET
           profile = excluded.profile, vals = excluded.vals, seen_at = excluded.seen_at
         WHERE excluded.seen_at >= addon_settings_seen.seen_at",
        params![flavor, profile, json(&known(vals.clone())), at],
    )?;
    Ok(())
}

// ---- get_settings ----------------------------------------------------------

/// The `get_settings` tool's answer: the schema (or one module of it) with
/// each option's value.
pub fn read(db: &Db, flavor: &str, module: Option<&str>) -> AppResult<Value> {
    let (vals, pending) = db.with_conn(|c| effective(c, flavor))?;
    let options: Vec<Value> = schema()
        .options
        .iter()
        .filter(|o| module.is_none_or(|m| o.module == m))
        .map(|o| {
            serde_json::json!({
                "id": o.id, "module": o.module, "section": o.section, "type": o.kind,
                "label": o.label, "help": o.help, "default": o.default,
                "requires": o.parent, "acts": o.acts,
                "value": vals.get(&o.id).unwrap_or(&o.default),
            })
        })
        .collect();
    if options.is_empty() {
        let mut modules: Vec<&str> = schema().options.iter().map(|o| o.module.as_str()).collect();
        modules.sort_unstable();
        modules.dedup();
        return Err(AppError::NotFound(format!(
            "No settings module called {:?}. There are: {}.",
            module.unwrap_or_default(),
            modules.join(", ")
        )));
    }
    Ok(serde_json::json!({
        "schema_version": schema().version,
        "values_are": if pending {
            "settings the player approved, which reach the game at its next login"
        } else {
            "the game's, as of the last logout (defaults before the first)"
        },
        "options": options,
        "presets": [],
        "lints": [],
    }))
}

// ---- proposals -------------------------------------------------------------

/// One change as `propose_settings` takes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposedChange {
    pub option: String,
    pub value: Value,
    #[serde(default)]
    pub reason: Option<String>,
}

/// The `propose_settings` tool's arguments, in the inbox file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsProposal {
    pub changes: Vec<ProposedChange>,
}

/// One checked change: an Approvals row of its own, so each is approved
/// on its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingBody {
    pub option: String,
    pub value: Value,
}

/// Checks a proposal against the schema and the values now. Each change
/// comes back with its own reason.
pub fn check(
    db: &Db,
    flavor: &str,
    p: &SettingsProposal,
) -> Result<Vec<(SettingBody, Option<String>)>, String> {
    if p.changes.is_empty() || p.changes.len() > MAX_CHANGES {
        return Err(format!("1 to {MAX_CHANGES} changes"));
    }
    let (now, _) = db
        .with_conn(|c| effective(c, flavor))
        .map_err(|e| e.to_string())?;
    let mut out: Vec<(SettingBody, Option<String>)> = Vec::new();
    for ch in &p.changes {
        let o = option(&ch.option).ok_or_else(|| {
            format!(
                "No setting called {:?}: get_settings lists them.",
                ch.option
            )
        })?;
        if !fits(o, &ch.value) {
            return Err(format!("{} takes {}", o.id, allowed(o)));
        }
        if out.iter().any(|(b, _)| b.option == o.id) {
            return Err(format!("{} is changed twice", o.id));
        }
        if now.get(&o.id) == Some(&ch.value) {
            return Err(format!("{} is already {}", o.id, word(&ch.value)));
        }
        let reason = ch
            .reason
            .as_deref()
            .map(crate::proposals::plain_line)
            .filter(|r| !r.is_empty());
        if reason
            .as_ref()
            .is_some_and(|r| r.chars().count() > crate::proposals::MAX_REASON)
        {
            return Err(format!(
                "a reason is longer than {} characters",
                crate::proposals::MAX_REASON
            ));
        }
        out.push((
            SettingBody {
                option: o.id.clone(),
                value: ch.value.clone(),
            },
            reason,
        ));
    }
    // An option only works while its parent is on (U1d): the parent must be
    // on now or turned on by this same proposal.
    for (b, _) in &out {
        let Some(parent) = option(&b.option).and_then(|o| o.parent.as_deref()) else {
            continue;
        };
        let turned = out
            .iter()
            .find(|(x, _)| x.option == parent)
            .map(|(x, _)| &x.value);
        if turned.or(now.get(parent)) != Some(&Value::Bool(true)) {
            return Err(format!(
                "{} only works while {parent} is on: turn that on in the same proposal",
                b.option
            ));
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct SettingView {
    pub option: String,
    /// Ours, from the schema: never the agent's words.
    pub label: String,
    pub help: String,
    pub section: String,
    /// "on" / "off".
    pub from: String,
    pub to: String,
    /// Lets the addon act for you: approved on its own, never with others.
    pub acts: bool,
    /// Already the value (changed since): Approve changes nothing.
    pub same: bool,
}

pub fn view(db: &Db, flavor: &str, b: &SettingBody) -> AppResult<SettingView> {
    let (now, _) = db.with_conn(|c| effective(c, flavor))?;
    let o =
        option(&b.option).ok_or_else(|| AppError::NotFound(format!("the setting {}", b.option)))?;
    let from = now.get(&o.id).unwrap_or(&o.default);
    Ok(SettingView {
        option: o.id.clone(),
        label: o.label.clone(),
        help: o.help.clone(),
        section: o.section.clone(),
        from: word(from),
        to: word(&b.value),
        acts: o.acts,
        same: from == &b.value,
    })
}

/// Approve: a new profile, the values about to be with this change.
pub fn approve(db: &Db, flavor: &str, b: &SettingBody, now: i64) -> AppResult<()> {
    let o = option(&b.option)
        .filter(|o| fits(o, &b.value))
        .ok_or_else(|| AppError::NotFound(format!("the setting {}", b.option)))?;
    db.with_conn(|c| {
        let tx = c.transaction()?;
        let (mut vals, _) = effective(&tx, flavor)?;
        if vals.get(&o.id) == Some(&b.value) {
            return Ok(());
        }
        let s = seen(&tx, flavor)?;
        let n = newest(&tx, flavor)?;
        // The question afterwards compares with what the game had before
        // the first of these changes.
        let base = pending(s.as_ref(), n.as_ref())
            .and_then(|p| p.base.clone())
            .unwrap_or_else(|| in_game(s.as_ref()));
        vals.insert(o.id.clone(), b.value.clone());
        insert_profile(&tx, flavor, &vals, Some(&base), now)?;
        tx.commit()?;
        Ok(())
    })
}

/// A new profile, numbered above every profile stored and every one a game
/// has reported applying. After a restore from an older backup the db's own
/// numbers start lower than the game's, and the addon would skip those.
fn insert_profile(
    tx: &Transaction<'_>,
    flavor: &str,
    vals: &Values,
    base: Option<&Values>,
    now: i64,
) -> AppResult<()> {
    tx.execute(
        "INSERT INTO addon_profiles (id, flavor, vals, base, created_at)
         VALUES (max(coalesce((SELECT max(id) FROM addon_profiles), 0),
                     coalesce((SELECT max(profile) FROM addon_settings_seen), 0)) + 1,
                 ?1, ?2, ?3, ?4)",
        params![flavor, json(vals), base.map(json), now],
    )?;
    Ok(())
}

// ---- the slot --------------------------------------------------------------

/// The Settings slot's entries: the newest profile, `profile = id` and
/// `values = { [id] = value }`. Nothing before the first approval.
pub fn slot_entries(db: &Db, flavor: &str) -> AppResult<Vec<(LuaValue, LuaValue)>> {
    let Some(p) = db.with_conn(|c| newest(c, flavor))? else {
        return Ok(Vec::new());
    };
    let values = LuaTable {
        array: Vec::new(),
        hash: p
            .vals
            .iter()
            .filter_map(|(k, v)| {
                let v = match v {
                    Value::Bool(b) => LuaValue::Bool(*b),
                    Value::String(s) => LuaValue::str(s),
                    Value::Number(n) => n
                        .as_i64()
                        .map(LuaValue::Int)
                        .or_else(|| n.as_f64().map(LuaValue::Num))?,
                    _ => return None,
                };
                Some((LuaValue::str(k), v))
            })
            .collect(),
    };
    Ok(vec![
        (LuaValue::str("profile"), LuaValue::Int(p.id)),
        (LuaValue::str("values"), LuaValue::Table(Box::new(values))),
    ])
}

// ---- keep or go back -------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct TrialChange {
    pub label: String,
    pub from: String,
    pub to: String,
}

/// New settings the game has applied, waiting for "Keep or go back?".
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct Trial {
    pub profile: u32,
    pub changes: Vec<TrialChange>,
}

fn trial_in(c: &Connection, flavor: &str) -> AppResult<Option<(Profile, Vec<TrialChange>)>> {
    let Some(applied) = seen(c, flavor)?.and_then(|s| s.profile) else {
        return Ok(None);
    };
    let Some(p) = newest(c, flavor)?.filter(|p| p.id == applied && p.decision.is_none()) else {
        return Ok(None);
    };
    let Some(base) = &p.base else {
        return Ok(None);
    };
    let changes: Vec<TrialChange> = schema()
        .options
        .iter()
        .filter_map(|o| {
            let (was, is) = (base.get(&o.id)?, p.vals.get(&o.id)?);
            (was != is).then(|| TrialChange {
                label: o.label.clone(),
                from: word(was),
                to: word(is),
            })
        })
        .collect();
    Ok((!changes.is_empty()).then_some((p, changes)))
}

pub fn trial(db: &Db, flavor: &str) -> AppResult<Option<Trial>> {
    db.with_conn(|c| {
        Ok(trial_in(c, flavor)?.map(|(p, changes)| Trial {
            profile: p.id as u32,
            changes,
        }))
    })
}

/// Keep the new settings, or go back: what the game had before goes out
/// as a new profile (with nothing to ask about afterwards).
pub fn decide(db: &Db, flavor: &str, keep: bool, now: i64) -> AppResult<()> {
    db.with_conn(|c| {
        let tx = c.transaction()?;
        let (p, _) = trial_in(&tx, flavor)?
            .ok_or_else(|| AppError::NotFound("new settings to keep or undo".into()))?;
        tx.execute(
            "UPDATE addon_profiles SET decision = ?2 WHERE id = ?1",
            params![p.id, if keep { "kept" } else { "undone" }],
        )?;
        if !keep {
            let base = p.base.clone().unwrap_or_default();
            insert_profile(&tx, flavor, &base, None, now)?;
        }
        tx.commit()?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const FLAVOR: &str = "_classic_beta_";

    fn change(option: &str, value: Value) -> ProposedChange {
        ProposedChange {
            option: option.into(),
            value,
            reason: Some("you said minimal".into()),
        }
    }

    fn proposal(changes: Vec<ProposedChange>) -> SettingsProposal {
        SettingsProposal { changes }
    }

    /// A logout reporting `vals` and the last profile applied.
    fn logout(db: &Db, profile: Option<i64>, vals: Values, at: i64) {
        db.with_conn(|c| {
            let tx = c.transaction()?;
            record_seen(&tx, FLAVOR, profile, &vals, at)?;
            tx.commit()?;
            Ok(())
        })
        .unwrap();
    }

    fn slot(db: &Db) -> (i64, Values) {
        let e = slot_entries(db, FLAVOR).unwrap();
        let LuaValue::Int(id) = e[0].1 else { panic!() };
        let LuaValue::Table(t) = &e[1].1 else {
            panic!()
        };
        let vals = t
            .hash
            .iter()
            .map(|(k, v)| {
                let LuaValue::Bool(b) = v else { panic!() };
                (
                    String::from_utf8(k.as_bytes().unwrap().to_vec()).unwrap(),
                    json!(b),
                )
            })
            .collect();
        (id, vals)
    }

    #[test]
    fn the_bundled_schema_reads_and_every_default_fits() {
        assert!(!schema().options.is_empty());
        for o in &schema().options {
            assert!(fits(o, &o.default), "{}", o.id);
        }
    }

    /// Security's P4b note: every type is checked for range, choice and
    /// length, the same cases as the harness's `settings_fits`.
    #[test]
    fn values_are_checked_for_range_choice_and_length() {
        let decl = |extra: Value| -> Opt {
            let mut o = json!({ "id": "x.y", "module": "x", "section": "X", "type": "bool",
                                "default": false, "label": "L", "help": "H", "acts": false });
            o.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            serde_json::from_value(o).unwrap()
        };
        let num = decl(json!({ "type": "number", "min": 0, "max": 1 }));
        let choice = decl(json!({ "type": "enum", "values": ["left", "right"] }));
        let text = decl(json!({ "type": "string", "maxLength": 5 }));
        let any_text = decl(json!({ "type": "string" }));
        for (o, v, want) in [
            (&decl(json!({})), json!(true), true),
            (&decl(json!({})), json!("true"), false),
            (&num, json!(0), true),
            (&num, json!(1), true),
            (&num, json!(0.5), true),
            (&num, json!(-0.1), false),
            (&num, json!(1.5), false),
            (&num, json!("0.5"), false),
            (&choice, json!("left"), true),
            (&choice, json!("up"), false),
            (&choice, json!(1), false),
            (&text, json!("hello"), true),
            (&text, json!("hello!"), false),
            (&text, json!("a\nb"), false),
            (&any_text, json!("x".repeat(200)), true),
            (&any_text, json!("x".repeat(201)), false),
            (&decl(json!({ "type": "color" })), json!("ff0000"), false),
        ] {
            assert_eq!(fits(o, &v), want, "{} {v}", o.kind);
        }
        assert_eq!(allowed(&num), "a number from 0 to 1");
        assert_eq!(allowed(&choice), "one of: left, right");
    }

    #[test]
    fn a_proposal_is_checked_against_the_schema_and_the_values_now() {
        let db = Db::open_in_memory().unwrap();
        let bad = |changes, want: &str| {
            let err = check(&db, FLAVOR, &proposal(changes)).unwrap_err();
            assert!(err.contains(want), "{err:?} lacks {want:?}");
        };
        bad(vec![], "1 to 50 changes");
        bad(
            vec![change("bars.fade", json!(true))],
            "No setting called \"bars.fade\"",
        );
        bad(
            vec![change("window.coach", json!("yes"))],
            "takes true or false",
        );
        bad(
            vec![change("window.briefing", json!(true))],
            "is already on",
        );
        bad(
            vec![
                change("window.coach", json!(true)),
                change("window.coach", json!(true)),
            ],
            "changed twice",
        );
        bad(
            vec![change("window.coachCombat", json!(true))],
            "only works while window.coach is on",
        );
        // With its parent in the same proposal it's fine.
        let ok = check(
            &db,
            FLAVOR,
            &proposal(vec![
                change("window.coachCombat", json!(true)),
                change("window.coach", json!(true)),
            ]),
        )
        .unwrap();
        assert_eq!(ok.len(), 2);
        assert_eq!(ok[0].1.as_deref(), Some("you said minimal"));
        let long = ProposedChange {
            reason: Some("x".repeat(301)),
            ..change("window.coach", json!(true))
        };
        bad(vec![long], "longer than 300");
    }

    #[test]
    fn approving_makes_a_profile_the_slot_carries_and_the_game_applies_once() {
        let db = Db::open_in_memory().unwrap();
        assert!(
            slot_entries(&db, FLAVOR).unwrap().is_empty(),
            "nothing before an approval"
        );
        logout(&db, None, defaults(), 100);

        let coach = SettingBody {
            option: "window.coach".into(),
            value: json!(true),
        };
        let minimap = SettingBody {
            option: "window.minimap".into(),
            value: json!(false),
        };
        assert_eq!(view(&db, FLAVOR, &coach).unwrap().from, "off");
        approve(&db, FLAVOR, &coach, 200).unwrap();
        approve(&db, FLAVOR, &minimap, 201).unwrap();
        let (id, vals) = slot(&db);
        assert_eq!(vals["window.coach"], json!(true));
        assert_eq!(vals["window.minimap"], json!(false));
        assert_eq!(
            vals.len(),
            schema().options.len(),
            "every option, not only the changes"
        );
        // The preview now shows what's about to be.
        assert!(view(&db, FLAVOR, &coach).unwrap().same);
        assert!(
            trial(&db, FLAVOR).unwrap().is_none(),
            "not asked before the game has it"
        );

        // The game applied it: asked once, both changes against the game's
        // values before the first.
        logout(&db, Some(id), vals.clone(), 300);
        let t = trial(&db, FLAVOR).unwrap().unwrap();
        let words: Vec<String> = t
            .changes
            .iter()
            .map(|c| format!("{}: {} → {}", c.label, c.from, c.to))
            .collect();
        assert_eq!(
            words,
            ["Session coach: off → on", "Minimap button: on → off"]
        );

        // Go back: the old values as a new profile, and no question about it.
        decide(&db, FLAVOR, false, 400).unwrap();
        assert!(trial(&db, FLAVOR).unwrap().is_none());
        let (back, vals) = slot(&db);
        assert!(back > id);
        assert_eq!(vals, defaults());
        logout(&db, Some(back), vals, 500);
        assert!(trial(&db, FLAVOR).unwrap().is_none());
        assert!(decide(&db, FLAVOR, true, 600).is_err(), "nothing to decide");
    }

    /// After a restore from an older backup the db has no profiles, but the
    /// game has applied profile 40: the next one must be numbered above it,
    /// or the addon would skip it.
    #[test]
    fn after_a_restore_new_profiles_are_numbered_above_the_games() {
        let db = Db::open_in_memory().unwrap();
        logout(&db, Some(40), defaults(), 100);
        let coach = SettingBody {
            option: "window.coach".into(),
            value: json!(true),
        };
        approve(&db, FLAVOR, &coach, 200).unwrap();
        let (id, vals) = slot(&db);
        assert_eq!(id, 41);
        // And going back after it is numbered above that.
        logout(&db, Some(41), vals, 300);
        decide(&db, FLAVOR, false, 400).unwrap();
        assert_eq!(slot(&db).0, 42);
    }

    #[test]
    fn keep_asks_no_more_and_a_newer_logout_wins() {
        let db = Db::open_in_memory().unwrap();
        let coach = SettingBody {
            option: "window.coach".into(),
            value: json!(true),
        };
        approve(&db, FLAVOR, &coach, 200).unwrap();
        let (id, vals) = slot(&db);
        logout(&db, Some(id), vals, 300);
        decide(&db, FLAVOR, true, 400).unwrap();
        assert!(trial(&db, FLAVOR).unwrap().is_none());
        // An older file arriving late doesn't overwrite the newer one.
        logout(&db, None, defaults(), 250);
        let (now, pending) = db.with_conn(|c| effective(c, FLAVOR)).unwrap();
        assert_eq!((now["window.coach"].clone(), pending), (json!(true), false));
    }
}
