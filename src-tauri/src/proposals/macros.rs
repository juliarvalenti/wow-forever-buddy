//! MC1 (issue #262): macros an agent proposes. A new macro, an edit, or a
//! copy of one to another character; never a delete (the player's own job,
//! security on MC1).
//!
//! The app never writes `macros-cache.txt`: the game keeps macros on the
//! server and would put the old ones back. An approved macro waits in
//! `macro_queue`, the Macros slot carries it to the addon, and the player's
//! click in the game applies it with the game's own macro calls, out of
//! combat. So each one is approved on its own, never with Approve all.
//!
//! Everything is checked before anything's queued, here in the agent's call
//! and again when the app picks the file up: the game's 255 bytes and 16
//! character names, the 120 account and 18 character slots, a character or
//! account that exists, an edit's old text still being what the game has,
//! and no lines that run code (`/run`, `/script`, `/console`, `/dump`): a
//! macro from an agent is slash commands the player presses.

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::characters;
use crate::db::Db;
use crate::error::AppResult;
use crate::macros::{Macro, MacrosList, MACRO_MAX};

/// The game's slots: account-wide macros, and each character's own.
pub const MAX_ACCOUNT_MACROS: usize = 120;
pub const MAX_CHARACTER_MACROS: usize = 18;
/// The game's macro name box takes 16 characters.
pub const MAX_NAME: usize = 16;
const MAX_ICON: usize = 64;
const DEFAULT_ICON: &str = "INV_Misc_QuestionMark";
/// Commands that run code in the client, not as a button would.
const REFUSED: [&str; 4] = ["/run", "/script", "/console", "/dump"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum MacroAction {
    New,
    Edit,
    /// Only in a proposal: a copy is stored and applied as a new macro.
    Copy,
}

/// A copy's source: a character's macro, or an account-wide one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MacroSource {
    #[serde(default)]
    pub character: Option<String>,
    #[serde(default)]
    pub account: Option<u32>,
    pub name: String,
}

/// A macro as an agent proposes it (`propose_macro`'s arguments).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MacroProposal {
    pub action: MacroAction,
    /// Whose macro: a character, or none for the account-wide macros.
    #[serde(default)]
    pub character: Option<String>,
    /// Which account, by list_macros' number, for an account-wide macro
    /// when there's more than one.
    #[serde(default)]
    pub account: Option<u32>,
    pub name: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    /// An edit: the macro's text as get_macro gave it.
    #[serde(default)]
    pub was: Option<String>,
    /// A copy: the macro to copy.
    #[serde(default)]
    pub from: Option<MacroSource>,
}

/// Whose macro, once checked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Target {
    /// The account folder: the app's own record, never sent to an agent or
    /// the game.
    pub account: String,
    /// A character's own macro; `None` for an account-wide one.
    pub character_id: Option<u32>,
    /// The characters the game may apply it on, `(name, surname)`: the
    /// one, or every character Forever Buddy knows on the account.
    pub on: Vec<(String, Option<String>)>,
    /// "Velyra Duskmane", or "Every character on the account".
    pub label: String,
}

/// A checked macro proposal: what's stored, approved and sent to the game.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MacroBody {
    /// `New` or `Edit`.
    pub action: MacroAction,
    pub target: Target,
    pub name: String,
    pub icon: String,
    pub body: String,
    /// An edit: the text it replaces. The addon applies the edit only if
    /// the game still has this.
    pub old_body: Option<String>,
    /// A copy: where it came from, "Velyra Duskmane's Mount".
    pub copied_from: Option<String>,
}

/// The Approvals preview: every field Apply uses, verbatim.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct MacroView {
    pub action: MacroAction,
    pub target: String,
    pub name: String,
    pub icon: String,
    pub body: String,
    pub length: u32,
    pub old_body: Option<String>,
    pub copied_from: Option<String>,
}

pub fn view(b: &MacroBody) -> MacroView {
    MacroView {
        action: b.action,
        target: b.target.label.clone(),
        name: b.name.clone(),
        icon: b.icon.clone(),
        body: b.body.clone(),
        length: u32::try_from(b.body.len()).unwrap_or(u32::MAX),
        old_body: b.old_body.clone(),
        copied_from: b.copied_from.clone(),
    }
}

struct Known {
    id: u32,
    name: String,
    surname: Option<String>,
    account: String,
    group: String,
    dir: String,
}

