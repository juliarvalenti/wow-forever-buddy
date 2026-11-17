//! The market watcher (`fbl-rs live`). Leave it running: after every price scan or save that reaches
//! disk (Save prices & reload, /fbl r, any /reload) it archives the scan, prints what's worth doing,
//! and pushes an update to the dashboard.
//!
//! Dashboard: http://localhost:4747 (this machine only). It serves the page in reader/web plus:
//! - `GET /data.json`: `{ items, opps, stamp }` (see `dashboard`);
//! - `GET /events`: server-sent events, "update" after each rebuild;
//! - `POST /hide`: `{ id, name, on }` hides an item from suggestions, or brings it back;
//! - `GET /history.json?id=N`: one item's cheapest price and quantity at every archived scan;
//! - `GET /wealth.json`: gold and item value over time (see `wealth`), recorded in wealth.jsonl next to
//!   the scan archive.
//!
//! Archived scans keep each item's cheapest price and quantity per scan, which Auctionator itself
//! folds into one low/high per day.

use crate::config::Config;
use crate::format::{iso_from_millis, money_short as money};
use crate::sources::{addon, auctionator, hidden, questie, Env};
use crate::{catalog, dashboard, history, report, strategies, wealth};
use std::collections::{BTreeMap, HashSet};
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

/// The game writes several files on /reload: wait this long after the last change before rebuilding.
const SETTLE: Duration = Duration::from_secs(3);
/// How often to look at the files' modification times.
const POLL: Duration = Duration::from_secs(1);
/// The largest /hide request body accepted.
const MAX_BODY: usize = 10_000;

#[derive(Debug, Clone)]
pub struct LiveOptions {
    pub port: u16,
    /// The most to spend on one buy, copper.
    pub budget: i64,
    pub open_browser: bool,
    /// Where scans are archived (default: Documents/FBL Market/scans).
    pub archive_dir: PathBuf,
    /// The dashboard's files (default: reader/web next to this crate).
    pub web_dir: PathBuf,
}

impl LiveOptions {
    pub fn default_archive_dir() -> PathBuf {
        let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_default();
        home.join("Documents").join("FBL Market").join("scans")
    }

    pub fn default_web_dir() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("reader").join("web")
    }
}

/// What changes between rebuilds.
#[derive(Default)]
struct State {
    /// The current `/data.json` and `/wealth.json`.
    data: Arc<String>,
    wealth: Arc<String>,
    last_archive: Option<u64>,
    seen_buys: HashSet<u32>,
    seen_gaps: HashSet<String>,
}

pub struct Live {
    env: Env,
    cfg: Config,
    opts: LiveOptions,
    /// QuestieDB is big and doesn't change while the game runs: read once.
    questie: questie::QuestieItems,
    state: Mutex<State>,
    listeners: Mutex<Vec<TcpStream>>,
    /// Every archived scan, by item, for the intraday charts.
    history: Mutex<history::ScanHistory>,
}

impl Live {
    pub fn new(env: Env, cfg: Config, opts: LiveOptions) -> Live {
        let questie = questie::load_items(&env.questiedb_dir(), cfg.vendor_cutoff);
        let history = Mutex::new(history::ScanHistory::load(&opts.archive_dir));
        Live { env, cfg, opts, questie, state: Mutex::default(), listeners: Mutex::default(), history }
    }

    /// The files to watch: the addon's, and Auctionator's next to each.
    pub fn watched_files(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for f in self.env.addon_files() {
            let beside = f.parent().map(|d| d.join("Auctionator.lua"));
            for file in std::iter::once(f).chain(beside) {
                if file.exists() && !files.contains(&file) {
                    files.push(file);
                }
            }
        }
        files
    }

