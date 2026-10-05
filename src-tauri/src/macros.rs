//! The Macros screen, read-only (F7): each account's and each character's
//! `macros-cache.txt`, parsed into macros to list and view. Nothing here
//! writes; editing comes later, through the write gate.
//!
//! The file is the game's own:
//!
//! ```text
//! VER 3 0000000000000093 "Judge + Seal" "Spell_Holy_RighteousFury"
//! #showtooltip Judgement
//! /cast Judgement
//! END
//! ```
//!
//! A header names the macro and its icon (a texture name or a file id), the
//! body follows, and `END` closes it. Account macros live in
//! `WTF/Account/<account>/macros-cache.txt`, character macros in
//! `WTF/Account/<account>/<group>/<character>/macros-cache.txt`. The text is
//! the player's (or pasted from anywhere), so it's read as data, capped in
//! size, and the UI renders it as text.

use std::path::Path;

use serde::Serialize;

use crate::fsx::read::safe_read;
use crate::install::layout::Flavor;
use crate::sessions;

/// The game's limit on a macro's body, in bytes.
pub const MACRO_MAX: u32 = 255;
/// A real file is a few KB (at most a few hundred macros); bigger isn't read.
const FILE_MAX: u64 = 1024 * 1024;
const FILE_NAME: &str = "macros-cache.txt";

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct Macro {
    pub name: String,
    /// A texture name (`INV_Misc_QuestionMark`) or an icon file id.
    pub icon: String,
    /// Lines joined with `\n`.
    pub body: String,
    /// The body's length as the game counts it (bytes), against
    /// `MACRO_MAX`.
    pub length: u32,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct AccountMacros {
    pub account: String,
    pub macros: Vec<Macro>,
    /// When the file last changed (RFC 3339): "as of logout, 3 Oct".
    pub modified: Option<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct CharacterMacros {
    pub account: String,
    pub group: String,
    /// The character folder, e.g. `Ellygie-Vargur`.
    pub folder: String,
    pub macros: Vec<Macro>,
    /// When the file last changed (RFC 3339); `None` without a file.
    pub modified: Option<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct MacrosList {
    /// The flavor folder, e.g. `_classic_beta_`.
    pub flavor: String,
    /// One per account folder, in name order.
    pub accounts: Vec<AccountMacros>,
    /// One per character on the WTF roster (older settings folders left
    /// out), in the Characters screen's order, macros or not.
    pub characters: Vec<CharacterMacros>,
    /// The longest body the game allows.
    pub max: u32,
}

/// `"Judge + Seal"` at the start of `s`: the text up to the closing quote,
/// and the rest.
fn quoted(s: &str) -> Option<(&str, &str)> {
    let s = s.trim_start().strip_prefix('"')?;
    let end = s.find('"')?;
    Some((&s[..end], &s[end + 1..]))
}

/// Every macro in one `macros-cache.txt`. A header that doesn't parse
/// starts no macro (its lines are skipped up to the next `END`), and a file
/// cut off before `END` keeps the macros before it.
pub fn parse(text: &str) -> Vec<Macro> {
    let mut out = Vec::new();
    let mut lines = text.lines().map(|l| l.strip_suffix('\r').unwrap_or(l));
    while let Some(line) = lines.next() {
        let Some(rest) = line.strip_prefix("VER ") else {
            continue;
        };
        // VER <version> <hex id> "<name>" "<icon>"
        let mut parts = rest.splitn(3, ' ');
        let (_version, _id, tail) = (parts.next(), parts.next(), parts.next().unwrap_or(""));
        let header = quoted(tail).and_then(|(name, rest)| Some((name, quoted(rest)?.0)));
        let mut body: Vec<&str> = Vec::new();
        let mut closed = false;
        for l in lines.by_ref() {
            if l == "END" {
                closed = true;
                break;
            }
            body.push(l);
        }
        if let (Some((name, icon)), true) = (header, closed) {
            let body = body.join("\n");
            out.push(Macro {
                name: name.to_string(),
                icon: icon.to_string(),
                length: u32::try_from(body.len()).unwrap_or(u32::MAX),
                body,
            });
        }
    }
    out
}

/// The macros in `dir`'s file, and when it last changed.
fn read_macros(dir: &Path) -> (Vec<Macro>, Option<String>) {
    let path = dir.join(FILE_NAME);
    let meta = match std::fs::metadata(&path) {
        Ok(m) if m.is_file() && m.len() <= FILE_MAX => m,
        _ => return (Vec::new(), None),
    };
    let modified = meta
        .modified()
        .ok()
        .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339());
    let macros = safe_read(&path)
        .map(|bytes| parse(&String::from_utf8_lossy(&bytes)))
        .unwrap_or_default();
    (macros, modified)
}

/// Every account's and every character's macros in the flavor's WTF.
pub fn list(flavor: &Flavor) -> MacrosList {
    let wtf = flavor.dir.join("WTF");
    let accounts_dir = wtf.join("Account");
    let mut accounts: Vec<String> = std::fs::read_dir(&accounts_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    accounts.sort();
    MacrosList {
        flavor: flavor.id.clone(),
        accounts: accounts
            .into_iter()
            .map(|account| {
                let (macros, modified) = read_macros(&accounts_dir.join(&account));
                AccountMacros {
                    account,
                    macros,
                    modified,
                }
            })
            .collect(),
        characters: sessions::wtf_characters(&wtf)
            .into_iter()
            .filter(|c| !c.older)
            .map(|c| {
                let r = c.character;
                let dir = accounts_dir.join(&r.account).join(&r.realm).join(&r.name);
                let (macros, modified) = read_macros(&dir);
                CharacterMacros {
                    account: r.account,
                    group: r.realm,
                    folder: r.name,
                    macros,
                    modified,
                }
            })
            .collect(),
        max: MACRO_MAX,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_games_format() {
        let text = "VER 3 00000000000000D9 \" \" \"INV_Misc_QuestionMark\"\r\n\
                    #show Insignia of the Alliance\r\n\
                    /cast !Stealth\r\n\
                    END\r\n\
                    \r\n\
                    VER 3 0000000000000093 \"Judge + Seal\" \"132118\"\n\
                    #showtooltip Judgement\n\
                    /cast [mod:shift] Seal of Light; Seal of Wisdom\n\
                    END\n";
        let m = parse(text);
        assert_eq!(m.len(), 2);
        assert_eq!(
            (m[0].name.as_str(), m[0].icon.as_str()),
            (" ", "INV_Misc_QuestionMark")
        );
        assert_eq!(
            m[0].body, "#show Insignia of the Alliance\n/cast !Stealth",
            "CRLF stripped"
        );
        assert_eq!(m[1].name, "Judge + Seal");
        assert_eq!(m[1].icon, "132118");
        assert_eq!(m[1].length as usize, m[1].body.len());
    }

    #[test]
    fn broken_entries_are_skipped_not_fatal() {
        let text = "VER 3 01 \"Good\" \"Icon\"\n/sit\nEND\n\
                    VER 3 02 no quotes here\n/dance\nEND\n\
                    VER 3 03 \"Cut\" \"Icon\"\n/wave\n";
        let m = parse(text);
        assert_eq!(m.len(), 1, "a bad header and a cut-off macro are left out");
        assert_eq!(m[0].body, "/sit");
        // An empty body is still a macro.
        assert_eq!(parse("VER 3 04 \"Empty\" \"Icon\"\nEND\n")[0].body, "");
    }

    #[test]
    fn length_counts_bytes_as_the_game_does() {
        let m = parse("VER 3 01 \"Ü\" \"I\"\n/say Grüße\nEND\n");
        assert_eq!(m[0].length, "/say Grüße".len() as u32);
        assert!(m[0].length > "/say Grüße".chars().count() as u32);
    }

    #[test]
    fn lists_account_and_character_macros() {
        let tmp = tempfile::tempdir().unwrap();
        let wtf = tmp.path().join("WTF/Account/A");
        std::fs::create_dir_all(wtf.join("70/Thrandor")).unwrap();
        std::fs::create_dir_all(wtf.join("70/Sela")).unwrap();
        std::fs::write(
            wtf.join(FILE_NAME),
            "VER 3 01 \"Ready\" \"I\"\n/readycheck\nEND\n",
        )
        .unwrap();
        std::fs::write(
            wtf.join("70/Thrandor").join(FILE_NAME),
            "VER 3 02 \"Mount\" \"I\"\n/cast Summon Warhorse\nEND\n",
        )
        .unwrap();
        let flavor = Flavor {
            id: "_classic_beta_".into(),
            label: "WoW: Forever (Beta)".into(),
            product: None,
            version: None,
            is_forever: true,
            dir: tmp.path().to_path_buf(),
            exe: None,
            has_wtf: true,
            accounts: vec!["A".into()],
            characters: 2,
            links: Vec::new(),
        };
        let l = list(&flavor);
        assert_eq!(l.accounts.len(), 1);
        assert_eq!(l.accounts[0].macros[0].name, "Ready");
        let thrandor = l
            .characters
            .iter()
            .find(|c| c.folder == "Thrandor")
            .unwrap();
        assert_eq!(thrandor.macros[0].body, "/cast Summon Warhorse");
        let sela = l.characters.iter().find(|c| c.folder == "Sela").unwrap();
        assert!(sela.macros.is_empty(), "no file: no macros, still listed");
        assert!(sela.modified.is_none() && thrandor.modified.is_some());
        assert_eq!(l.max, 255);
    }
}