fn known(db: &Db, flavor: &str) -> Result<Vec<Known>, String> {
    db.with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT id, name, surname, account, group_dir, char_dir FROM visible_characters
             WHERE flavor = ?1 ORDER BY name, surname",
        )?;
        let rows = stmt
            .query_map(params![flavor], |r| {
                Ok(Known {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    surname: r.get(2)?,
                    account: r.get(3)?,
                    group: r.get(4)?,
                    dir: r.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .map_err(|e| e.to_string())
}

fn full(k: &Known) -> String {
    match k.surname.as_deref() {
        Some(s) if !s.is_empty() => format!("{} {s}", k.name),
        _ => k.name.clone(),
    }
}

/// A scope's target and the macros it has now.
fn scope<'a>(
    db: &Db,
    flavor: &str,
    files: &'a MacrosList,
    known: &[Known],
    character: Option<&str>,
    account: Option<u32>,
) -> Result<(Target, &'a [Macro]), String> {
    if let Some(key) = character {
        if account.is_some() {
            return Err("Give a character or an account, not both.".into());
        }
        let card = characters::by_name(db, flavor, key).map_err(|e| e.to_string())?;
        let k = known
            .iter()
            .find(|k| k.id == card.id)
            .ok_or("That character isn't in the game folder Forever Buddy reads.")?;
        let now: &[Macro] = files
            .characters
            .iter()
            .find(|c| {
                c.account == k.account
                    && c.group == k.group
                    && c.folder.eq_ignore_ascii_case(&k.dir)
            })
            .map_or(&[], |c| &c.macros);
        return Ok((
            Target {
                account: k.account.clone(),
                character_id: Some(k.id),
                on: vec![(k.name.clone(), k.surname.clone())],
                label: full(k),
            },
            now,
        ));
    }
    let acc =
        match (account, files.accounts.len()) {
            (Some(n), _) => files
                .accounts
                .get((n as usize).wrapping_sub(1))
                .ok_or("There's no account with that number: list_macros numbers them.")?,
            (None, 1) => &files.accounts[0],
            (None, 0) => return Err("The game folder has no accounts yet.".into()),
            (None, _) => return Err(
                "There's more than one account: say which with account (list_macros numbers them)."
                    .into(),
            ),
        };
    let on: Vec<(String, Option<String>)> = known
        .iter()
        .filter(|k| k.account == acc.account)
        .map(|k| (k.name.clone(), k.surname.clone()))
        .collect();
    if on.is_empty() {
        return Err(
            "Forever Buddy doesn't know a character on that account yet, so the game couldn't apply it."
                .into(),
        );
    }
    Ok((
        Target {
            account: acc.account.clone(),
            character_id: None,
            on,
            label: "Every character on the account".into(),
        },
        &acc.macros,
    ))
}

fn check_name(name: &str) -> Result<(), String> {
    let n = name.chars().count();
    if name.trim().is_empty() || n > MAX_NAME {
        return Err(format!("A macro's name is 1 to {MAX_NAME} characters."));
    }
    if name.chars().any(|c| c.is_control() || c == '"') {
        return Err("A macro's name can't have quotes or line breaks.".into());
    }
    Ok(())
}

fn check_icon(icon: &str) -> Result<(), String> {
    if icon.is_empty()
        || icon.len() > MAX_ICON
        || !icon.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(
            "The icon is a texture name like INV_Misc_QuestionMark, or an icon file id.".into(),
        );
    }
    Ok(())
}

/// The game's limit, plain text only, and no line that runs code.
pub fn check_body(body: &str) -> Result<(), String> {
    if body.trim().is_empty() {
        return Err("The macro has no text.".into());
    }
    if body.len() > MACRO_MAX as usize {
        return Err(format!(
            "The macro is {} bytes; the game allows {MACRO_MAX}.",
            body.len()
        ));
    }
    if body.chars().any(|c| c.is_control() && c != '\n') {
        return Err("Only lines of plain text: no tabs or other control characters.".into());
    }
    for line in body.lines() {
        let first = line
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        if REFUSED.contains(&first.as_str()) {
            return Err(format!(
                "{first} lines aren't allowed: a macro from an agent is slash commands the player presses, never code."
            ));
        }
    }
    Ok(())
}

/// Approved macros the game hasn't applied yet, for this target.
fn queued(db: &Db, flavor: &str, t: &Target) -> Result<usize, String> {
    let rows: Vec<String> = db
        .with_conn(|c| {
            let mut stmt =
                c.prepare("SELECT body FROM macro_queue WHERE flavor = ?1 AND done_at IS NULL")?;
            let rows = stmt
                .query_map(params![flavor], |r| r.get(0))?
                .collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        })
        .map_err(|e| e.to_string())?;
    Ok(rows
        .iter()
        .filter_map(|b| serde_json::from_str::<MacroBody>(b).ok())
        .filter(|b| {
            b.action == MacroAction::New
                && b.target.account == t.account
                && b.target.character_id == t.character_id
        })
        .count())
}

fn room(db: &Db, flavor: &str, t: &Target, now: &[Macro]) -> Result<(), String> {
    let (max, whose) = match t.character_id {
        Some(_) => (MAX_CHARACTER_MACROS, "That character"),
        None => (MAX_ACCOUNT_MACROS, "The account"),
    };
    if now.len() + queued(db, flavor, t)? >= max {
        return Err(format!(
            "{whose} has no free macro slot: the game allows {max}, counting ones already approved."
        ));
    }
    Ok(())
}

fn one<'a>(macros: &'a [Macro], name: &str) -> Result<Option<&'a Macro>, String> {
    let mut same = macros.iter().filter(|m| m.name.eq_ignore_ascii_case(name));
    let first = same.next();
    if same.next().is_some() {
        return Err(
            "More than one macro there has that name, so the player has to pick which one.".into(),
        );
    }
    Ok(first)
}