    /// Reads everything again, archives the scan, prints the news and the report, and tells the
    /// dashboards.
    pub fn rebuild(&self, reason: &str) -> Result<(), String> {
        let started = Instant::now();
        let addon = addon::load(&self.env)?;
        let prices = auctionator::load(&self.env)?;
        let catalog = catalog::build(&addon, &prices, &self.questie);
        let now = crate::format::now_seconds();
        let archived = self.archive_scan(&prices)?;
        let wealth = self.record_wealth(&addon, &prices, now);
        let inputs = crate::sources::market_inputs_from(&addon, prices, &self.questie, hidden::load(&self.env.hidden_file()), now);
        let analysis = strategies::analyze(&inputs, self.opts.budget, &self.cfg);
        let data = dashboard::data_json(&catalog, &analysis).to_string();

        let archived_note =
            archived.map_or(String::new(), |f| format!(" · archived {}", f.file_name().unwrap_or_default().to_string_lossy()));
        println!(
            "\n===== {} · {reason} · {} items in {} ms{archived_note}",
            clock(now),
            catalog.items.len(),
            started.elapsed().as_millis()
        );
        let news = {
            let mut state = self.state.lock().unwrap();
            state.data = Arc::new(data);
            state.wealth = Arc::new(wealth);
            news(&mut state, &analysis)
        };
        for line in news {
            println!("{line}");
        }
        println!("{}", report::report(&analysis, 8));
        self.broadcast();
        Ok(())
    }

