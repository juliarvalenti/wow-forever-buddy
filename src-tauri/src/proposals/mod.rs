//! The staged-changes queue (P2b, spec §4, IMPLEMENTING §17): what agents
//! propose waits here until the player approves or declines it in the app.
//!
//! The app is the only writer. `ingest` reads the agent inbox, checks each
//! file as untrusted input (any local process can write the folder), and
//! stores it as `staged`, or as `rejected` with the reason in plain words.
//! While agent access is off nothing is staged. The stored body is what the
//! Approvals preview shows and exactly what Approve applies, through the
//! kind's normal app code (`notes::add` for a login note).

pub mod inbox;

use std::path::Path;

use chrono::{Local, NaiveDate, TimeZone};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::characters;
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::notes::{self, Author, LoginNote, NewNote};

/// Spec §5: past this many waiting, new ones are refused.
pub const MAX_PENDING: i64 = 50;
/// The agent's reason: a sentence, not an essay.
pub const MAX_REASON: usize = 300;
const MAX_PRODUCER: usize = 64;
/// How far back the Decided list goes.
const DECIDED_DAYS: i64 = 30;

pub const LOGIN_NOTE: &str = "login_note";

/// Emitted when ingest stores something, so Approvals and its sidebar count
/// refresh.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct ApprovalsChanged;

/// A login note as an agent proposes it (the `propose_note` tool's
/// arguments, in the inbox file).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoteProposal {
    pub character: String,
    pub text: String,
    /// Shown at each login until this day (YYYY-MM-DD); absent, the next
    /// login only.
    #[serde(default)]
    pub until: Option<String>,
    /// The note it replaces, and that note's text as the agent read it.
    #[serde(default)]
    pub replaces: Option<Replaces>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(deny_unknown_fields)]
pub struct Replaces {
    pub id: u32,
    pub text: String,
}