/// Checks a proposal against the player's macros now (`files`).
pub fn check(
    db: &Db,
    flavor: &str,
    files: &MacrosList,
    p: &MacroProposal,
) -> Result<MacroBody, String> {
    check_name(&p.name)?;
    if let Some(icon) = &p.icon {
        check_icon(icon)?;
    }
    let known = known(db, flavor)?;
    let (target, now) = scope(db, flavor, files, &known, p.character.as_deref(), p.account)?;
    let existing = one(now, &p.name)?;
    match p.action {
        MacroAction::New => {
            if p.was.is_some() || p.from.is_some() {
                return Err("A new macro takes a name and text, not was or from.".into());
            }
            let body = p.body.as_deref().ok_or("A new macro needs its text.")?;
            check_body(body)?;
            if existing.is_some() {
                return Err(
                    "There's already a macro with that name there: propose an edit instead.".into(),
                );
            }
            room(db, flavor, &target, now)?;
            Ok(MacroBody {
                action: MacroAction::New,
                target,
                name: p.name.clone(),
                icon: p.icon.clone().unwrap_or_else(|| DEFAULT_ICON.into()),
                body: body.to_string(),
                old_body: None,
                copied_from: None,
            })
        }
        MacroAction::Edit => {
            if p.from.is_some() {
                return Err("An edit takes the new text and was, not from.".into());
            }
            let body = p.body.as_deref().ok_or("An edit needs the new text.")?;
            check_body(body)?;
            let was = p
                .was
                .as_deref()
                .ok_or("An edit needs was: the macro's text as get_macro gave it.")?;
            let m = existing.ok_or("There's no macro with that name there to edit.")?;
            if m.body != was {
                return Err(
                    "The macro changed since you read it: read it again with get_macro.".into(),
                );
            }
            if m.body == body && p.icon.as_ref().is_none_or(|i| *i == m.icon) {
                return Err("That's the macro as it is now.".into());
            }
            Ok(MacroBody {
                action: MacroAction::Edit,
                target,
                name: m.name.clone(),
                icon: p.icon.clone().unwrap_or_else(|| m.icon.clone()),
                body: body.to_string(),
                old_body: Some(m.body.clone()),
                copied_from: None,
            })
        }
        MacroAction::Copy => {
            if p.body.is_some() || p.was.is_some() {
                return Err("A copy takes from, not text or was.".into());
            }
            let from = p
                .from
                .as_ref()
                .ok_or("A copy needs from: the macro to copy.")?;
            let (source, there) = scope(
                db,
                flavor,
                files,
                &known,
                from.character.as_deref(),
                from.account,
            )?;
            if source.account == target.account && source.character_id == target.character_id {
                return Err(
                    "That's where the macro already is: copy it to another character.".into(),
                );
            }
            let m = one(there, &from.name)?.ok_or("There's no macro with that name to copy.")?;
            check_body(&m.body)?;
            if existing.is_some() {
                return Err(
                    "There's already a macro with that name there: pick another name.".into(),
                );
            }
            room(db, flavor, &target, now)?;
            let whose = match source.character_id {
                Some(_) => format!("{}'s {}", source.label, m.name),
                None => format!("the account's {}", m.name),
            };
            Ok(MacroBody {
                action: MacroAction::New,
                target,
                name: p.name.clone(),
                icon: p.icon.clone().unwrap_or_else(|| m.icon.clone()),
                body: m.body.clone(),
                old_body: None,
                copied_from: Some(whose),
            })
        }
    }
}

