//! The proposal tools (spec §3; P2b notes, P2c plans and lists). Each checks
//! the proposal against the app's data (read-only, so the agent hears about
//! a typo at once) and writes it to the inbox. Nothing applies until the player
//! approves it in the app, which checks the file again from scratch.
//! `list_proposals` tells the agent what became of its suggestions.

use serde::Deserialize;
use serde_json::{json, Value};

use super::Paths;
use crate::addon_settings::SettingsProposal;
use crate::db::Db;
use crate::goals::{self, GoalKind};
use crate::proposals::bags::{BagProposal, ProposedMark};
use crate::proposals::draft::DraftProposal;
use crate::proposals::goal::GoalProposal;
use crate::proposals::inbox::{self, InboxFile};
use crate::proposals::list::{ListProposal, ProposedItem};
use crate::proposals::plan::{PlanProposal, ProposedStep};
use crate::proposals::{self, NoteProposal};

const WAITING: &str =
    "Waiting for the player's approval in Forever Buddy. Nothing changes until they approve it.";

struct Tool {
    name: &'static str,
    description: &'static str,
    read_only: bool,
    schema: fn() -> Value,
}

const TOOLS: [Tool; 10] = [
    Tool {
        name: "propose_note",
        description: "Suggests a login note for one character: a line shown in the game's chat when it logs in, at the next login only or at each login until a date. The player approves or declines it in Forever Buddy; nothing changes until then. To replace one of the character's notes (get_character lists them), pass its id and the text you read.",
        read_only: false,
        schema: || {
            json!({ "type": "object", "properties": {
                "character": { "type": "string", "description": "The character's name as list_characters gives it." },
                "text": { "type": "string", "maxLength": crate::notes::MAX_TEXT, "description": "One line of plain text." },
                "until": { "type": "string", "description": "Show at each login until this day (YYYY-MM-DD). Leave out for the next login only." },
                "replaces": { "type": "object", "properties": {
                    "id": { "type": "integer" }, "text": { "type": "string", "description": "The note's text as you read it." } },
                    "required": ["id", "text"], "additionalProperties": false },
                "reason": { "type": "string", "maxLength": proposals::MAX_REASON, "description": "Why, in a sentence. Shown to the player." } },
              "required": ["character", "text"], "additionalProperties": false })
        },
    },
    Tool {
        name: "propose_quest_plan",
        description: "Suggests tonight's quest plan for one character: a short checklist the player sees in game (/fb plan) and ticks off. Approving it replaces the character's current plan, which the player sees side by side first. Give each step's quest id where there is one; the app adds the quest giver and map position from what the character has actually seen, so don't invent coordinates.",
        read_only: false,
        schema: || {
            json!({ "type": "object", "properties": {
                "character": { "type": "string", "description": "The character's name as list_characters gives it." },
                "title": { "type": "string", "maxLength": 120 },
                "steps": { "type": "array", "minItems": 1, "maxItems": 50, "items": { "type": "object", "properties": {
                    "text": { "type": "string", "maxLength": 200, "description": "What to do, in plain words." },
                    "quest_id": { "type": "integer" },
                    "zone": { "type": "string" },
                    "kind": { "type": "string", "enum": ["accept", "turn_in", "objective"],
                              "description": "What finishes it: taking the quest, handing it in, or the player ticking it. Accept and turn_in need a quest_id." } },
                    "required": ["text"], "additionalProperties": false } },
                "reason": { "type": "string", "maxLength": proposals::MAX_REASON, "description": "Why, in a sentence. Shown to the player." } },
              "required": ["character", "title", "steps"], "additionalProperties": false })
        },
    },
    Tool {
        name: "propose_list_change",
        description: "Suggests a new shopping list, or items to add to (or new amounts for) one of the player's lists. Lists track what the player is gathering across characters. Use item ids from find_items or get_character. Nothing is ever removed this way.",
        read_only: false,
        schema: || {
            json!({ "type": "object", "properties": {
                "list": { "type": "string", "maxLength": 60, "description": "The name of one of the player's lists, or of a new one." },
                "for_character": { "type": "string", "description": "A new list only: the character it's for." },
                "items": { "type": "array", "minItems": 1, "maxItems": 100, "items": { "type": "object", "properties": {
                    "item_id": { "type": "integer" }, "need": { "type": "integer", "minimum": 1, "maximum": 9999 } },
                    "required": ["item_id", "need"], "additionalProperties": false } },
                "reason": { "type": "string", "maxLength": proposals::MAX_REASON, "description": "Why, in a sentence. Shown to the player." } },
              "required": ["list", "items"], "additionalProperties": false })
        },
    },
    Tool {
        name: "propose_bag_marks",
        description: "Suggests marking some of one character's bag items to sell at a vendor, or to send to another of the player's characters. The marks show in the player's bags in game; selling and sending stay manual, and nothing changes until the player approves. A reason, if given, must match what Forever Buddy itself works out for that item: 'grey' (poor quality), 'outgrown' (below what the character wears there, and no one else can use it), or 'upgrade' (an item level upgrade for the character it's sent to). Soulbound items can't be sent.",
        read_only: false,
        schema: || {
            json!({ "type": "object", "properties": {
                "character": { "type": "string", "description": "Whose bags: the name as list_characters gives it." },
                "marks": { "type": "array", "minItems": 1, "maxItems": 50, "items": { "type": "object", "properties": {
                    "item_id": { "type": "integer", "description": "An item the character holds (get_character)." },
                    "action": { "type": "string", "enum": ["sell", "send"] },
                    "to": { "type": "string", "description": "For send: another of the player's characters." },
                    "reason": { "type": "string", "enum": ["grey", "outgrown", "upgrade"] } },
                    "required": ["item_id", "action"], "additionalProperties": false } },
                "reason": { "type": "string", "maxLength": proposals::MAX_REASON, "description": "Why, in a sentence. Shown to the player." } },
              "required": ["character", "marks"], "additionalProperties": false })
        },
    },
    Tool {
        name: "propose_goal",
        description: "Suggests a goal: a character reaching a level, or a character (or, with no character, the whole account) having an amount of gold, optionally by a date. A goal that's already reached is refused. For collecting items, propose a list instead. Progress comes from the characters' own data after each logout, and the goal shows in the app and the in-game briefing. Nothing changes until the player approves it.",
        read_only: false,
        schema: || {
            json!({ "type": "object", "properties": {
                "character": { "type": "string", "description": "The character's name as list_characters gives it. Required for a level goal; omit for the whole account's gold." },
                "kind": { "type": "string", "enum": ["level", "gold"] },
                "target": { "type": "integer", "minimum": 1, "description": "A level, or whole gold (not copper)." },
                "label": { "type": "string", "maxLength": goals::MAX_LABEL, "description": "Gold only: what it's for, shown after the amount, e.g. \"for the mount\"." },
                "by": { "type": "string", "description": "Optional deadline, YYYY-MM-DD." },
                "reason": { "type": "string", "maxLength": proposals::MAX_REASON, "description": "Why, in a sentence. Shown to the player." } },
              "required": ["kind", "target"], "additionalProperties": false })
        },
    },
    Tool {
        name: "propose_settings",
        description: "Suggests changes to the Forever Buddy addon's in-game settings (get_settings lists them, with their ids and values now). Each change is its own row in the player's Approvals, shown with our label for the option, its old and new value, and your reason; the player approves each one, and approved settings reach the game at its next login. An option that requires another works only while that one is on, so turn the parent on in the same proposal. Up to 50 changes.",
        read_only: false,
        schema: || {
            json!({ "type": "object", "properties": {
                "changes": { "type": "array", "minItems": 1, "maxItems": crate::addon_settings::MAX_CHANGES, "items": { "type": "object", "properties": {
                    "option": { "type": "string", "description": "An option id from get_settings, e.g. \"window.coach\"." },
                    "value": { "description": "The new value, of the option's type (true or false for an on/off option)." },
                    "reason": { "type": "string", "maxLength": proposals::MAX_REASON, "description": "Why, in a sentence. Shown to the player." } },
                    "required": ["option", "value"], "additionalProperties": false } } },
              "required": ["changes"], "additionalProperties": false })
        },
    },
    Tool {
        name: "propose_ui_draft",
        description: "Makes a draft of the player's in-game look: a named set of HUD and action bar settings (get_settings lists them; only display-only hud.* and actionbars.* options). The player sees each draft drawn on their own screen in Forever Buddy's Your UI tab and picks one with Use this, which sends it to the game at its next login; Throw away drops it. Nothing reaches the game until then. Make two or three drafts that differ in a clear way and let the player pick, rather than one. Up to 10 drafts wait at once.",
        read_only: false,
        schema: || {
            json!({ "type": "object", "properties": {
                "name": { "type": "string", "maxLength": proposals::draft::MAX_NAME, "description": "A few words the player picks it by, e.g. \"Stacked, blue\"." },
                "why": { "type": "string", "maxLength": proposals::MAX_REASON, "description": "What this take does differently, in a sentence. Shown to the player." },
                "words": { "type": "string", "maxLength": proposals::MAX_REASON, "description": "What the player asked for, in their words." },
                "settings": { "type": "object", "minProperties": 1, "maxProperties": crate::addon_settings::MAX_CHANGES,
                              "description": "Option id to value, e.g. {\"hud.enabled\": true, \"hud.layout\": \"stacked\"}. Each value of the option's type." } },
              "required": ["name", "why", "settings"], "additionalProperties": false })
        },
    },
    // MC1: one macro, approved on its own, applied by the player in game.
    Tool {
        name: "propose_macro",
        description: "Suggests one macro: a new one, a change to one (with was: its text as get_macro gave it), or a copy of one to another character (with from, and new text if you adapt it, such as Kick to Counterspell for a mage). Never a delete. The player reads the exact text and approves each macro on its own, then applies it in the game with one click, out of combat. Checked before anything is queued: the game's 255 bytes and 16-character names, the 120 account and 18 character slots, the character, and no /run, /script, /console or /dump lines: a macro you propose is slash commands the player presses, never code. One macro per call: to copy one to several characters, call once for each.",
        read_only: false,
        schema: || {
            json!({ "type": "object", "properties": {
                "action": { "type": "string", "enum": ["new", "edit", "copy"] },
                "character": { "type": "string", "description": "Whose macro: the name as list_characters gives it. Omit for an account-wide macro, shared by every character on the account." },
                "account": { "type": "integer", "minimum": 1, "description": "For an account-wide macro when there's more than one account: list_macros' number." },
                "name": { "type": "string", "maxLength": proposals::macros::MAX_NAME, "description": "The macro's name: for an edit, the one to change; for a copy, the name it gets." },
                "body": { "type": "string", "maxLength": crate::macros::MACRO_MAX, "description": "The macro's text, lines joined with \\n. New and edit: required. Copy: only to adapt it." },
                "icon": { "type": "string", "description": "A texture name like INV_Misc_QuestionMark or an icon file id. Optional: an edit or copy keeps its icon." },
                "was": { "type": "string", "description": "Edit only: the macro's text now, exactly as get_macro gave it." },
                "from": { "type": "object", "description": "Copy only: the macro to copy.", "properties": {
                    "character": { "type": "string", "description": "Its character; omit for an account-wide macro." },
                    "account": { "type": "integer", "minimum": 1 },
                    "name": { "type": "string" } },
                  "required": ["name"], "additionalProperties": false },
                "reason": { "type": "string", "maxLength": proposals::MAX_REASON, "description": "Why, in a sentence. Shown to the player." } },
              "required": ["action", "name"], "additionalProperties": false })
        },
    },
    Tool {
        name: "list_ui_drafts",
        description: "The player's UI drafts: those waiting on the Your UI tab, and the last 30 days' used, thrown away, or not queued (with the reason), each with its settings.",
        read_only: true,
        schema: || json!({ "type": "object", "properties": {}, "additionalProperties": false }),
    },
    Tool {
        name: "list_proposals",
        description: "What became of the suggestions made through this connection in the last 30 days: waiting, approved, declined, or not queued (with the reason).",
        read_only: true,
        schema: || json!({ "type": "object", "properties": {}, "additionalProperties": false }),
    },
];

