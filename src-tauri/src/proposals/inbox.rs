//! The agent inbox (spec §4): one JSON file per proposal in
//! `<local data>/agent/inbox/`, written by the agent process and read,
//! checked and deleted by the app. The app treats every file as untrusted:
//! any local process can write this folder.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};
use crate::fsx::atomic::atomic_replace;

/// The file format this build writes and reads.
pub const VERSION: u32 = 1;
/// Spec §5: a larger file is refused unread.
pub const MAX_FILE: u64 = 64 * 1024;
/// Spec §5: the agent process stops writing past this many unread files.
pub const MAX_FILES: usize = 200;

/// One proposal as the agent process writes it. The producer and the body
/// are claims until the app has checked them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InboxFile {
    pub v: u32,
    /// The client's own name for itself (MCP `clientInfo.name`).
    pub producer: String,
    /// `login_note` in P2b.
    pub kind: String,
    /// Unix seconds.
    pub created_at: i64,
    pub reason: Option<String>,
    pub body: serde_json::Value,
}

pub fn dir(agent_dir: &Path) -> PathBuf {
    agent_dir.join("inbox")
}

/// A ULID file name: what the agent process writes. Anything else in the
/// folder (an atomic write's temp file, a stray) is left alone.
fn is_proposal_name(name: &str) -> bool {
    name.len() == 31
        && name.ends_with(".json")
        && name[..26]
            .bytes()
            .all(|b| b.is_ascii_digit() || b.is_ascii_uppercase())
}

/// The proposal files waiting, oldest first (ULIDs sort by time).
pub fn waiting(agent_dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir(agent_dir)) else {
        return Vec::new();
    };
    let mut files: Vec<(String, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_string();
            is_proposal_name(&name).then(|| (name, e.path()))
        })
        .collect();
    files.sort();
    files
}

/// Writes a proposal for the app to pick up. Returns its id (the ULID).
pub fn write(agent_dir: &Path, file: &InboxFile) -> AppResult<String> {
    if waiting(agent_dir).len() >= MAX_FILES {
        return Err(AppError::InvalidSettings(format!(
            "{MAX_FILES} suggestions are waiting for Forever Buddy to pick them up. Open the app, then try again."
        )));
    }
    let bytes = serde_json::to_vec(file).map_err(|e| AppError::Io(e.to_string()))?;
    if bytes.len() as u64 > MAX_FILE {
        return Err(AppError::InvalidSettings(
            "That suggestion is too large.".into(),
        ));
    }
    let id = ulid::Ulid::generate().to_string();
    atomic_replace(&dir(agent_dir).join(format!("{id}.json")), &bytes)?;
    Ok(id)
}

/// Reads a waiting file, refusing one past the size limit.
pub fn read(path: &Path) -> Result<InboxFile, &'static str> {
    let len = std::fs::metadata(path).map(|m| m.len()).unwrap_or(u64::MAX);
    if len > MAX_FILE {
        return Err("it was larger than 64 KB");
    }
    let bytes = std::fs::read(path).map_err(|_| "it couldn't be read")?;
    let file: InboxFile =
        serde_json::from_slice(&bytes).map_err(|_| "it isn't a suggestion this version reads")?;
    if file.v != VERSION {
        return Err("it's from another version of Forever Buddy");
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file() -> InboxFile {
        InboxFile {
            v: VERSION,
            producer: "Claude Desktop".into(),
            kind: "login_note".into(),
            created_at: 100,
            reason: None,
            body: serde_json::json!({}),
        }
    }

    #[test]
    fn written_files_are_listed_oldest_first_and_strays_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let a = write(tmp.path(), &file()).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = write(tmp.path(), &file()).unwrap();
        std::fs::write(dir(tmp.path()).join("notes.txt"), "x").unwrap();
        std::fs::write(dir(tmp.path()).join(".x.json.wfb-tmp-1"), "x").unwrap();
        let names: Vec<String> = waiting(tmp.path()).into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, [format!("{a}.json"), format!("{b}.json")]);
        assert_eq!(read(&waiting(tmp.path())[0].1).unwrap(), file());
    }

    #[test]
    fn bad_files_are_refused_with_a_reason() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("f.json");
        std::fs::write(&p, vec![b' '; MAX_FILE as usize + 1]).unwrap();
        assert_eq!(read(&p), Err("it was larger than 64 KB"));
        std::fs::write(
            &p,
            r#"{"v":1,"producer":"x","kind":"login_note","created_at":1,"body":{},"extra":1}"#,
        )
        .unwrap();
        assert_eq!(read(&p), Err("it isn't a suggestion this version reads"));
        std::fs::write(
            &p,
            r#"{"v":2,"producer":"x","kind":"login_note","created_at":1,"reason":null,"body":{}}"#,
        )
        .unwrap();
        assert_eq!(read(&p), Err("it's from another version of Forever Buddy"));
    }

    #[test]
    fn the_agent_stops_at_the_file_limit() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir(tmp.path())).unwrap();
        for _ in 0..MAX_FILES {
            let id = ulid::Ulid::generate().to_string();
            std::fs::write(dir(tmp.path()).join(format!("{id}.json")), "{}").unwrap();
        }
        assert!(write(tmp.path(), &file()).is_err());
    }
}