    /// Every item's cheapest price and quantity in this scan, gzipped, one file per scan. Skipped
    /// when nothing changed since the last one.
    fn archive_scan(&self, prices: &auctionator::PriceDb) -> Result<Option<PathBuf>, String> {
        let mut snapshot: BTreeMap<u32, (i64, i64)> = BTreeMap::new();
        for entry in prices.realms.iter().flat_map(|r| r.items.iter()) {
            let (Some(id), Some(now)) = (entry.id(), entry.now.filter(|&m| m != 0.0)) else { continue };
            let listed = entry.days.last().and_then(|d| d.listed).unwrap_or(0.0);
            snapshot.insert(id, (now as i64, listed as i64));
        }
        let items: serde_json::Map<String, serde_json::Value> =
            snapshot.iter().map(|(id, (now, listed))| (id.to_string(), serde_json::json!([now, listed]))).collect();
        let items_json = serde_json::Value::Object(items).to_string();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        items_json.hash(&mut hasher);
        let hash = hasher.finish();
        let mut state = self.state.lock().unwrap();
        if state.last_archive == Some(hash) {
            return Ok(None);
        }
        state.last_archive = Some(hash);
        drop(state);

        let at = iso_from_millis(SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0));
        let dir = &self.opts.archive_dir;
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let file = dir.join(format!("{}.json.gz", at[..19].replace([':', 'T'], "-")));
        let body = format!("{{\"at\":\"{at}\",\"items\":{items_json}}}");
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(body.as_bytes())
            .and_then(|_| gz.finish())
            .and_then(|bytes| std::fs::write(&file, bytes))
            .map_err(|e| e.to_string())?;
        let at_seconds = crate::format::seconds_from_iso(&at).unwrap_or_else(crate::format::now_seconds);
        self.history.lock().unwrap().add(at_seconds, snapshot.iter().map(|(id, (now, listed))| (*id, *now, *listed)));
        Ok(Some(file))
    }

    /// Where the wealth history lives: wealth.jsonl next to the scans folder.
    fn wealth_file(&self) -> PathBuf {
        self.opts.archive_dir.parent().unwrap_or(&self.opts.archive_dir).join("wealth.jsonl")
    }

    /// Records every character's gold and item value (when changed) and returns `/wealth.json`.
    fn record_wealth(&self, addon: &addon::AddonData, prices: &auctionator::PriceDb, now: f64) -> String {
        let chars: Vec<addon::Character> = addon.characters.values().cloned().collect();
        let snapshot = wealth::Snapshot::of(now, &wealth::characters_now(&chars, prices));
        if let Err(e) = wealth::record(&self.wealth_file(), &snapshot) {
            eprintln!("wealth history: {e}");
        }
        wealth::wealth_json(&chars, prices, &self.wealth_file()).to_string()
    }

    fn broadcast(&self) {
        let stamp = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        let message = format!("event: update\ndata: {stamp}\n\n");
        // A dashboard that went away fails the write and is dropped.
        self.listeners.lock().unwrap().retain_mut(|s| s.write_all(message.as_bytes()).and_then(|_| s.flush()).is_ok());
    }

    /// Serves the dashboard and watches the files until the process is stopped.
    pub fn run(self: Arc<Self>) -> Result<(), String> {
        self.rebuild("started")?;
        let listener = TcpListener::bind(("127.0.0.1", self.opts.port)).map_err(|e| format!("port {}: {e}", self.opts.port))?;
        let url = format!("http://localhost:{}", self.opts.port);
        let watched = self.watched_files();
        let names: Vec<String> = watched.iter().map(|f| f.file_name().unwrap_or_default().to_string_lossy().into_owned()).collect();
        println!("\nDashboard: {url}  ·  watching {}  ·  scans archived to {}", names.join(", "), self.opts.archive_dir.display());
        println!("Leave this running. After each /fbl market save (or any /reload), it updates. Ctrl+C to stop.");
        if self.opts.open_browser {
            open_in_browser(&url);
        }
        // "localhost" tries IPv6 (::1) first: listen there too, or every request waits for that
        // attempt to fail before falling back to 127.0.0.1. Still this machine only.
        let mut listeners = vec![listener];
        if let Ok(v6) = TcpListener::bind(("::1", self.opts.port)) {
            listeners.push(v6);
        }
        for listener in listeners {
            let server = Arc::clone(&self);
            std::thread::spawn(move || {
                for stream in listener.incoming().flatten() {
                    let live = Arc::clone(&server);
                    std::thread::spawn(move || live.handle(stream));
                }
            });
        }
        self.watch(&watched);
        Ok(())
    }

    /// Polls the files' modification times; rebuilds once they've been quiet for `SETTLE`.
    fn watch(&self, files: &[PathBuf]) {
        let modified = |f: &PathBuf| std::fs::metadata(f).and_then(|m| m.modified()).ok();
        let mut last: Vec<Option<SystemTime>> = files.iter().map(modified).collect();
        let mut pending: Option<(Instant, String)> = None;
        loop {
            std::thread::sleep(POLL);
            for (i, f) in files.iter().enumerate() {
                let m = modified(f);
                if m != last[i] {
                    last[i] = m;
                    let name = f.file_name().unwrap_or_default().to_string_lossy().into_owned();
                    pending = Some((Instant::now() + SETTLE, format!("{name} changed")));
                }
            }
            if let Some((due, reason)) = &pending {
                if Instant::now() >= *due {
                    if let Err(e) = self.rebuild(reason) {
                        eprintln!("rebuild failed: {e}");
                    }
                    pending = None;
                }
            }
        }
    }

    fn handle(&self, mut stream: TcpStream) {
        let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
        let Some(request) = read_request(&mut stream) else { return };
        let path = request.path.split('?').next().unwrap_or("/").to_string();
        let reply = match (request.method.as_str(), path.as_str()) {
            (_, "/events") => {
                let head = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\n\r\n: hello\n\n";
                if stream.write_all(head.as_bytes()).is_ok() {
                    self.listeners.lock().unwrap().push(stream);
                }
                return;
            }
            ("POST", "/hide") => match self.hide(&request.body) {
                Ok(()) => Reply::new(200, "application/json", b"{}".to_vec()),
                Err(e) => Reply::new(400, "text/plain; charset=utf-8", e.into_bytes()),
            },
            (_, "/history.json") => {
                let id = request
                    .path
                    .split_once('?')
                    .and_then(|(_, q)| q.split('&').find_map(|kv| kv.strip_prefix("id=")))
                    .and_then(|v| v.parse().ok());
                match id {
                    Some(id) => {
                        let body = self.history.lock().unwrap().item_json(id).to_string();
                        Reply::new(200, "application/json", body.into_bytes()).cache("no-store")
                    }
                    None => Reply::new(400, "text/plain; charset=utf-8", b"history.json wants ?id=<item id>".to_vec()),
                }
            }
            (_, "/wealth.json") => {
                let wealth = Arc::clone(&self.state.lock().unwrap().wealth);
                Reply::new(200, "application/json", wealth.as_bytes().to_vec()).cache("no-store")
            }
            (_, "/data.json") => {
                let data = Arc::clone(&self.state.lock().unwrap().data);
                Reply::new(200, "application/json", data.as_bytes().to_vec()).cache("no-store")
            }
            _ => match web_file(&self.opts.web_dir, &path) {
                Some((file, kind)) => match std::fs::read(&file) {
                    Ok(bytes) => Reply::new(200, kind, bytes).cache("no-cache"),
                    Err(_) => Reply::not_found(),
                },
                None => Reply::not_found(),
            },
        };
        let _ = reply.send(&mut stream);
    }

    /// `{ id, name, on }`: hide (on) or show an item, then rebuild.
    fn hide(&self, body: &[u8]) -> Result<(), String> {
        let v: serde_json::Value = serde_json::from_slice(body).map_err(|e| e.to_string())?;
        let id = v.get("id").and_then(serde_json::Value::as_u64).and_then(|i| u32::try_from(i).ok()).ok_or("bad id")?;
        let name = v.get("name").and_then(serde_json::Value::as_str).map(|n| n.chars().take(100).collect::<String>());
        let on = v.get("on").is_some_and(crate::json::truthy);
        let now = iso_from_millis(crate::format::now_seconds() * 1000.0);
        hidden::set(&self.env.hidden_file(), id, name.as_deref(), on, &now).map_err(|e| e.to_string())?;
        self.rebuild(&format!("{} {}", if on { "hid" } else { "unhid" }, name.unwrap_or_else(|| id.to_string())))
    }
}

