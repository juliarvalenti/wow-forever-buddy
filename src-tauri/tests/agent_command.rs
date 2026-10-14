//! G2 (BUG-MCP): the exact line Settings › Agents gives for Claude Code,
//! pasted into a real shell, reaches `claude` with the app's path in one
//! piece, and the app started with what `claude` got answers MCP.
//!
//! The app is copied to a path like a real install's (spaces and
//! parentheses). A stub `claude` records which shim ran and the arguments
//! it was given. On Windows it's both shims npm installs, `claude.ps1`
//! (which PowerShell prefers, and where BUG-MCP's `--` was lost) and
//! `claude.cmd`; elsewhere a shell script. All hand over to Python, which
//! splits a Windows command line the way an exe does. Each shell the platform has runs the line:
//! PowerShell 7, Windows PowerShell and cmd on Windows; sh, bash and zsh
//! on macOS and Linux. Shells that aren't installed are skipped, and say so.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use wow_forever_buddy_lib::agent;

const STUB: &str = r#"import json, os, sys
with open(os.environ["CLAUDE_STUB_OUT"], "w", encoding="utf-8") as f:
    json.dump({"shim": os.environ.get("CLAUDE_STUB_SHIM"), "args": sys.argv[1:]}, f)
"#;

/// What the stub claude recorded: which shim ran, and its arguments.
#[derive(serde::Deserialize)]
struct Got {
    shim: Option<String>,
    args: Vec<String>,
}

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
        // Both shims npm installs. PowerShell prefers claude.ps1, and
        // calling a script is where it drops a bare `--` (BUG-MCP); cmd
        // runs claude.cmd.
        std::fs::write(
            bin.join("claude.cmd"),
            format!("@echo off\r\nset CLAUDE_STUB_SHIM=cmd\r\n{python} \"%~dp0stub.py\" %*\r\n"),
        )
        .unwrap();
        std::fs::write(
            bin.join("claude.ps1"),
            format!(
                "$env:CLAUDE_STUB_SHIM = 'ps1'\r\n& {python} \"$PSScriptRoot\\stub.py\" $args\r\nexit $LASTEXITCODE\r\n"
            ),
        )
        .unwrap();
    } else {
        let script = bin.join("claude");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nCLAUDE_STUB_SHIM=sh exec {python} \"$(dirname \"$0\")/stub.py\" \"$@\"\n"
            ),
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
        // PowerShell's own way to take a command verbatim. Bypass: a
        // runner's policy mustn't decide whether the .ps1 shim runs.
        c.args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
        ])
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

/// The installed app and a stub `claude` first on PATH.
struct Desk {
    dir: tempfile::TempDir,
    exe: String,
    path: std::ffi::OsString,
}

fn desk() -> Option<Desk> {
    let Some(python) = python() else {
        eprintln!("skipped: no Python for the stub claude");
        return None;
    };
    let dir = tempfile::tempdir().unwrap();
    let exe = installed_app(dir.path()).to_string_lossy().into_owned();
    let bin = stub_claude(dir.path(), python);
    let path = std::env::join_paths(
        std::iter::once(bin).chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    Some(Desk { dir, exe, path })
}

impl Desk {
    /// `line` pasted into `shell`: what the stub claude got, or `None` if
    /// the shell isn't installed.
    fn paste(&self, shell: &str, line: &str) -> Option<Got> {
        let out = self.dir.path().join(format!("{shell}.json"));
        let _ = std::fs::remove_file(&out);
        let status = match run_in(shell, line)
            .env("PATH", &self.path)
            .env("CLAUDE_STUB_OUT", &out)
            .stdin(Stdio::null())
            .status()
        {
            Ok(s) => s,
            Err(_) => {
                eprintln!("skipped: {shell} isn't installed");
                return None;
            }
        };
        assert!(status.success(), "{shell}: {line}");
        let text = std::fs::read_to_string(&out)
            .unwrap_or_else(|e| panic!("{shell}: the stub claude didn't run ({e}): {line}"));
        Some(serde_json::from_str(&text).unwrap())
    }
}

/// The shim each shell runs: PowerShell picks npm's claude.ps1 over
/// claude.cmd, as it does on a real install.
fn shim_for(shell: &str) -> &'static str {
    match shell {
        "pwsh" | "powershell" => "ps1",
        "cmd" => "cmd",
        _ => "sh",
    }
}

#[test]
fn the_settings_line_survives_each_shell() {
    let Some(d) = desk() else { return };
    let line = agent::claude_code_command(&d.exe);
    let mut ran = 0;
    for shell in shells() {
        let Some(got) = d.paste(shell, &line) else {
            continue;
        };
        assert_eq!(
            got.shim.as_deref(),
            Some(shim_for(shell)),
            "{shell}: which claude ran"
        );
        assert_eq!(
            got.args,
            [
                "mcp".to_string(),
                "add".into(),
                "forever-buddy".into(),
                d.exe.clone(),
                agent::ARG.into(),
            ],
            "{shell}: {line}"
        );
        answers_mcp(&got.args[3..]);
        ran += 1;
    }
    assert!(ran >= 2, "at least two shells ran the line");
}

/// The stubs reproduce BUG-MCP: the line Settings gave before 0.9
/// (`-- "<exe>" --mcp`) loses its `--` through PowerShell's claude.ps1, so
/// claude would read `--mcp` as its own option. Through cmd it survives.
/// If the stubs ever stopped showing this, the test above would prove
/// nothing about PowerShell.
#[cfg(windows)]
#[test]
fn the_old_line_loses_its_dashes_in_powershell() {
    let Some(d) = desk() else { return };
    let old = format!("claude mcp add forever-buddy -- \"{}\" --mcp", d.exe);
    for shell in ["pwsh", "powershell"] {
        let Some(got) = d.paste(shell, &old) else {
            continue;
        };
        assert_eq!(got.shim.as_deref(), Some("ps1"), "{shell}");
        assert!(
            !got.args.iter().any(|a| a == "--"),
            "{shell}: `--` eaten: {:?}",
            got.args
        );
    }
    let got = d.paste("cmd", &old).expect("cmd is always there");
    assert!(
        got.args.iter().any(|a| a == "--"),
        "cmd keeps it: {:?}",
        got.args
    );
}