pub fn list() -> Vec<Value> {
    TOOLS
        .iter()
        .map(|t| {
            let annotations = if t.read_only {
                json!({ "readOnlyHint": true, "openWorldHint": false })
            } else {
                json!({ "readOnlyHint": false, "destructiveHint": false, "openWorldHint": false })
            };
            json!({ "name": t.name, "description": t.description, "inputSchema": (t.schema)(), "annotations": annotations })
        })
        .collect()
}

pub fn exists(name: &str) -> bool {
    TOOLS.iter().any(|t| t.name == name)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MacroArgs {
    action: proposals::macros::MacroAction,
    character: Option<String>,
    account: Option<u32>,
    name: String,
    body: Option<String>,
    icon: Option<String>,
    was: Option<String>,
    from: Option<proposals::macros::MacroSource>,
    reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoteArgs {
    character: String,
    text: String,
    until: Option<String>,
    replaces: Option<proposals::Replaces>,
    reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanArgs {
    character: String,
    title: String,
    steps: Vec<ProposedStep>,
    reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListArgs {
    list: String,
    for_character: Option<String>,
    items: Vec<ProposedItem>,
    reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BagArgs {
    character: String,
    marks: Vec<ProposedMark>,
    reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GoalArgs {
    character: Option<String>,
    kind: GoalKind,
    target: u64,
    label: Option<String>,
    by: Option<String>,
    reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoArgs {}

/// Writes a checked proposal to the inbox for the app to pick up.
fn stage(
    db: &Db,
    paths: &Paths,
    flavor: &str,
    client: &str,
    kind: &str,
    reason: Option<String>,
    body: Value,
) -> Result<Value, String> {
    if reason
        .as_ref()
        .is_some_and(|r| r.chars().count() > proposals::MAX_REASON)
    {
        return Err(format!(
            "The reason is longer than {} characters.",
            proposals::MAX_REASON
        ));
    }
    if proposals::waiting_count(db, flavor).map_err(|e| e.to_string())? >= proposals::MAX_PENDING {
        return Err(format!(
            "{} suggestions are already waiting for the player. Try again once they've decided some.",
            proposals::MAX_PENDING
        ));
    }
    let id = inbox::write(
        &paths.dir,
        &InboxFile {
            v: inbox::VERSION,
            producer: client.to_string(),
            kind: kind.into(),
            created_at: chrono::Utc::now().timestamp(),
            reason,
            body,
        },
    )
    .map_err(|e| e.to_string())?;
    Ok(json!({ "proposal": id, "status": WAITING }))
}

fn args<T: for<'de> Deserialize<'de>>(v: &Value) -> Result<T, String> {
    let v = if v.is_null() { json!({}) } else { v.clone() };
    serde_json::from_value(v).map_err(|e| format!("Bad arguments: {e}"))
}

fn open(paths: &Paths) -> Result<Db, String> {
    if !paths.db.exists() {
        return Err(
            "Forever Buddy has no data yet: open the app and play a character first.".into(),
        );
    }
    Db::open_read_only(&paths.db).map_err(|e| format!("Forever Buddy couldn't read its data: {e}"))
}

#[cfg(test)]
pub fn call(
    paths: &Paths,
    flavor: &str,
    client: &str,
    name: &str,
    raw: &Value,
) -> Result<Value, String> {
    call_in(paths, None, flavor, client, name, raw)
}

/// `call`, with the game folder (`game`) a macro is checked against (MC1).
pub fn call_in(
    paths: &Paths,
    game: Option<&std::path::Path>,
    flavor: &str,
    client: &str,
    name: &str,
    raw: &Value,
) -> Result<Value, String> {
    let db = open(paths)?;
    let now = chrono::Utc::now().timestamp();
    match name {
        "propose_macro" => {
            let a: MacroArgs = args(raw)?;
            let p = proposals::macros::MacroProposal {
                action: a.action,
                character: a.character,
                account: a.account,
                name: a.name,
                body: a.body,
                icon: a.icon,
                was: a.was,
                from: a.from,
            };
            // The app checks again on pickup; this tells the agent now.
            let files = super::tools::macro_files(game, flavor)?;
            proposals::macros::check(&db, flavor, &files, &p)?;
            let body = serde_json::to_value(&p).map_err(|e| e.to_string())?;
            stage(&db, paths, flavor, client, proposals::MACRO, a.reason, body)
        }
        "propose_note" => {
            let a: NoteArgs = args(raw)?;
            let note = NoteProposal {
                character: a.character,
                text: a.text,
                until: a.until,
                replaces: a.replaces,
            };
            // The app checks again on pickup; this tells the agent now.
            proposals::check_note(&db, flavor, &note, now)?;
            let body = serde_json::to_value(&note).map_err(|e| e.to_string())?;
            stage(
                &db,
                paths,
                flavor,
                client,
                proposals::LOGIN_NOTE,
                a.reason,
                body,
            )
        }
        "propose_quest_plan" => {
            let a: PlanArgs = args(raw)?;
            let p = PlanProposal {
                character: a.character,
                title: a.title,
                steps: a.steps,
            };
            proposals::plan::check(&db, flavor, &p)?;
            let body = serde_json::to_value(&p).map_err(|e| e.to_string())?;
            stage(
                &db,
                paths,
                flavor,
                client,
                proposals::QUEST_PLAN,
                a.reason,
                body,
            )
        }
        "propose_list_change" => {
            let a: ListArgs = args(raw)?;
            let p = ListProposal {
                list: a.list,
                for_character: a.for_character,
                items: a.items,
            };
            proposals::list::check(&db, flavor, &p)?;
            let body = serde_json::to_value(&p).map_err(|e| e.to_string())?;
            stage(&db, paths, flavor, client, proposals::LIST, a.reason, body)
        }
        "propose_bag_marks" => {
            let a: BagArgs = args(raw)?;
            let p = BagProposal {
                character: a.character,
                marks: a.marks,
            };
            proposals::bags::check(&db, flavor, &p)?;
            let body = serde_json::to_value(&p).map_err(|e| e.to_string())?;
            stage(
                &db,
                paths,
                flavor,
                client,
                proposals::BAG_MARKS,
                a.reason,
                body,
            )
        }
        "propose_goal" => {
            let a: GoalArgs = args(raw)?;
            let p = GoalProposal {
                character: a.character,
                kind: a.kind,
                target: a.target,
                label: a.label,
                by: a.by,
            };
            proposals::goal::check(&db, flavor, &p, now)?;
            let body = serde_json::to_value(&p).map_err(|e| e.to_string())?;
            stage(&db, paths, flavor, client, proposals::GOAL, a.reason, body)
        }
        "propose_settings" => {
            let p: SettingsProposal = args(raw)?;
            let rows = crate::addon_settings::check(&db, flavor, &p)?;
            if proposals::waiting_count(&db, flavor).map_err(|e| e.to_string())? + rows.len() as i64
                > proposals::MAX_PENDING
            {
                return Err(format!(
                    "That would make more than {} suggestions waiting for the player. Try again once they've decided some, or send fewer changes.",
                    proposals::MAX_PENDING
                ));
            }
            let body = serde_json::to_value(&p).map_err(|e| e.to_string())?;
            stage(&db, paths, flavor, client, proposals::SETTINGS, None, body)
        }
        "propose_ui_draft" => {
            let p: DraftProposal = args(raw)?;
            // The app checks again on pickup; this tells the agent now.
            proposals::draft::check(&db, flavor, &p)?;
            if proposals::drafts_count(&db, flavor).map_err(|e| e.to_string())?
                >= proposals::draft::MAX_STAGED
            {
                return Err(format!(
                    "{} drafts are already waiting for the player. Ask them to use or throw some away first.",
                    proposals::draft::MAX_STAGED
                ));
            }
            let body = serde_json::to_value(&p).map_err(|e| e.to_string())?;
            stage(&db, paths, flavor, client, proposals::UI_DRAFT, None, body)
        }
        "list_ui_drafts" => {
            args::<NoArgs>(raw)?;
            let drafts = proposals::draft_history(&db, flavor, now).map_err(|e| e.to_string())?;
            let row = |r: &proposals::DraftRecord| {
                json!({
                    "status": match r.status.as_str() {
                        "staged" => "waiting",
                        "applied" => "used",
                        "discarded" => "thrown away",
                        _ => "not queued",
                    },
                    "why_not_queued": r.status_reason,
                    "proposed_at": r.created_at,
                    "decided_at": r.decided_at,
                    "name": r.draft.as_ref().map(|d| d.name.clone()),
                    "why": r.draft.as_ref().map(|d| d.why.clone()),
                    "settings": r.draft.as_ref().map(|d| d.changes.iter()
                        .map(|c| json!({ "option": c.option, "from": c.from, "to": c.to }))
                        .collect::<Vec<_>>()),
                })
            };
            Ok(json!({
                "drafts": drafts.iter().map(row).collect::<Vec<_>>(),
                "not_yet_picked_up": inbox::waiting(&paths.dir).len(),
            }))
        }
        "list_proposals" => {
            args::<NoArgs>(raw)?;
            let a = proposals::list(&db, flavor, now).map_err(|e| e.to_string())?;
            let row = |p: &proposals::Proposal| {
                json!({
                    "kind": p.kind,
                    "status": match p.status.as_str() {
                        "staged" => "waiting",
                        "applied" => "approved",
                        "discarded" => "declined",
                        _ => "not queued",
                    },
                    "why_not_queued": p.status_reason,
                    "proposed_at": p.created_at,
                    "decided_at": p.decided_at,
                    "character": p.note.as_ref().map(|n| n.character.clone())
                        .or_else(|| p.plan.as_ref().map(|n| n.character.clone())),
                    "text": p.note.as_ref().map(|n| n.text.clone()),
                    "plan_title": p.plan.as_ref().map(|n| n.title.clone()),
                    "list": p.list.as_ref().map(|l| l.name.clone()),
                    "bag_marks_for": p.bags.as_ref().map(|b| b.character.clone()),
                    "goal_for": p.goal.as_ref().map(|g| g.character.clone().unwrap_or_else(|| "the account".into())),
                    "setting": p.setting.as_ref().map(|s| json!({ "option": s.option, "from": s.from, "to": s.to })),
                    "macro": p.macro_change.as_ref().map(|m| json!({
                        "action": m.action, "name": m.name, "for": m.target, "in_game": m.game })),
                })
            };
            let not_picked_up = inbox::waiting(&paths.dir).len();
            Ok(json!({
                "proposals": a.waiting.iter().chain(&a.decided).map(row).collect::<Vec<_>>(),
                "not_yet_picked_up": not_picked_up,
            }))
        }
        _ => Err(format!("Unknown tool: {name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLAVOR: &str = "_classic_beta_";

    fn setup() -> (tempfile::TempDir, Paths) {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::new(&tmp.path().join("config"), &tmp.path().join("local"));
        std::fs::create_dir_all(paths.db.parent().unwrap()).unwrap();
        let db = Db::open(&paths.db).unwrap();
        db.with_conn(|c| {
            c.execute(
                "INSERT INTO characters (flavor, account, group_dir, char_dir, name, first_seen, last_seen)
                 VALUES (?1, 'A', '70', 'Coinpurse', 'Coinpurse', 0, 0)",
                [FLAVOR],
            )?;
            Ok(())
        })
        .unwrap();
        (tmp, paths)
    }

    /// MC1: a macro is checked against the game's own files at once; a good
    /// one waits in the inbox for the app.
    #[test]
    fn propose_macro_checks_against_the_game_and_queues() {
        let (_tmp, paths) = setup();
        let game = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(game.path().join(FLAVOR).join("WTF/Account/A/70/Coinpurse"))
            .unwrap();
        let run = |a: Value| {
            call_in(
                &paths,
                Some(game.path()),
                FLAVOR,
                "Claude Desktop",
                "propose_macro",
                &a,
            )
        };
        let ok = run(
            json!({ "action": "new", "character": "Coinpurse", "name": "Sit", "body": "/sit",
                             "reason": "One key to rest" }),
        )
        .unwrap();
        assert_eq!(ok["status"], WAITING);
        assert_eq!(inbox::waiting(&paths.dir).len(), 1);
        let e = run(json!({ "action": "new", "character": "Coinpurse", "name": "Code", "body": "/run x()" }))
            .unwrap_err();
        assert!(e.contains("never code"), "{e}");
        let e = call_in(
            &paths,
            None,
            FLAVOR,
            "c",
            "propose_macro",
            &json!({ "action": "new", "name": "X", "body": "/sit" }),
        )
        .unwrap_err();
        assert!(e.contains("game folder"), "{e}");
        assert_eq!(inbox::waiting(&paths.dir).len(), 1, "nothing more queued");
    }

    #[test]
    fn a_proposal_goes_through_the_inbox_and_the_agent_can_follow_it() {
        let (_tmp, paths) = setup();
        let out = call(
            &paths,
            FLAVOR,
            "Claude Desktop",
            "propose_note",
            &json!({ "character": "Coinpurse", "text": "Post the bars", "reason": "prices are up" }),
        )
        .unwrap();
        assert_eq!(out["status"], WAITING);
        assert_eq!(
            inbox::waiting(&paths.dir).len(),
            1,
            "written to the inbox, not the db"
        );

        let seen = call(
            &paths,
            FLAVOR,
            "Claude Desktop",
            "list_proposals",
            &json!({}),
        )
        .unwrap();
        assert_eq!(seen["not_yet_picked_up"], 1);

        // The app picks it up.
        let db = Db::open(&paths.db).unwrap();
        proposals::ingest(
            &db,
            &paths.dir,
            FLAVOR,
            true,
            chrono::Utc::now().timestamp(),
        )
        .unwrap();
        drop(db);
        let seen = call(
            &paths,
            FLAVOR,
            "Claude Desktop",
            "list_proposals",
            &json!({}),
        )
        .unwrap();
        assert_eq!(seen["proposals"][0]["status"], "waiting");
        assert_eq!(seen["proposals"][0]["text"], "Post the bars");
        assert_eq!(seen["not_yet_picked_up"], 0);
    }

    #[test]
    fn plans_and_list_changes_are_checked_then_written() {
        let (_tmp, paths) = setup();
        let plan = json!({ "character": "Coinpurse", "title": "Tonight", "steps": [{ "text": "Hearth" }] });
        call(&paths, FLAVOR, "c", "propose_quest_plan", &plan).unwrap();
        let bad_plan = json!({ "character": "Coinpurse", "title": "Tonight", "steps": [] });
        assert!(call(&paths, FLAVOR, "c", "propose_quest_plan", &bad_plan).is_err());
        let unknown_item = json!({ "list": "Raid", "items": [{ "item_id": 99999, "need": 1 }] });
        let e = call(&paths, FLAVOR, "c", "propose_list_change", &unknown_item).unwrap_err();
        assert_eq!(e, "item 99999 isn't one we know");
        assert_eq!(inbox::waiting(&paths.dir).len(), 1, "only the valid plan");
    }

    #[test]
    fn mistakes_are_told_to_the_agent_at_once_and_nothing_is_written() {
        let (_tmp, paths) = setup();
        for bad in [
            json!({ "character": "Nobody", "text": "x" }),
            json!({ "character": "Coinpurse", "text": "  " }),
            json!({ "character": "Coinpurse", "text": "x", "until": "yesterday" }),
            json!({ "character": "Coinpurse", "text": "x", "macro": "/cast" }),
            json!({ "character": "Coinpurse", "text": "x", "reason": "y".repeat(proposals::MAX_REASON + 1) }),
        ] {
            assert!(
                call(&paths, FLAVOR, "c", "propose_note", &bad).is_err(),
                "{bad}"
            );
        }
        assert!(inbox::waiting(&paths.dir).is_empty());
    }
}
