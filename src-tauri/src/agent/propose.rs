//! The proposal tools (spec §3, P2b). `propose_note` checks the proposal
//! against the app's data (read-only, so the agent hears about a typo at
//! once) and writes it to the inbox. Nothing applies until the player
//! approves it in the app, which checks the file again from scratch.
//! `list_proposals` tells the agent what became of its suggestions.

use serde::Deserialize;
use serde_json::{json, Value};

use super::Paths;
use crate::db::Db;
use crate::proposals::inbox::{self, InboxFile};
use crate::proposals::{self, NoteProposal};

const WAITING: &str =
    "Waiting for the player's approval in Forever Buddy. Nothing changes until they approve it.";

struct Tool {
    name: &'static str,
    description: &'static str,
    read_only: bool,
    schema: fn() -> Value,
}

const TOOLS: [Tool; 2] = [
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
struct NoteArgs {
    character: String,
    text: String,
    until: Option<String>,
    replaces: Option<proposals::Replaces>,
    reason: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoArgs {}

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

pub fn call(
    paths: &Paths,
    flavor: &str,
    client: &str,
    name: &str,
    raw: &Value,
) -> Result<Value, String> {
    let db = open(paths)?;
    let now = chrono::Utc::now().timestamp();
    match name {
        "propose_note" => {
            let a: NoteArgs = args(raw)?;
            let note = NoteProposal {
                character: a.character,
                text: a.text,
                until: a.until,
                replaces: a.replaces,
            };
            if a.reason
                .as_ref()
                .is_some_and(|r| r.chars().count() > proposals::MAX_REASON)
            {
                return Err(format!(
                    "The reason is longer than {} characters.",
                    proposals::MAX_REASON
                ));
            }
            if proposals::waiting_count(&db, flavor).map_err(|e| e.to_string())?
                >= proposals::MAX_PENDING
            {
                return Err(format!(
                    "{} suggestions are already waiting for the player. Try again once they've decided some.",
                    proposals::MAX_PENDING
                ));
            }
            // The app checks again on pickup; this tells the agent now.
            proposals::check_note(&db, flavor, &note, now)?;
            let id = inbox::write(
                &paths.dir,
                &InboxFile {
                    v: inbox::VERSION,
                    producer: client.to_string(),
                    kind: proposals::LOGIN_NOTE.into(),
                    created_at: now,
                    reason: a.reason,
                    body: serde_json::to_value(&note).map_err(|e| e.to_string())?,
                },
            )
            .map_err(|e| e.to_string())?;
            Ok(json!({ "proposal": id, "status": WAITING }))
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
                    "character": p.note.as_ref().map(|n| n.character.clone()),
                    "text": p.note.as_ref().map(|n| n.text.clone()),
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
