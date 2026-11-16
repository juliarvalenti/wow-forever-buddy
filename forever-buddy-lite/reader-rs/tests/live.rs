//! `fbl-rs live` end to end: starts the server on the fixture and talks HTTP to it.

use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Server {
    child: Child,
    port: u16,
    dir: PathBuf,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A price database where Linen Cloth (held by the fixture's characters) is cheap today.
fn auctionator_file(dir: &Path) -> PathBuf {
    // { version = 2, ["2589"] = { m = 5000, h = { [2509] = 10000, [2510] = 10000 }, a = { [2509] = 200, [2510] = 200 } } }
    let mut b = vec![0xa2, 0x67];
    b.extend(b"version");
    b.extend([0x02, 0x64]);
    b.extend(b"2589");
    b.extend([0xa3, 0x61, b'm', 0x19, 0x13, 0x88]);
    b.extend([0x61, b'h', 0xa2, 0x19, 0x09, 0xcd, 0x19, 0x27, 0x10, 0x19, 0x09, 0xce, 0x19, 0x27, 0x10]);
    b.extend([0x61, b'a', 0xa2, 0x19, 0x09, 0xcd, 0x18, 200, 0x19, 0x09, 0xce, 0x18, 200]);
    let escaped: String = b
        .iter()
        .map(|&c| if (32..127).contains(&c) && c != b'"' && c != b'\\' { (c as char).to_string() } else { format!("\\{c:03}") })
        .collect();
    let file = dir.join("Auctionator.lua");
    std::fs::write(&file, format!("AUCTIONATOR_PRICE_DATABASE = {{\n[\"__dbversion\"] = 8,\n[\"Realm\"] = \"{escaped}\",\n}}\n")).unwrap();
    file
}

fn start() -> Server {
    let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let dir = std::env::temp_dir().join(format!("fbl-rs-live-{}-{port}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let child = Command::new(env!("CARGO_BIN_EXE_fbl-rs"))
        .args(["live", "--port", &port.to_string(), "--no-open", "--archive"])
        .arg(dir.join("scans"))
        .env("FBL_FILE", manifest.join("tests/fixture/ForeverBuddyLite.lua"))
        .env("FBL_AUCTIONATOR", auctionator_file(&dir))
        .env("FBL_HIDDEN", dir.join("hidden.json"))
        .env("QUESTIEDB_DIR", dir.join("no-questie"))
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let server = Server { child, port, dir };
    let deadline = Instant::now() + Duration::from_secs(20);
    while TcpStream::connect(("127.0.0.1", port)).is_err() {
        assert!(Instant::now() < deadline, "the server didn't start");
        std::thread::sleep(Duration::from_millis(100));
    }
    server
}

/// (status, content type, body)
fn request(port: u16, method: &str, path: &str, body: &str) -> (u16, String, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(s, "{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n{body}", body.len()).unwrap();
    let mut text = String::new();
    s.read_to_string(&mut text).unwrap();
    let (head, body) = text.split_once("\r\n\r\n").unwrap();
    let status = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    let kind = head.lines().find_map(|l| l.strip_prefix("Content-Type: ")).unwrap_or("").to_string();
    (status, kind, body.to_string())
}

#[test]
fn serves_the_dashboard_its_data_and_hides_items() {
    let server = start();
    let port = server.port;

    let (status, kind, body) = request(port, "GET", "/data.json", "");
    assert_eq!((status, kind.as_str()), (200, "application/json"));
    let data: Value = serde_json::from_str(&body).unwrap();
    let linen = data["items"].as_array().unwrap().iter().find(|i| i["id"] == 2589).expect("Linen Cloth in the catalog");
    assert_eq!(linen["n"], "Linen Cloth");
    assert_eq!(data["opps"]["buy"][0]["id"], 2589);
    assert_eq!(data["stamp"], "2026-11-15");

    let (status, kind, _) = request(port, "GET", "/", "");
    assert_eq!(status, 200);
    assert!(kind.starts_with("text/html"));
    let (status, kind, _) = request(port, "GET", "/lib/api.js", "");
    assert_eq!(status, 200);
    assert!(kind.starts_with("text/javascript"));
    for bad in ["/../reader-rs/Cargo.toml", "/%2e%2e/web/index.html", "/..%5Cx.js", "/missing.js"] {
        assert_eq!(request(port, "GET", bad, "").0, 404, "{bad}");
    }

    // Updates arrive as server-sent events after a rebuild.
    let mut events = TcpStream::connect(("127.0.0.1", port)).unwrap();
    events.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    write!(events, "GET /events HTTP/1.1\r\nHost: localhost\r\n\r\n").unwrap();
    let mut events = BufReader::new(events);
    let mut line = String::new();
    while !line.starts_with(": hello") {
        line.clear();
        events.read_line(&mut line).unwrap();
    }

    assert_eq!(request(port, "POST", "/hide", r#"{"id":"x"}"#).0, 400);
    assert_eq!(request(port, "POST", "/hide", r#"{"id":2589,"name":"Linen Cloth","on":true}"#).0, 200);
    loop {
        line.clear();
        events.read_line(&mut line).unwrap();
        if line.starts_with("event: update") {
            break;
        }
    }
    let data: Value = serde_json::from_str(&request(port, "GET", "/data.json", "").2).unwrap();
    assert_eq!(data["opps"]["hidden"][0]["id"], 2589);
    assert!(data["opps"]["buy"].as_array().unwrap().is_empty());
    assert!(std::fs::read_to_string(server.dir.join("hidden.json")).unwrap().contains("2589"));

    let archived: Vec<_> = std::fs::read_dir(server.dir.join("scans")).unwrap().flatten().collect();
    assert_eq!(archived.len(), 1); // the hide rebuilt, but the scan hadn't changed
    assert!(archived[0].file_name().to_string_lossy().ends_with(".json.gz"));
}