/// A login note proposal after checking: what's stored and applied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NoteBody {
    character_id: u32,
    text: String,
    /// Unix seconds, the end of the chosen day; `None` is the next login.
    until: Option<i64>,
    replaces: Option<Replaces>,
}

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct Proposal {
    pub id: u32,
    pub kind: String,
    /// Exactly as the client reported it. Shown quoted, as a claim.
    pub producer: String,
    pub reason: Option<String>,
    /// RFC 3339.
    pub created_at: String,
    /// staged | applied | discarded | rejected
    pub status: String,
    /// Rejected: why, in plain words.
    pub status_reason: Option<String>,
    pub decided_at: Option<String>,
    /// The preview for a login note: every field Approve applies.
    pub note: Option<NoteView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct NoteView {
    pub character_id: u32,
    pub character: String,
    /// File token, lowercase, for the class colour.
    pub class: Option<String>,
    pub text: String,
    pub once: bool,
    /// RFC 3339.
    pub until: Option<String>,
    pub replaces: Option<Replaced>,
}

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct Replaced {
    pub id: u32,
    /// The note's text when the agent read it.
    pub saw: String,
    /// The note as it is now; `None` once it's gone (shown, removed, expired).
    pub now: Option<LoginNote>,
    /// The note changed after the agent read it: the player picks, and it's
    /// never part of Approve all.
    pub conflict: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct Approvals {
    /// Newest first.
    pub waiting: Vec<Proposal>,
    /// The last 30 days, newest first: approved, declined, not queued.
    pub decided: Vec<Proposal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Approve,
    Decline,
    /// For a conflict: replace the player's note anyway. "Keep mine" is
    /// `Decline`.
    UseProposed,
}

fn rfc3339(t: i64) -> String {
    chrono::DateTime::from_timestamp(t, 0)
        .unwrap_or_default()
        .to_rfc3339()
}

/// Text from an agent as stored: control characters out, trimmed.
fn plain_line(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_string()
}

pub fn producer_name(s: &str) -> String {
    let p: String = plain_line(s).chars().take(MAX_PRODUCER).collect();
    if p.is_empty() {
        "unknown client".into()
    } else {
        p
    }
}

/// The end of `day` in the player's time zone, as unix seconds.
fn end_of_day(day: &str) -> Option<i64> {
    let d = NaiveDate::parse_from_str(day, "%Y-%m-%d").ok()?;
    Local
        .from_local_datetime(&d.and_hms_opt(23, 59, 59)?)
        .earliest()
        .map(|t| t.timestamp())
}

/// Checks a login note proposal against the app's own data. The error is
/// the reason shown under "not queued", and given back to the agent.
pub fn check_note(
    db: &Db,
    flavor: &str,
    p: &NoteProposal,
    now: i64,
) -> Result<NoteBodyChecked, String> {
    let c = characters::by_name(db, flavor, &p.character).map_err(|e| match e {
        AppError::NotFound(m) => m,
        e => e.to_string(),
    })?;
    let text = notes::clean(&p.text).map_err(|e| match e {
        AppError::InvalidSettings(m) => m,
        e => e.to_string(),
    })?;
    let until = match &p.until {
        None => None,
        Some(day) => {
            let t =
                end_of_day(day).ok_or_else(|| format!("{day:?} isn't a date like 2026-10-09"))?;
            if t <= now {
                return Err(format!("{day} has already passed"));
            }
            Some(t)
        }
    };
    if let Some(r) = &p.replaces {
        let known: bool = db
            .with_conn(|conn| {
                Ok(conn
                    .query_row(
                        "SELECT 1 FROM login_notes WHERE id = ?1 AND character_id = ?2",
                        params![r.id, c.id],
                        |_| Ok(()),
                    )
                    .optional()?
                    .is_some())
            })
            .map_err(|e| e.to_string())?;
        if !known {
            return Err(format!(
                "note {} isn't one of {}'s",
                r.id,
                characters::full_name(&c)
            ));
        }
    }
    Ok(NoteBodyChecked(NoteBody {
        character_id: c.id,
        text,
        until,
        replaces: p.replaces.clone(),
    }))
}

/// A login note proposal that passed `check_note`.
pub struct NoteBodyChecked(NoteBody);

/// Staged proposals waiting in `flavor`.
pub fn waiting_count(db: &Db, flavor: &str) -> AppResult<i64> {
    db.with_conn(|c| {
        Ok(c.query_row(
            "SELECT count(*) FROM proposals WHERE flavor = ?1 AND status = 'staged'",
            [flavor],
            |r| r.get(0),
        )?)
    })
}

struct Row<'a> {
    file: &'a str,
    kind: &'a str,
    body: Option<String>,
    producer: &'a str,
    reason: Option<&'a str>,
    created_at: i64,
    status: &'a str,
    status_reason: Option<&'a str>,
}

fn insert(db: &Db, flavor: &str, row: Row<'_>, now: i64) -> AppResult<bool> {
    db.with_conn(|c| {
        let n = c.execute(
            "INSERT OR IGNORE INTO proposals
               (flavor, file, kind, body, producer, reason, created_at, received_at, status, status_reason)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                flavor,
                row.file,
                row.kind,
                row.body,
                row.producer,
                row.reason,
                row.created_at,
                now,
                row.status,
                row.status_reason
            ],
        )?;
        Ok(n > 0)
    })
}

/// Reads the agent inbox: every waiting file is stored, staged or rejected,
/// and deleted. `access` is the Settings switch: while it's off each file
/// is recorded as refused and nothing is staged. Returns how many were
/// stored.
pub fn ingest(db: &Db, agent_dir: &Path, flavor: &str, access: bool, now: i64) -> AppResult<usize> {
    let mut stored = 0;
    for (name, path) in inbox::waiting(agent_dir).into_iter().take(inbox::MAX_FILES) {
        let file = inbox::read(&path);
        let (producer, kind, created_at, reason) = match &file {
            Ok(f) => (
                producer_name(&f.producer),
                f.kind.clone(),
                f.created_at.clamp(0, now),
                f.reason
                    .as_deref()
                    .map(plain_line)
                    .filter(|r| !r.is_empty()),
            ),
            Err(_) => ("unknown client".into(), "unknown".into(), now, None),
        };
        let checked: Result<String, String> = (|| {
            let f = file.as_ref().map_err(|e| format!("not read: {e}"))?;
            if !access {
                return Err("agent access is off".into());
            }
            if waiting_count(db, flavor).map_err(|e| e.to_string())? >= MAX_PENDING {
                return Err(format!("{MAX_PENDING} suggestions were already waiting"));
            }
            if reason
                .as_ref()
                .is_some_and(|r| r.chars().count() > MAX_REASON)
            {
                return Err(format!("the reason is longer than {MAX_REASON} characters"));
            }
            match f.kind.as_str() {
                LOGIN_NOTE => {
                    let p: NoteProposal = serde_json::from_value(f.body.clone())
                        .map_err(|_| "the note isn't in the form this version reads".to_string())?;
                    let NoteBodyChecked(body) = check_note(db, flavor, &p, now)?;
                    serde_json::to_string(&body).map_err(|e| e.to_string())
                }
                other => Err(format!(
                    "{other:?} isn't something Forever Buddy can apply yet"
                )),
            }
        })();
        let row = Row {
            file: &name,
            kind: &kind,
            producer: &producer,
            reason: reason.as_deref(),
            created_at,
            body: checked.as_ref().ok().cloned(),
            status: if checked.is_ok() {
                "staged"
            } else {
                "rejected"
            },
            status_reason: checked.as_ref().err().map(String::as_str),
        };
        if insert(db, flavor, row, now)? {
            stored += 1;
        }
        // Stored (or stored before): the file has done its job. If it can't
        // be removed now, the next pass finds it already stored and tries
        // again.
        let _ = std::fs::remove_file(&path);
    }
    Ok(stored)
}

