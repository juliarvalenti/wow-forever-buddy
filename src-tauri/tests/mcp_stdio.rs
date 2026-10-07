//! BUG-MCP: the built app, started the way an agent client starts it (piped
//! stdin/stdout, one argument), answers an MCP `initialize` and `tools/list`
//! and exits when the client closes stdin. Runs on the Windows and macOS CI
//! runners. Both spellings: `mcp` (what Settings › Agents gives since 0.9)
//! and `--mcp` (configs made before).
//!
//! The test build is a console program; only a release build is a Windows
//! GUI-subsystem one. `MCP_EXE` points the test at another exe, and the
//! release workflow runs it on the release build before it's published.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Sends one line and reads the reply line.
fn ask(arg: &str, stdin: &mut impl Write, out: &mut impl BufRead, line: &str) -> serde_json::Value {
    writeln!(stdin, "{line}").unwrap();
    stdin.flush().unwrap();
    let mut reply = String::new();
    out.read_line(&mut reply).expect("a reply line");
    serde_json::from_str(&reply).unwrap_or_else(|e| panic!("{arg}: not JSON ({e}): {reply:?}"))
}

fn round_trip(arg: &str) {
    let exe = std::env::var("MCP_EXE")
        .unwrap_or_else(|_| env!("CARGO_BIN_EXE_wow-forever-buddy").to_string());
    let mut child = Command::new(exe)
        .arg(arg)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("start the app");
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let init = ask(
        arg,
        &mut stdin,
        &mut out,
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"ci","version":"1"}}}"#,
    );
    assert_eq!(init["id"], 1, "{arg}: {init}");
    assert!(
        init["result"]["serverInfo"]["name"].is_string(),
        "{arg}: {init}"
    );
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#
    )
    .unwrap();
    let tools = ask(
        arg,
        &mut stdin,
        &mut out,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
    );
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("{arg}: {tools}"))
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert!(names.contains(&"list_characters"), "{arg}: {names:?}");

    // The client closing stdin ends the server, with success.
    drop(stdin);
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "{arg}: exited with {status}");
            return;
        }
        if start.elapsed() > Duration::from_secs(20) {
            let _ = child.kill();
            panic!("{arg}: still running after stdin closed");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn the_app_serves_mcp_over_stdio() {
    round_trip("mcp");
    round_trip("--mcp");
}