/// Approve: the macro waits for the game in `macro_queue`, and the Macros
/// slot carries it there.
pub fn apply(db: &Db, flavor: &str, proposal: i64, b: &MacroBody, now: i64) -> AppResult<()> {
    let body = serde_json::to_string(b).map_err(|e| crate::error::AppError::Db(e.to_string()))?;
    db.with_conn(|c| {
        c.execute(
            "INSERT INTO macro_queue (flavor, proposal_id, body, approved_at) VALUES (?1, ?2, ?3, ?4)",
            params![flavor, proposal, body, now],
        )?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::macros::{AccountMacros, CharacterMacros};

    const FLAVOR: &str = "_classic_beta_";

    fn m(name: &str, body: &str) -> Macro {
        Macro {
            name: name.into(),
            icon: "I".into(),
            body: body.into(),
            length: body.len() as u32,
        }
    }

    fn setup() -> (Db, MacrosList) {
        let db = Db::open_in_memory().unwrap();
        db.with_conn(|c| {
            for (name, surname, account) in [
                ("Velyra", Some("Duskmane"), "A"),
                ("Coinpurse", None, "A"),
                ("Brannic", None, "B"),
            ] {
                c.execute(
                    "INSERT INTO characters (flavor, account, group_dir, char_dir, name, surname,
                                             first_seen, last_seen)
                     VALUES (?1, ?2, '70', ?3, ?3, ?4, 0, 0)",
                    params![FLAVOR, account, name, surname],
                )?;
            }
            Ok(())
        })
        .unwrap();
        let files = MacrosList {
            flavor: FLAVOR.into(),
            accounts: vec![
                AccountMacros {
                    account: "A".into(),
                    macros: vec![m("Ready", "/readycheck")],
                    modified: None,
                },
                AccountMacros {
                    account: "B".into(),
                    macros: vec![],
                    modified: None,
                },
            ],
            characters: vec![
                CharacterMacros {
                    account: "A".into(),
                    group: "70".into(),
                    folder: "Velyra".into(),
                    macros: vec![m("Mount", "/cast Summon Warhorse")],
                    modified: None,
                },
                CharacterMacros {
                    account: "A".into(),
                    group: "70".into(),
                    folder: "Coinpurse".into(),
                    macros: vec![],
                    modified: None,
                },
            ],
            max: MACRO_MAX,
        };
        (db, files)
    }

    fn new(character: Option<&str>, name: &str, body: &str) -> MacroProposal {
        MacroProposal {
            action: MacroAction::New,
            character: character.map(str::to_string),
            account: None,
            name: name.into(),
            body: Some(body.into()),
            icon: None,
            was: None,
            from: None,
        }
    }

    #[test]
    fn a_new_macro_is_checked_and_targets_one_character() {
        let (db, files) = setup();
        let b = check(
            &db,
            FLAVOR,
            &files,
            &new(Some("Velyra"), "Evasion", "#showtooltip\n/cast Evasion"),
        )
        .unwrap();
        assert_eq!(b.action, MacroAction::New);
        assert_eq!(
            b.target.on,
            vec![("Velyra".to_string(), Some("Duskmane".to_string()))]
        );
        assert_eq!(b.target.label, "Velyra Duskmane");
        assert_eq!(b.icon, DEFAULT_ICON);
        let e = check(&db, FLAVOR, &files, &new(Some("Velyra"), "mount", "/sit")).unwrap_err();
        assert!(e.contains("propose an edit"), "{e}");
        let e = check(&db, FLAVOR, &files, &new(Some("Nobody"), "X", "/sit")).unwrap_err();
        assert!(e.contains("No character named"), "{e}");
    }

    #[test]
    fn code_and_oversized_text_are_refused() {
        let (db, files) = setup();
        for body in [
            "/run print(1)",
            "/sit\n  /SCRIPT x()",
            "/console gamma 2",
            "/dump GetMoney()",
        ] {
            let e = check(&db, FLAVOR, &files, &new(Some("Velyra"), "Bad", body)).unwrap_err();
            assert!(e.contains("never code"), "{body}: {e}");
        }
        let long = format!("/say {}", "a".repeat(260));
        assert!(
            check(&db, FLAVOR, &files, &new(Some("Velyra"), "Long", &long))
                .unwrap_err()
                .contains("255")
        );
        assert!(check(&db, FLAVOR, &files, &new(Some("Velyra"), "Tab", "/sit\t")).is_err());
        assert!(check(
            &db,
            FLAVOR,
            &files,
            &new(Some("Velyra"), "Seventeen chars!!", "/sit")
        )
        .is_err());
        assert!(check(&db, FLAVOR, &files, &new(Some("Velyra"), "Quo\"te", "/sit")).is_err());
        // Slash commands a player presses are fine.
        let ok = "#showtooltip\n/castsequence reset=3 Sinister Strike, Eviscerate\n/stopmacro [nocombat]\n/click ActionButton1";
        assert!(check(&db, FLAVOR, &files, &new(Some("Velyra"), "Fine", ok)).is_ok());
    }

    #[test]
    fn account_macros_need_a_known_account() {
        let (db, files) = setup();
        let e = check(&db, FLAVOR, &files, &new(None, "Hi", "/wave")).unwrap_err();
        assert!(e.contains("more than one account"), "{e}");
        let mut p = new(None, "Hi", "/wave");
        p.account = Some(1);
        let b = check(&db, FLAVOR, &files, &p).unwrap();
        assert_eq!(b.target.character_id, None);
        assert_eq!(b.target.on.len(), 2, "every known character on account A");
        p.account = Some(3);
        assert!(check(&db, FLAVOR, &files, &p).is_err());
    }

    #[test]
    fn an_edit_must_match_what_the_game_has() {
        let (db, files) = setup();
        let mut p = new(Some("Velyra"), "Mount", "/cast Summon Charger");
        p.action = MacroAction::Edit;
        assert!(check(&db, FLAVOR, &files, &p).unwrap_err().contains("was"));
        p.was = Some("/cast something else".into());
        assert!(check(&db, FLAVOR, &files, &p)
            .unwrap_err()
            .contains("changed since"));
        p.was = Some("/cast Summon Warhorse".into());
        let b = check(&db, FLAVOR, &files, &p).unwrap();
        assert_eq!(b.old_body.as_deref(), Some("/cast Summon Warhorse"));
        assert_eq!(b.icon, "I", "the icon stays unless changed");
        p.body = Some("/cast Summon Warhorse".into());
        assert!(check(&db, FLAVOR, &files, &p)
            .unwrap_err()
            .contains("as it is now"));
    }

    #[test]
    fn a_copy_is_a_new_macro_on_another_character() {
        let (db, files) = setup();
        let p = MacroProposal {
            action: MacroAction::Copy,
            character: Some("Coinpurse".into()),
            account: None,
            name: "Mount".into(),
            body: None,
            icon: None,
            was: None,
            from: Some(MacroSource {
                character: Some("Velyra".into()),
                account: None,
                name: "Mount".into(),
            }),
        };
        let b = check(&db, FLAVOR, &files, &p).unwrap();
        assert_eq!(b.action, MacroAction::New);
        assert_eq!(b.body, "/cast Summon Warhorse");
        assert_eq!(b.copied_from.as_deref(), Some("Velyra Duskmane's Mount"));
        let mut same = p.clone();
        same.character = Some("Velyra".into());
        assert!(check(&db, FLAVOR, &files, &same)
            .unwrap_err()
            .contains("already is"));
    }

    #[test]
    fn slots_count_what_is_already_approved() {
        let (db, mut files) = setup();
        files.characters[1].macros = (0..17).map(|i| m(&format!("M{i}"), "/sit")).collect();
        let b = check(&db, FLAVOR, &files, &new(Some("Coinpurse"), "Last", "/sit")).unwrap();
        db.with_conn(|c| {
            c.execute(
                "INSERT INTO proposals (id, flavor, file, kind, producer, created_at, received_at, status)
                 VALUES (1, ?1, 'f', 'macro', 'test', 0, 0, 'applied')",
                params![FLAVOR],
            )?;
            Ok(())
        })
        .unwrap();
        apply(&db, FLAVOR, 1, &b, 0).unwrap();
        let e = check(&db, FLAVOR, &files, &new(Some("Coinpurse"), "Over", "/sit")).unwrap_err();
        assert!(e.contains("no free macro slot"), "{e}");
    }
}