struct Stored {
    id: i64,
    kind: String,
    body: Option<String>,
    producer: String,
    reason: Option<String>,
    created_at: i64,
    status: String,
    status_reason: Option<String>,
    decided_at: Option<i64>,
}

fn stored(
    db: &Db,
    flavor: &str,
    filter: &str,
    args: &[&dyn rusqlite::ToSql],
) -> AppResult<Vec<Stored>> {
    db.with_conn(|c| {
        let sql = format!(
            "SELECT id, kind, body, producer, reason, created_at, status, status_reason, decided_at
             FROM proposals WHERE flavor = ? AND {filter}
             ORDER BY coalesce(decided_at, created_at) DESC, id DESC"
        );
        let mut stmt = c.prepare(&sql)?;
        let mut all: Vec<&dyn rusqlite::ToSql> = vec![&flavor];
        all.extend_from_slice(args);
        let rows = stmt
            .query_map(all.as_slice(), |r| {
                Ok(Stored {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    body: r.get(2)?,
                    producer: r.get(3)?,
                    reason: r.get(4)?,
                    created_at: r.get(5)?,
                    status: r.get(6)?,
                    status_reason: r.get(7)?,
                    decided_at: r.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

fn note_body(s: &Stored) -> Option<NoteBody> {
    (s.kind == LOGIN_NOTE)
        .then(|| serde_json::from_str(s.body.as_deref()?).ok())
        .flatten()
}

fn note_view(db: &Db, flavor: &str, b: &NoteBody, now: i64) -> AppResult<NoteView> {
    let card = characters::overview(db, flavor)?
        .characters
        .into_iter()
        .find(|c| c.id == b.character_id);
    let replaces = match &b.replaces {
        None => None,
        Some(r) => {
            let current = notes::active(db, flavor, r.id, now)?;
            let conflict = current.as_ref().is_none_or(|n| n.text != r.text);
            Some(Replaced {
                id: r.id,
                saw: r.text.clone(),
                now: current,
                conflict,
            })
        }
    };
    Ok(NoteView {
        character_id: b.character_id,
        character: card.as_ref().map(characters::full_name).unwrap_or_default(),
        class: card.and_then(|c| c.class),
        text: b.text.clone(),
        once: b.until.is_none(),
        until: b.until.map(rfc3339),
        replaces,
    })
}

fn view(db: &Db, flavor: &str, s: Stored, now: i64) -> AppResult<Proposal> {
    let note = match note_body(&s) {
        Some(b) => Some(note_view(db, flavor, &b, now)?),
        None => None,
    };
    Ok(Proposal {
        id: s.id as u32,
        kind: s.kind,
        producer: s.producer,
        reason: s.reason,
        created_at: rfc3339(s.created_at),
        status: s.status,
        status_reason: s.status_reason,
        decided_at: s.decided_at.map(rfc3339),
        note,
    })
}

/// What Approvals shows: everything waiting, and the last 30 days decided.
pub fn list(db: &Db, flavor: &str, now: i64) -> AppResult<Approvals> {
    let since = now - DECIDED_DAYS * 86_400;
    let waiting = stored(db, flavor, "status = 'staged'", &[])?
        .into_iter()
        .map(|s| view(db, flavor, s, now))
        .collect::<AppResult<_>>()?;
    let decided = stored(
        db,
        flavor,
        "status != 'staged' AND coalesce(decided_at, received_at) >= ?",
        &[&since],
    )?
    .into_iter()
    .map(|s| view(db, flavor, s, now))
    .collect::<AppResult<_>>()?;
    Ok(Approvals { waiting, decided })
}

fn set_status(db: &Db, id: i64, status: &str, now: i64) -> AppResult<()> {
    db.with_conn(|c| {
        c.execute(
            "UPDATE proposals SET status = ?2, decided_at = ?3 WHERE id = ?1 AND status = 'staged'",
            params![id, status, now],
        )?;
        Ok(())
    })
}

/// Approves or declines one waiting proposal. Approve applies the stored
/// body through the kind's own code and nothing else; a conflicting note
/// needs `UseProposed` (or `Decline`, "Keep mine").
pub fn decide(db: &Db, flavor: &str, id: u32, decision: Decision, now: i64) -> AppResult<()> {
    let s = stored(db, flavor, "id = ? AND status = 'staged'", &[&id])?
        .into_iter()
        .next()
        .ok_or_else(|| AppError::NotFound("that suggestion, or it was already decided".into()))?;
    if decision == Decision::Decline {
        return set_status(db, s.id, "discarded", now);
    }
    match s.kind.as_str() {
        LOGIN_NOTE => {
            let b = note_body(&s)
                .ok_or_else(|| AppError::Db("a stored note that doesn't read".into()))?;
            let view = note_view(db, flavor, &b, now)?;
            if decision == Decision::Approve && view.replaces.as_ref().is_some_and(|r| r.conflict) {
                return Err(AppError::InvalidSettings(
                    "this note changed after the agent read it: pick Keep mine or Use proposed"
                        .into(),
                ));
            }
            let new = NewNote {
                character_id: b.character_id,
                text: b.text.clone(),
                once: b.until.is_none(),
                until: b.until.map(|t| t as f64),
            };
            notes::add(
                db,
                flavor,
                &new,
                Author::Agent {
                    producer: &s.producer,
                },
                now,
            )?;
            if let Some(r) = view.replaces.filter(|r| r.now.is_some()) {
                notes::delete(db, flavor, r.id, now)?;
            }
        }
        other => {
            return Err(AppError::InvalidSettings(format!(
                "{other:?} can't be applied by this version"
            )))
        }
    }
    set_status(db, s.id, "applied", now)
}

#[cfg(test)]
mod tests {
    use super::inbox::{InboxFile, VERSION};
    use super::*;
    use serde_json::json;

    const FLAVOR: &str = "_classic_beta_";
    const NOW: i64 = 1_790_000_000;

    fn setup() -> (tempfile::TempDir, Db) {
        let tmp = tempfile::tempdir().unwrap();
        let db = Db::open_in_memory().unwrap();
        db.with_conn(|c| {
            for (name, surname) in [("Coinpurse", None), ("Velyra", Some("Duskmane"))] {
                c.execute(
                    "INSERT INTO characters (flavor, account, group_dir, char_dir, name, surname,
                                             first_seen, last_seen)
                     VALUES (?1, 'A', '70', ?2, ?2, ?3, 0, 0)",
                    params![FLAVOR, name, surname],
                )?;
            }
            Ok(())
        })
        .unwrap();
        (tmp, db)
    }

    fn drop_file(dir: &Path, kind: &str, body: serde_json::Value) -> String {
        inbox::write(
            dir,
            &InboxFile {
                v: VERSION,
                producer: "Claude Desktop\u{7}".into(),
                kind: kind.into(),
                created_at: NOW - 60,
                reason: Some("Thursday's raid".into()),
                body,
            },
        )
        .unwrap()
    }

    fn note(text: &str) -> serde_json::Value {
        json!({ "character": "coinpurse", "text": text })
    }

    #[test]
    fn a_valid_note_is_staged_and_nothing_applies_until_approved() {
        let (tmp, db) = setup();
        drop_file(tmp.path(), LOGIN_NOTE, note("Hand in the attunement"));
        assert_eq!(ingest(&db, tmp.path(), FLAVOR, true, NOW).unwrap(), 1);
        assert!(inbox::waiting(tmp.path()).is_empty(), "the file is deleted");

        let a = list(&db, FLAVOR, NOW).unwrap();
        assert_eq!(a.waiting.len(), 1);
        let p = &a.waiting[0];
        assert_eq!(
            p.producer, "Claude Desktop",
            "control characters are dropped"
        );
        assert_eq!(p.reason.as_deref(), Some("Thursday's raid"));
        let n = p.note.as_ref().unwrap();
        assert_eq!(
            (n.character.as_str(), n.text.as_str(), n.once),
            ("Coinpurse", "Hand in the attunement", true)
        );
        assert!(
            notes::list(&db, FLAVOR, NOW).unwrap().is_empty(),
            "nothing applied yet"
        );

        decide(&db, FLAVOR, p.id, Decision::Approve, NOW).unwrap();
        let applied = notes::list(&db, FLAVOR, NOW).unwrap();
        // What was previewed is what was applied.
        assert_eq!(applied[0].text, n.text);
        assert_eq!(applied[0].once, n.once);
        assert_eq!(applied[0].producer.as_deref(), Some("Claude Desktop"));
        let a = list(&db, FLAVOR, NOW).unwrap();
        assert!(a.waiting.is_empty());
        assert_eq!(a.decided[0].status, "applied");
        assert!(
            decide(&db, FLAVOR, p.id, Decision::Approve, NOW).is_err(),
            "only once"
        );
    }

    #[test]
    fn with_access_off_nothing_reaches_approvals() {
        let (tmp, db) = setup();
        drop_file(tmp.path(), LOGIN_NOTE, note("Valid in every other way"));
        ingest(&db, tmp.path(), FLAVOR, false, NOW).unwrap();
        let a = list(&db, FLAVOR, NOW).unwrap();
        assert!(a.waiting.is_empty());
        assert_eq!(a.decided[0].status, "rejected");
        assert_eq!(
            a.decided[0].status_reason.as_deref(),
            Some("agent access is off")
        );
        assert!(a.decided[0].note.is_none(), "no body is kept");
        assert!(inbox::waiting(tmp.path()).is_empty());
    }

    #[test]
    fn malformed_oversized_and_unknown_targets_are_rejected_with_a_reason() {
        let (tmp, db) = setup();
        let dir = inbox::dir(tmp.path());
        std::fs::create_dir_all(&dir).unwrap();
        let ulid = || ulid::Ulid::generate().to_string();
        std::fs::write(dir.join(format!("{}.json", ulid())), "not json").unwrap();
        std::fs::write(dir.join(format!("{}.json", ulid())), vec![b' '; 70_000]).unwrap();
        drop_file(tmp.path(), LOGIN_NOTE, note(""));
        drop_file(
            tmp.path(),
            LOGIN_NOTE,
            json!({ "character": "Nobody", "text": "x" }),
        );
        drop_file(
            tmp.path(),
            LOGIN_NOTE,
            json!({ "character": "Velyra", "text": "x", "script": "/run" }),
        );
        drop_file(
            tmp.path(),
            LOGIN_NOTE,
            json!({ "character": "Velyra", "text": "x", "until": "2001-01-01" }),
        );
        drop_file(
            tmp.path(),
            LOGIN_NOTE,
            json!({ "character": "Velyra", "text": "x", "replaces": { "id": 99, "text": "y" } }),
        );
        drop_file(tmp.path(), "sv_edit", json!({}));
        ingest(&db, tmp.path(), FLAVOR, true, NOW).unwrap();
        let a = list(&db, FLAVOR, NOW).unwrap();
        assert!(a.waiting.is_empty());
        let mut reasons: Vec<String> = a
            .decided
            .iter()
            .filter_map(|p| p.status_reason.clone())
            .collect();
        reasons.sort();
        assert_eq!(a.decided.len(), 8);
        for want in [
            "not read: it isn't a suggestion this version reads",
            "not read: it was larger than 64 KB",
            "a note needs some text",
            "No character named \"Nobody\". Known: Coinpurse, Velyra Duskmane.",
            "the note isn't in the form this version reads",
            "2001-01-01 has already passed",
            "note 99 isn't one of Velyra Duskmane's",
            "\"sv_edit\" isn't something Forever Buddy can apply yet",
        ] {
            assert!(
                reasons.iter().any(|r| r == want),
                "missing {want:?} in {reasons:?}"
            );
        }
    }

    #[test]
    fn a_note_changed_after_the_agent_read_it_is_a_conflict() {
        let (tmp, db) = setup();
        let mine = notes::add(
            &db,
            FLAVOR,
            &NewNote {
                character_id: 1,
                text: "Relist the bars".into(),
                once: true,
                until: None,
            },
            Author::You,
            NOW - 100,
        )
        .unwrap();
        // The agent read "Relist the bars"; then the player replaced it.
        drop_file(
            tmp.path(),
            LOGIN_NOTE,
            json!({ "character": "Coinpurse", "text": "Relist above 36g", "replaces": { "id": mine, "text": "Relist the bars" } }),
        );
        ingest(&db, tmp.path(), FLAVOR, true, NOW).unwrap();
        let id = list(&db, FLAVOR, NOW).unwrap().waiting[0].id;
        assert!(
            !list(&db, FLAVOR, NOW).unwrap().waiting[0]
                .note
                .as_ref()
                .unwrap()
                .replaces
                .as_ref()
                .unwrap()
                .conflict
        );

        notes::delete(&db, FLAVOR, mine, NOW).unwrap();
        let yours = notes::add(
            &db,
            FLAVOR,
            &NewNote {
                character_id: 1,
                text: "Relist above 38g".into(),
                once: true,
                until: None,
            },
            Author::You,
            NOW,
        )
        .unwrap();
        let r = list(&db, FLAVOR, NOW).unwrap().waiting[0]
            .note
            .clone()
            .unwrap()
            .replaces
            .unwrap();
        assert!(
            r.conflict && r.now.is_none(),
            "the note it replaces is gone"
        );
        assert!(
            decide(&db, FLAVOR, id, Decision::Approve, NOW).is_err(),
            "Approve won't pick for you"
        );
        decide(&db, FLAVOR, id, Decision::UseProposed, NOW).unwrap();
        let texts: Vec<String> = notes::list(&db, FLAVOR, NOW)
            .unwrap()
            .into_iter()
            .map(|n| n.text)
            .collect();
        assert!(texts.contains(&"Relist above 36g".to_string()));
        assert!(
            notes::active(&db, FLAVOR, yours, NOW).unwrap().is_some(),
            "a different note isn't touched"
        );
    }

    #[test]
    fn approving_a_replacement_retires_the_old_note_and_decline_changes_nothing() {
        let (tmp, db) = setup();
        let old = notes::add(
            &db,
            FLAVOR,
            &NewNote {
                character_id: 1,
                text: "Old".into(),
                once: true,
                until: None,
            },
            Author::You,
            NOW - 10,
        )
        .unwrap();
        drop_file(
            tmp.path(),
            LOGIN_NOTE,
            json!({ "character": "Coinpurse", "text": "New", "replaces": { "id": old, "text": "Old" } }),
        );
        drop_file(tmp.path(), LOGIN_NOTE, note("Declined one"));
        ingest(&db, tmp.path(), FLAVOR, true, NOW).unwrap();
        let w = list(&db, FLAVOR, NOW).unwrap().waiting;
        let (declined, replacing) = (w[0].id, w[1].id);
        decide(&db, FLAVOR, declined, Decision::Decline, NOW).unwrap();
        decide(&db, FLAVOR, replacing, Decision::Approve, NOW).unwrap();
        let texts: Vec<String> = notes::list(&db, FLAVOR, NOW)
            .unwrap()
            .into_iter()
            .map(|n| n.text)
            .collect();
        assert_eq!(texts, ["New"]);
    }

    #[test]
    fn past_the_pending_limit_new_ones_are_refused() {
        let (tmp, db) = setup();
        for i in 0..MAX_PENDING + 1 {
            drop_file(tmp.path(), LOGIN_NOTE, note(&format!("n{i}")));
        }
        ingest(&db, tmp.path(), FLAVOR, true, NOW).unwrap();
        let a = list(&db, FLAVOR, NOW).unwrap();
        assert_eq!(a.waiting.len() as i64, MAX_PENDING);
        assert_eq!(
            a.decided[0].status_reason.as_deref(),
            Some("50 suggestions were already waiting")
        );
    }
}