/// What's new since the last rebuild: gaps not seen before (per ladder search) and new buys.
fn news(state: &mut State, a: &strategies::Analysis) -> Vec<String> {
    let gap_key = |g: &strategies::GapPick| format!("{}@{}", g.key, g.at);
    let new_gaps: Vec<String> = a
        .gaps
        .iter()
        .filter(|g| !state.seen_gaps.contains(&gap_key(g)))
        .take(6)
        .map(|g| {
            format!(
                "{}: buy {} up to {}, relist {} (+{})",
                g.name,
                g.gap.qty,
                money(g.gap.up_to as f64),
                money(g.gap.relist as f64),
                money(g.gap.profit as f64)
            )
        })
        .collect();
    let new_buys: Vec<String> = a
        .buy
        .iter()
        .filter(|p| !state.seen_buys.contains(&p.item.id))
        .take(6)
        .map(|p| {
            format!(
                "{} at {} (usual {}, +{} each)",
                p.item.name,
                money(p.item.now as f64),
                money(p.item.usual as f64),
                money(p.profit as f64)
            )
        })
        .collect();
    state.seen_gaps = a.gaps.iter().map(gap_key).collect();
    state.seen_buys = a.buy.iter().map(|p| p.item.id).collect();
    let mut lines = Vec::new();
    if !new_gaps.is_empty() {
        lines.push(format!("NEW GAPS: {}", new_gaps.join("; ")));
    }
    if !new_buys.is_empty() {
        lines.push(format!("NEW BUYS: {}", new_buys.join("; ")));
    }
    lines
}

/// "HH:MM:SS UTC" (the standard library has no time zones).
fn clock(now: f64) -> String {
    format!("{} UTC", &iso_from_millis(now * 1000.0)[11..19])
}

fn open_in_browser(url: &str) {
    let opener = if cfg!(windows) {
        "explorer.exe"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let _ = std::process::Command::new(opener).arg(url).spawn();
}

// ---------------------------------------------------------------- a minimal HTTP/1.1

struct Request {
    method: String,
    path: String,
    body: Vec<u8>,
}

/// Reads the request line, the headers and (up to `MAX_BODY`) the body.
fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    let mut parts = line.split_whitespace();
    let (method, path) = (parts.next()?.to_string(), parts.next()?.to_string());
    let mut length = 0;
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).ok()? == 0 || header.trim().is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().ok()?;
            }
        }
    }
    if length > MAX_BODY {
        return None;
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    Some(Request { method, path, body })
}

struct Reply {
    status: u16,
    content_type: &'static str,
    cache: Option<&'static str>,
    body: Vec<u8>,
}

impl Reply {
    fn new(status: u16, content_type: &'static str, body: Vec<u8>) -> Reply {
        Reply { status, content_type, cache: None, body }
    }

    fn not_found() -> Reply {
        Reply::new(404, "text/plain; charset=utf-8", b"not found".to_vec())
    }

