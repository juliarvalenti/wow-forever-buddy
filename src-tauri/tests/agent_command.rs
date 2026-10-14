//! G2 (BUG-MCP): the exact line Settings › Agents gives for Claude Code,
//! pasted into a real shell, reaches `claude` with the app's path in one
//! piece, and the app started with what `claude` got answers MCP.
//!
//! The app is copied to a path like a real install's (spaces and
//! parentheses). A stub `claude` (a `claude.cmd` shim on Windows, as npm
//! installs it; a shell script elsewhere; both hand over to Python, which
//! splits a Windows command line the way an exe does) records the
//! arguments it was given. Each shell the platform has runs the line:
//! PowerShell 7, Windows PowerShell and cmd on Windows; sh, bash and zsh
//! on macOS and Linux. Shells that aren't installed are skipped, and say so.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use wow_forever_buddy_lib::agent;

const STUB: &str = r#"import json, os, sys
with open(os.environ["CLAUDE_STUB_OUT"], "w", encoding="utf-8") as f:
    json.dump(sys.argv[1:], f)
"#;

fn python() -> Option<&'static str> {
    // On Windows `python3` can be the Store's stub; `python` is the real one.
    let names = if cfg!(windows) {
        ["python", "python3"]
    } else {
        ["python3", "python"]
    };
    names.into_iter().find(|p| {
        Command::new(p)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

/// The app at a path like `C:\Program Files (x86)\WoW Forever Buddy\…`.
fn installed_app(dir: &Path) -> PathBuf {
    let src = PathBuf::from(env!("CARGO_BIN_EXE_wow-forever-buddy"));
    let to = dir.join("Program Files (x86)").join("WoW Forever Buddy");
    std::fs::create_dir_all(&to).unwrap();
    let exe = to.join(src.file_name().unwrap());
    std::fs::copy(&src, &exe).unwrap();
    exe
}

/// A `claude` on PATH that records its arguments.
fn stub_claude(dir: &Path, python: &str) -> PathBuf {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("stub.py"), STUB).unwrap();
    if cfg!(windows) {
        std::fs::write(
            bin.join("claude.cmd"),
            format!("@echo off\r\n{python} \"%~dp0stub.py\" %*\r\n"),
        )
        .unwrap();
    } else {
        let script = bin.join("claude");
        std::fs::write(
            &script,
            format!("#!/bin/sh\nexec {python} \"$(dirname \"$0\")/stub.py\" \"$@\"\n"),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    bin
}

/// The shells to paste into, each given `line` exactly as typed: Rust's own
/// argument quoting must not touch it, or the test would check that
/// instead of the shell.
fn shells() -> Vec<&'static str> {
    if cfg!(windows) {
        vec!["pwsh", "powershell", "cmd"]
    } else {
        vec!["sh", "bash", "zsh"]
    }
}

#[cfg(windows)]
fn run_in(shell: &str, line: &str) -> Command {
    use std::os::windows::process::CommandExt;
    let mut c = Command::new(shell);
    if shell == "cmd" {
        // /s: cmd drops the outer quotes and runs the rest as typed.
        c.raw_arg(format!("/d /s /c \"{line}\""));
    } else {
        // PowerShell's own way to take a command verbatim.
        c.args(["-NoProfile", "-NonInteractive", "-EncodedCommand"])
            .arg(utf16_base64(line));
    }
    c
}

#[cfg(not(windows))]
fn run_in(shell: &str, line: &str) -> Command {
    let mut c = Command::new(shell);
    c.arg("-c").arg(line);
    c
}

/// Base64 of UTF-16LE, what -EncodedCommand takes.
#[cfg(windows)]
fn utf16_base64(s: &str) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes: Vec<u8> = s.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | ((*b as u32) << (16 - 8 * i)));
        for i in 0..4usize {
            out.push(if i <= chunk.len() {
                T[((n >> (18 - 6 * i)) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    out
}

/// The server, started with what `claude` would start it with, answers an
/// MCP `initialize`.
fn answers_mcp(args: &[String]) {
    let mut child = Command::new(&args[0])
        .args(&args[1..])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|e| panic!("start {args:?}: {e}"));
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":"2025-06-18","capabilities":{{}},"clientInfo":{{"name":"ci","version":"1"}}}}}}"#
    )
    .unwrap();
    stdin.flush().unwrap();
    let mut reply = String::new();
    out.read_line(&mut reply).unwrap();
    let reply: serde_json::Value =
        serde_json::from_str(&reply).unwrap_or_else(|e| panic!("not JSON ({e}): {reply:?}"));
    assert!(reply["result"]["serverInfo"]["name"].is_string(), "{reply}");
    drop(stdin);
    let _ = child.wait();
}

#[test]
fn the_settings_line_survives_each_shell() {
    let Some(python) = python() else {
        eprintln!("skipped: no Python for the stub claude");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let exe = installed_app(dir.path());
    let bin = stub_claude(dir.path(), python);
    let line = agent::claude_code_command(&exe.to_string_lossy());
    let path = std::env::join_paths(
        std::iter::once(bin.clone())
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();

    let mut ran = 0;
    for shell in shells() {
        let out = dir.path().join(format!("{shell}.json"));
        let status = match run_in(shell, &line)
            .env("PATH", &path)
            .env("CLAUDE_STUB_OUT", &out)
            .stdin(Stdio::null())
            .status()
        {
            Ok(s) => s,
            Err(_) => {
                eprintln!("skipped: {shell} isn't installed");
                continue;
            }
        };
        assert!(status.success(), "{shell}: {line}");
        let got: Vec<String> = serde_json::from_str(
            &std::fs::read_to_string(&out)
                .unwrap_or_else(|e| panic!("{shell}: the stub claude didn't run ({e}): {line}")),
        )
        .unwrap();
        assert_eq!(
            got,
            [
                "mcp".to_string(),
                "add".into(),
                "forever-buddy".into(),
                exe.to_string_lossy().into_owned(),
                agent::ARG.into(),
            ],
            "{shell}: {line}"
        );
        answers_mcp(&got[3..]);
        ran += 1;
    }
    assert!(ran >= 2, "at least two shells ran the line");
}
