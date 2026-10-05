//! A small append-only log in the app's log folder (`buddy.log`), for
//! things that happen in the background with nobody watching, like an
//! automatic backup failing. Never logs command arguments (secrets travel
//! through those). Rotates to `buddy.log.1` past 1 MB, so it stays small.

use std::io::Write;
use std::path::Path;

const FILE_NAME: &str = "buddy.log";
const MAX_BYTES: u64 = 1024 * 1024;

/// Appends one timestamped line. Logging must never take the app down, so
/// failures are ignored.
pub fn append(log_dir: &Path, line: &str) {
    let path = log_dir.join(FILE_NAME);
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = std::fs::rename(&path, log_dir.join(format!("{FILE_NAME}.1")));
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let line = line.replace(['\r', '\n'], " ");
        let _ = writeln!(file, "{} {line}", chrono::Utc::now().to_rfc3339());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_lines_and_rotates() {
        let tmp = tempfile::tempdir().unwrap();
        append(tmp.path(), "first");
        append(tmp.path(), "second\nline");
        let text = std::fs::read_to_string(tmp.path().join(FILE_NAME)).unwrap();
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].ends_with(" first") && lines[1].ends_with(" second line"));

        std::fs::write(
            tmp.path().join(FILE_NAME),
            vec![b'x'; MAX_BYTES as usize + 1],
        )
        .unwrap();
        append(tmp.path(), "fresh");
        assert!(tmp.path().join("buddy.log.1").exists());
        let text = std::fs::read_to_string(tmp.path().join(FILE_NAME)).unwrap();
        assert!(text.ends_with(" fresh\n"));
    }
}