    fn cache(mut self, policy: &'static str) -> Reply {
        self.cache = Some(policy);
        self
    }

    fn send(&self, stream: &mut TcpStream) -> std::io::Result<()> {
        let reason = match self.status {
            200 => "OK",
            400 => "Bad Request",
            _ => "Not Found",
        };
        let cache = self.cache.map_or(String::new(), |c| format!("Cache-Control: {c}\r\n"));
        let head = format!(
            "HTTP/1.1 {} {reason}\r\nContent-Type: {}\r\nContent-Length: {}\r\n{cache}Connection: close\r\n\r\n",
            self.status,
            self.content_type,
            self.body.len()
        );
        stream.write_all(head.as_bytes())?;
        stream.write_all(&self.body)?;
        stream.flush()
    }
}

fn content_type(path: &Path) -> Option<&'static str> {
    Some(match path.extension()?.to_str()? {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        _ => return None,
    })
}

/// `%41` -> `A`; None for a malformed escape or a result that isn't UTF-8.
fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// A file under `web_dir` for a URL path, with its content type. Never anything outside web_dir:
/// only plain path components, and only the file types the page uses.
fn web_file(web_dir: &Path, url_path: &str) -> Option<(PathBuf, &'static str)> {
    let decoded = percent_decode(url_path)?;
    let relative = decoded.trim_start_matches('/');
    let relative = if relative.is_empty() { "index.html" } else { relative };
    if relative.contains('\\') || relative.contains(':') {
        return None;
    }
    let rel = Path::new(relative);
    if !rel.components().all(|c| matches!(c, Component::Normal(_))) {
        return None;
    }
    let file = web_dir.join(rel);
    let kind = content_type(&file)?;
    file.is_file().then_some((file, kind))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn web_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fbl-rs-web-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("lib")).unwrap();
        std::fs::write(dir.join("index.html"), "<p>hi</p>").unwrap();
        std::fs::write(dir.join("lib").join("app.js"), "1").unwrap();
        std::fs::write(dir.join("notes.txt"), "no").unwrap();
        dir
    }

    #[test]
    fn serves_only_web_files() {
        let dir = web_dir();
        assert_eq!(web_file(&dir, "/").unwrap().1, "text/html; charset=utf-8");
        assert_eq!(web_file(&dir, "/lib/app.js").unwrap().1, "text/javascript; charset=utf-8");
        assert_eq!(web_file(&dir, "/lib%2Fapp.js").unwrap().1, "text/javascript; charset=utf-8");
        assert!(web_file(&dir, "/notes.txt").is_none()); // not a page type
        assert!(web_file(&dir, "/../Cargo.toml").is_none());
        assert!(web_file(&dir, "/lib/../../x.js").is_none());
        assert!(web_file(&dir, "/%2e%2e/x.js").is_none());
        assert!(web_file(&dir, "/..%5cx.js").is_none());
        assert!(web_file(&dir, "/C:/Windows/x.js").is_none());
        assert!(web_file(&dir, "/missing.js").is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn news_reports_only_what_is_new() {
        let mut state = State::default();
        let mut a = strategies::Analysis {
            date: String::new(),
            items: 0,
            busy: 0,
            budget: 0,
            min_listed: 0,
            skipped_vendor: 0,
            hidden: vec![],
            scan_at: None,
            buy: vec![],
            dips: vec![],
            sell: vec![],
            squeeze: vec![],
            flood: vec![],
            swingy: vec![],
            gaps: vec![],
            check: vec![],
            ladders: BTreeMap::new(),
        };
        let item = strategies::ItemMarket {
            id: 1,
            name: "Linen Cloth".into(),
            now: 50,
            usual: 100,
            days: 3,
            low: 50,
            high: 100,
            qty: 200,
            first_qty: 200,
            first_low: 100,
            held: 0,
        };
        a.buy.push(strategies::Pick { item, profit: 300 });
        assert_eq!(news(&mut state, &a), ["NEW BUYS: Linen Cloth at 50c (usual 1s 00c, +3s 00c each)"]);
        assert!(news(&mut state, &a).is_empty());
    }
}
