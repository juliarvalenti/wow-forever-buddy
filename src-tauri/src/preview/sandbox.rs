//! The preview's Lua (PV1): a Lua 5.1 state that runs the bundled WOWAPI
//! emulator and draws the logged-in scene, and can't touch anything else.
//!
//! - **No io, no shell.** The state gets only the table, string and math
//!   libraries and a cut-down `os` (time and dates). WOWAPI reaches files
//!   through `WOWAPI_HOST` (`tools/wowapi/wowapi/host.lua`), which is
//!   [`Host`]: a small virtual tree where `/tools` is the bundled code,
//!   `/tools/wowapi/wowapi/data` the downloaded WOWAPI data, `/addon` the
//!   bundled addon with the slots the app would send the game, `/scratch`
//!   the one folder it may write, and `/extract` the player's extracted game
//!   files, there only while the art switch is on.
//! - **Text only.** `loadstring` and `require` refuse bytecode; `load`,
//!   `loadfile`, `dofile` and `string.dump` are gone.
//! - **No debug library.** WOWAPI asks which file is on the stack (its taint
//!   check, its module folders); `debug.getinfo` answers only that, and
//!   `debug.traceback` gives the error's place.
//! - **Bounded.** A memory limit, and a time limit per draw.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use mlua::chunk::ChunkMode;
use mlua::{HookTriggers, Lua, LuaOptions, LuaString, StdLib, Table, Value, VmState};

use super::bundle;

/// The Lua state's memory limit. A full-art screen is tens of MB of SVG,
/// built from strings several times over.
const MEMORY_LIMIT: usize = 1536 * 1024 * 1024;

/// What the preview's Lua can see. Paths are `/`-separated from a root of
/// `/`; `..` is resolved before anything is looked up, and can't climb out.
pub struct Host {
    /// WOWAPI's `data/` (downloaded, checked against the app's hashes).
    pub data: Option<PathBuf>,
    /// The one folder the Lua may write.
    pub scratch: PathBuf,
    /// The player's extracted game files: only while the art switch is on.
    pub extract: Option<PathBuf>,
    /// Addon files that replace the bundled ones ("Data/Settings.lua"): the
    /// slots the app would send the game.
    pub slots: BTreeMap<String, Vec<u8>>,
}

const ADDON: &str = "addon/ForeverBuddy";
const DATA: &str = "tools/wowapi/wowapi/data";

/// Where a path lands.
enum At {
    Tool(String),
    Addon(String),
    Disk(PathBuf),
    /// A folder above the roots ("/", "/tools", "/addon").
    Virtual(String),
}

/// `/a/./b/../c` as `["a", "c"]`; `None` when it isn't absolute, climbs out
/// of the root, or has a part a disk path could misread (`C:`).
fn components(path: &str) -> Option<Vec<String>> {
    if !(path.starts_with('/') || path.starts_with('\\')) {
        return None;
    }
    let mut out: Vec<String> = Vec::new();
    for part in path.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                out.pop()?;
            }
            p if p.contains(':') || p.contains('\0') => return None,
            p => out.push(p.to_string()),
        }
    }
    Some(out)
}

impl Host {
    fn at(&self, path: &str) -> Option<At> {
        let parts = components(path)?;
        let joined = parts.join("/");
        let under = |root: &str| -> Option<Vec<String>> {
            let root: Vec<&str> = root.split('/').collect();
            (parts.len() >= root.len() && parts.iter().zip(&root).all(|(a, b)| a == b))
                .then(|| parts[root.len()..].to_vec())
        };
        let disk = |base: &Path, rest: Vec<String>| {
            let mut p = base.to_path_buf();
            p.extend(rest);
            At::Disk(p)
        };
        if let Some(rest) = under(DATA) {
            return Some(disk(self.data.as_ref()?, rest));
        }
        if let Some(rest) = under("scratch") {
            return Some(disk(&self.scratch, rest));
        }
        if let Some(rest) = under("extract") {
            return Some(disk(self.extract.as_ref()?, rest));
        }
        if let Some(rest) = under(ADDON) {
            return Some(At::Addon(rest.join("/")));
        }
        if under("tools").is_some_and(|r| !r.is_empty()) {
            return Some(At::Tool(joined));
        }
        Some(At::Virtual(joined))
    }

    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        match self.at(path)? {
            At::Tool(p) => bundle::tool(&p).map(<[u8]>::to_vec),
            At::Addon(p) => self.slots.get(&p).cloned().or_else(|| {
                crate::addon::bundled_files()
                    .iter()
                    .find(|(f, _)| *f == p)
                    .map(|(_, b)| b.to_vec())
            }),
            At::Disk(p) => std::fs::read(p).ok(),
            At::Virtual(_) => None,
        }
    }

    /// The names in a folder (files and folders), or `None` if it isn't one.
    pub fn list(&self, path: &str) -> Option<Vec<String>> {
        let mut names = std::collections::BTreeSet::new();
        let children = |names: &mut std::collections::BTreeSet<String>,
                        keys: &mut dyn Iterator<Item = &str>,
                        dir: &str| {
            let prefix = if dir.is_empty() {
                String::new()
            } else {
                format!("{dir}/")
            };
            let mut any = false;
            for key in keys {
                if let Some(rest) = key.strip_prefix(&prefix) {
                    any = true;
                    if let Some(first) = rest.split('/').next() {
                        names.insert(first.to_string());
                    }
                }
            }
            any
        };
        match self.at(path)? {
            At::Disk(p) => {
                let rd = std::fs::read_dir(p).ok()?;
                return Some(
                    rd.filter_map(|e| e.ok())
                        .map(|e| e.file_name().to_string_lossy().into_owned())
                        .collect(),
                );
            }
            At::Addon(dir) => {
                let mut keys = crate::addon::bundled_files()
                    .iter()
                    .map(|(f, _)| *f)
                    .chain(self.slots.keys().map(String::as_str));
                if !children(&mut names, &mut keys, &dir) {
                    return None;
                }
            }
            At::Tool(dir) | At::Virtual(dir) => {
                let mut keys = bundle::TOOLS
                    .iter()
                    .map(|(p, _)| *p)
                    .chain(std::iter::once(ADDON))
                    .chain(["scratch"])
                    .chain(self.extract.as_ref().map(|_| "extract"))
                    .chain(self.data.as_ref().map(|_| DATA));
                if !children(&mut names, &mut keys, &dir) {
                    return None;
                }
            }
        }
        Some(names.into_iter().collect())
    }

    pub fn exists(&self, path: &str) -> bool {
        match self.at(path) {
            Some(At::Disk(p)) => p.exists(),
            Some(_) => self.read(path).is_some() || self.list(path).is_some(),
            None => false,
        }
    }

    /// Every file under `dir` with one of `exts` (lowercase, no dot), as
    /// `./sub/file.ext`.
    pub fn walk(&self, dir: &str, exts: &[String]) -> Vec<String> {
        let wanted = |name: &str| {
            exts.is_empty()
                || name
                    .rsplit_once('.')
                    .is_some_and(|(_, e)| exts.iter().any(|x| x.eq_ignore_ascii_case(e)))
        };
        match self.at(dir) {
            Some(At::Disk(root)) => walkdir::WalkDir::new(&root)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
                .filter_map(|e| {
                    let rel = e
                        .path()
                        .strip_prefix(&root)
                        .ok()?
                        .to_string_lossy()
                        .replace('\\', "/");
                    wanted(&rel).then(|| format!("./{rel}"))
                })
                .collect(),
            Some(At::Tool(d)) | Some(At::Virtual(d)) => {
                let prefix = if d.is_empty() {
                    String::new()
                } else {
                    format!("{d}/")
                };
                bundle::TOOLS
                    .iter()
                    .filter_map(|(p, _)| p.strip_prefix(&prefix))
                    .filter(|rel| wanted(rel))
                    .map(|rel| format!("./{rel}"))
                    .collect()
            }
            Some(At::Addon(d)) => {
                let prefix = if d.is_empty() {
                    String::new()
                } else {
                    format!("{d}/")
                };
                crate::addon::bundled_files()
                    .iter()
                    .map(|(f, _)| *f)
                    .filter_map(|f| f.strip_prefix(&prefix))
                    .filter(|rel| wanted(rel))
                    .map(|rel| format!("./{rel}"))
                    .collect()
            }
            None => Vec::new(),
        }
    }

    /// Writes go to `/scratch` only.
    fn scratch_path(&self, path: &str) -> Option<PathBuf> {
        let parts = components(path)?;
        (parts.first().map(String::as_str) == Some("scratch")).then(|| {
            let mut p = self.scratch.clone();
            p.extend(&parts[1..]);
            p
        })
    }

    pub fn write(&self, path: &str, bytes: &[u8]) -> Result<(), String> {
        let p = self
            .scratch_path(path)
            .ok_or_else(|| format!("{path}: only /scratch is writable"))?;
        std::fs::write(p, bytes).map_err(|e| format!("{path}: {e}"))
    }

    pub fn mkdir(&self, path: &str) -> bool {
        self.scratch_path(path)
            .is_some_and(|p| std::fs::create_dir_all(p).is_ok())
    }
}

/// What to draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrawOptions {
    /// A class `tools/addon-shots/fixtures.lua` has ("rogue"), or `None` for
    /// the generic character.
    pub class: Option<String>,
    /// Blizzard's UI and art from `/extract` (the art switch).
    pub art: bool,
    /// The moment to draw (`preview.lua`'s MOMENTS): "standing", "combat",
    /// "stealth" or "low_health".
    pub moment: &'static str,
    /// A UI document to check and draw over the scene (UI1's render_ui),
    /// already within `ui::limits`. It reaches the Lua as a table built here
    /// from the JSON, never as source.
    pub ui: Option<serde_json::Value>,
}

/// A drawn frame.
#[derive(Debug, Clone)]
pub struct Drawn {
    /// Empty when the document had problems (nothing is drawn then).
    pub svg: String,
    /// The Lua errors the scene logged (WOWAPI's gaps, or our addon's).
    pub lua_errors: u32,
    /// The fixture character drawn ("rogue"), or `None` for the generic one.
    pub class: Option<String>,
    /// What the document's check (the addon's `UICheck.lua`) found wrong.
    pub problems: Vec<Problem>,
}

/// One thing wrong with a UI document: where (a JSON pointer into it, as
/// the agent wrote it), a code, and how to fix it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Problem {
    pub path: String,
    pub code: String,
    pub message: String,
}

/// A JSON value as Lua data: objects as tables with their own keys, lists
/// as sequences. `ui::limits` has refused what doesn't fit (nulls, size,
/// depth) before this runs; the depth guard here only keeps it finite.
fn json_to_lua(lua: &Lua, v: &serde_json::Value, depth: usize) -> mlua::Result<Value> {
    use serde_json::Value as J;
    if depth > 64 {
        return Err(mlua::Error::runtime("the document is nested too deep"));
    }
    Ok(match v {
        J::Null => Value::Nil,
        J::Bool(b) => Value::Boolean(*b),
        J::Number(n) => match n.as_i64() {
            Some(i) => Value::Integer(i),
            None => Value::Number(n.as_f64().unwrap_or(0.0)),
        },
        J::String(s) => Value::String(lua.create_string(s)?),
        J::Array(items) => {
            let t = lua.create_table_with_capacity(items.len(), 0)?;
            for (i, item) in items.iter().enumerate() {
                t.raw_set(i + 1, json_to_lua(lua, item, depth + 1)?)?;
            }
            Value::Table(t)
        }
        J::Object(map) => {
            let t = lua.create_table_with_capacity(0, map.len())?;
            for (k, item) in map {
                t.raw_set(k.as_str(), json_to_lua(lua, item, depth + 1)?)?;
            }
            Value::Table(t)
        }
    })
}

/// One Lua state, kept between draws (the modules stay loaded).
pub struct Session {
    lua: Lua,
    deadline: Rc<Cell<Option<Instant>>>,
}

/// Builds the sandbox's globals: the text-only loader, `require` over the
/// bundle, the debug shim and the host functions.
const BOOT: &str = r#"
local host, loadText = WOWAPI_HOST, loadstring
package = { loaded = {}, config = "/\n;\n?\n!\n-" } -- the tree's separator is "/"
local patterns = { "/tools/wowapi/?.lua", "/tools/wowapi/?/init.lua", "/tools/addon-shots/?.lua" }
function require(name)
  local done = package.loaded[name]
  if done ~= nil then return done end
  local path = name:gsub("%.", "/")
  for _, pattern in ipairs(patterns) do
    local file = pattern:gsub("%?", path)
    local src = host.read(file)
    if src then
      local fn = assert(loadText(src, "@" .. file))
      local result = fn(name)
      if package.loaded[name] == nil then package.loaded[name] = result == nil and true or result end
      return package.loaded[name]
    end
  end
  error("module '" .. name .. "' not found", 2)
end
"#;

impl Session {
    pub fn new(host: Arc<RwLock<Host>>) -> mlua::Result<Session> {
        let lua = Lua::new_with(
            StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::OS,
            LuaOptions::default(),
        )?;
        lua.set_memory_limit(MEMORY_LIMIT)?;
        let g = lua.globals();

        // os: the clock and dates only.
        let os: Table = g.get("os")?;
        let safe = lua.create_table()?;
        for name in ["time", "clock", "date", "difftime"] {
            safe.set(name, os.get::<Value>(name)?)?;
        }
        g.set("os", safe)?;
        for name in ["dofile", "loadfile", "load", "print"] {
            g.set(name, Value::Nil)?;
        }
        let string: Table = g.get("string")?;
        string.set("dump", Value::Nil)?;

        // loadstring: text only.
        g.set(
            "loadstring",
            lua.create_function(|lua, (src, name): (LuaString, Option<String>)| {
                let bytes = src.as_bytes();
                if bytes.first() == Some(&27) {
                    return Ok((Value::Nil, Some("bytecode isn't loaded".to_string())));
                }
                let chunk = lua
                    .load(&bytes[..])
                    .set_name(name.unwrap_or_else(|| "=(loadstring)".into()))
                    .set_mode(ChunkMode::Text);
                match chunk.into_function() {
                    Ok(f) => Ok((Value::Function(f), None)),
                    Err(e) => Ok((Value::Nil, Some(e.to_string()))),
                }
            })?,
        )?;

        // debug: which file is on the stack, and a traceback.
        let debug = lua.create_table()?;
        debug.set(
            "getinfo",
            lua.create_function(|lua, (level, _what): (Value, Option<String>)| {
                let level = match level {
                    Value::Integer(i) if i >= 0 => i as usize,
                    Value::Number(n) if n >= 0.0 => n as usize,
                    _ => return Ok(Value::Nil),
                };
                let found = lua.inspect_stack(level, |d| {
                    let s = d.source();
                    (
                        s.source.map(|c| c.into_owned()),
                        s.short_src.map(|c| c.into_owned()),
                        d.current_line(),
                        s.what,
                    )
                });
                let Some((source, short_src, line, what)) = found else {
                    return Ok(Value::Nil);
                };
                let t = lua.create_table()?;
                t.set("source", source)?;
                t.set("short_src", short_src)?;
                t.set("currentline", line.map(|l| l as i64).unwrap_or(-1))?;
                t.set("what", what)?;
                Ok(Value::Table(t))
            })?,
        )?;
        debug.set(
            "traceback",
            lua.create_function(|lua, (msg, level): (Option<Value>, Option<i64>)| {
                let mut out = match msg {
                    Some(Value::String(s)) => s.to_string_lossy(),
                    Some(Value::Nil) | None => String::new(),
                    Some(other) => return Ok(other),
                };
                out.push_str("\nstack traceback:");
                let mut level = level.unwrap_or(1).max(1) as usize;
                while let Some(line) = lua.inspect_stack(level, |d| {
                    let s = d.source();
                    format!(
                        "\n\t{}:{}:",
                        s.short_src.unwrap_or_default(),
                        d.current_line().map(|l| l as i64).unwrap_or(-1)
                    )
                }) {
                    out.push_str(&line);
                    level += 1;
                    if level > 40 {
                        break;
                    }
                }
                Ok(Value::String(lua.create_string(&out)?))
            })?,
        )?;
        g.set("debug", debug)?;

        // WOWAPI_HOST: the tree in `Host`.
        let hostt = lua.create_table()?;
        let h = host.clone();
        hostt.set(
            "read",
            lua.create_function(move |lua, path: String| {
                match h.read().expect("preview host lock").read(&path) {
                    Some(bytes) => Ok((Value::String(lua.create_string(bytes)?), None)),
                    None => Ok((
                        Value::Nil,
                        Some(format!("{path}: No such file or directory")),
                    )),
                }
            })?,
        )?;
        let h = host.clone();
        hostt.set(
            "list",
            lua.create_function(move |_, path: String| {
                Ok(h.read().expect("preview host lock").list(&path))
            })?,
        )?;
        let h = host.clone();
        hostt.set(
            "walk",
            lua.create_function(move |_, (path, exts): (String, Option<Table>)| {
                let mut wanted = Vec::new();
                if let Some(exts) = exts {
                    for pair in exts.pairs::<String, Value>() {
                        let (ext, on) = pair?;
                        if !matches!(on, Value::Nil | Value::Boolean(false)) {
                            wanted.push(ext);
                        }
                    }
                }
                Ok(h.read().expect("preview host lock").walk(&path, &wanted))
            })?,
        )?;
        let h = host.clone();
        hostt.set(
            "exists",
            lua.create_function(move |_, path: String| {
                Ok(h.read().expect("preview host lock").exists(&path))
            })?,
        )?;
        let h = host.clone();
        hostt.set(
            "write",
            lua.create_function(move |_, (path, text): (String, LuaString)| {
                match h
                    .read()
                    .expect("preview host lock")
                    .write(&path, &text.as_bytes())
                {
                    Ok(()) => Ok((Some(true), None)),
                    Err(e) => Ok((None, Some(e))),
                }
            })?,
        )?;
        let h = host;
        hostt.set(
            "mkdir",
            lua.create_function(move |_, path: String| {
                Ok(h.read().expect("preview host lock").mkdir(&path))
            })?,
        )?;
        hostt.set("cwd", lua.create_function(|_, ()| Ok("/scratch"))?)?;
        g.set("WOWAPI_HOST", hostt)?;

        lua.load(BOOT)
            .set_name("=preview-boot")
            .set_mode(ChunkMode::Text)
            .exec()?;

        // A draw that runs past its time is stopped.
        let deadline: Rc<Cell<Option<Instant>>> = Rc::default();
        let d = deadline.clone();
        lua.set_hook(
            HookTriggers::new().every_nth_instruction(200_000),
            move |_, _| {
                if d.get().is_some_and(|t| Instant::now() > t) {
                    return Err(mlua::Error::runtime("the preview took too long to draw"));
                }
                Ok(VmState::Continue)
            },
        )?;

        Ok(Session { lua, deadline })
    }

    /// Boots a fresh scene with `opts` and draws it, within `limit`.
    pub fn draw(&self, opts: &DrawOptions, limit: Duration) -> Result<Drawn, String> {
        self.deadline.set(Some(Instant::now() + limit));
        let result = (|| -> mlua::Result<Drawn> {
            let preview: Table = self
                .lua
                .load("return require('preview')")
                .set_mode(ChunkMode::Text)
                .eval()?;
            let draw: mlua::Function = preview.get("draw")?;
            let o = self.lua.create_table()?;
            o.set("addon", format!("/{ADDON}"))?;
            o.set("class", opts.class.clone())?;
            o.set("art", opts.art)?;
            o.set("moment", opts.moment)?;
            if opts.art {
                o.set("blizzard", "/extract/interface/addons")?;
            }
            if let Some(doc) = &opts.ui {
                o.set("ui", json_to_lua(&self.lua, doc, 0)?)?;
            }
            let (svg, errors, class, problems): (
                Option<LuaString>,
                u32,
                Option<String>,
                Option<Table>,
            ) = draw.call(o)?;
            let mut found = Vec::new();
            if let Some(list) = problems {
                found = problems_from(list)?;
            }
            Ok(Drawn {
                svg: svg.map(|s| s.to_string_lossy()).unwrap_or_default(),
                lua_errors: errors,
                class,
                problems: found,
            })
        })();
        self.deadline.set(None);
        // A fresh scene every draw: let the last one's frames go.
        let _ = self.lua.gc_collect();
        result.map_err(|e| e.to_string())
    }

    /// UI1's `propose_ui`: the language's check on a document, without
    /// drawing it, so no WOWAPI data is needed. Through `preview.check`, the
    /// same checker `draw` uses, wherever it lives.
    pub fn check_ui(&self, doc: &serde_json::Value) -> Result<Vec<Problem>, String> {
        self.deadline
            .set(Some(Instant::now() + Duration::from_secs(10)));
        let result = (|| -> mlua::Result<Vec<Problem>> {
            let check: mlua::Function = self
                .lua
                .load("return require('preview').check")
                .set_mode(ChunkMode::Text)
                .eval()?;
            let list: Table = check.call(json_to_lua(&self.lua, doc, 0)?)?;
            problems_from(list)
        })();
        self.deadline.set(None);
        result.map_err(|e| e.to_string())
    }
}

/// uicheck's list of `{ path, code, message }`.
fn problems_from(list: Table) -> mlua::Result<Vec<Problem>> {
    let mut found = Vec::new();
    for p in list.sequence_values::<Table>() {
        let p = p?;
        found.push(Problem {
            path: p.get::<Option<String>>("path")?.unwrap_or_default(),
            code: p.get::<Option<String>>("code")?.unwrap_or_default(),
            message: p.get::<Option<String>>("message")?.unwrap_or_default(),
        });
    }
    Ok(found)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn host(data: Option<PathBuf>, scratch: &Path) -> Host {
        Host {
            data,
            scratch: scratch.to_path_buf(),
            extract: None,
            slots: BTreeMap::new(),
        }
    }

    #[test]
    fn paths_cant_climb_out_or_name_a_drive() {
        assert_eq!(
            components("/a/./b/../c"),
            Some(vec!["a".into(), "c".into()])
        );
        assert_eq!(components("/../etc/passwd"), None);
        assert_eq!(components("/scratch/../../x"), None);
        assert_eq!(components("relative"), None);
        assert_eq!(components("/extract/C:/Windows"), None);
    }

    #[test]
    fn the_tree_reads_the_bundle_and_writes_only_scratch() {
        let tmp = tempfile::tempdir().unwrap();
        let mut h = host(None, tmp.path());
        h.slots.insert(
            "Data/Settings.lua".into(),
            b"ForeverBuddyData_Settings = { x = 1 }".to_vec(),
        );
        assert!(h.read("/tools/wowapi/wowapi/sim.lua").is_some());
        assert!(h
            .read("/tools/addon-shots/../wowapi/wowapi/host.lua")
            .is_some());
        assert_eq!(
            h.read("/addon/ForeverBuddy/Data/Settings.lua").unwrap(),
            b"ForeverBuddyData_Settings = { x = 1 }"
        );
        assert!(h.read("/addon/ForeverBuddy/ForeverBuddy.toc").is_some());
        assert!(h
            .list("/addon/ForeverBuddy")
            .unwrap()
            .contains(&"Data".to_string()));
        assert!(h
            .list("/tools/wowapi/wowapi")
            .unwrap()
            .contains(&"sim.lua".to_string()));
        assert!(h.exists("/addon/ForeverBuddy"));
        // No data downloaded: the data folder isn't there.
        assert!(h.read("/tools/wowapi/wowapi/data/apidocs.lua").is_none());
        // Writes: scratch only.
        assert!(h.write("/scratch/a.txt", b"hi").is_ok());
        assert_eq!(std::fs::read(tmp.path().join("a.txt")).unwrap(), b"hi");
        assert!(h.write("/tools/wowapi/wowapi/sim.lua", b"x").is_err());
        assert!(h
            .write("/addon/ForeverBuddy/ForeverBuddy.lua", b"x")
            .is_err());
        // The art folder only exists with the switch on.
        assert!(h.read("/extract/interface/addons/x.toc").is_none());
    }

    #[test]
    fn the_lua_has_no_io_os_shell_debug_or_bytecode() {
        let tmp = tempfile::tempdir().unwrap();
        let s = Session::new(Arc::new(RwLock::new(host(None, tmp.path())))).unwrap();
        let check = |code: &str| -> bool { s.lua.load(code).eval::<bool>().unwrap() };
        assert!(check("return io == nil"));
        assert!(check(
            "return os.execute == nil and os.remove == nil and os.getenv == nil and os.exit == nil"
        ));
        assert!(check(
            "return dofile == nil and loadfile == nil and load == nil and string.dump == nil"
        ));
        assert!(check(
            "return debug.sethook == nil and debug.getupvalue == nil and debug.getregistry == nil"
        ));
        assert!(check("return package.loadlib == nil and require ~= nil"));
        // Bytecode is refused, whatever loads it.
        assert!(check(
            r#"local f, err = loadstring("\27LuaQ") return f == nil and err == "bytecode isn't loaded""#
        ));
        // The stack shim names files, nothing more.
        assert!(check(
            r#"local f = loadstring("return debug.getinfo(1, 'S').source", "@/tools/x.lua") return f() == "@/tools/x.lua""#
        ));
    }

    /// WOWAPI's data folder as `tools/wowapi/fetch-data.sh` leaves it (CI
    /// runs that before `cargo test`).
    pub(crate) fn repo_data() -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tools/wowapi/wowapi/data");
        assert!(
            dir.join("apidocs.lua").exists(),
            "no WOWAPI data: run tools/wowapi/fetch-data.sh"
        );
        dir
    }

    #[test]
    fn the_scene_draws_in_the_sandbox() {
        let tmp = tempfile::tempdir().unwrap();
        let s = Session::new(Arc::new(RwLock::new(host(Some(repo_data()), tmp.path())))).unwrap();
        let opts = DrawOptions {
            class: Some("rogue".into()),
            art: false,
            moment: "standing",
            ui: None,
        };
        let started = Instant::now();
        let drawn = s.draw(&opts, Duration::from_secs(60)).unwrap();
        let first = started.elapsed();
        assert!(
            drawn.svg.starts_with("<svg"),
            "{}",
            &drawn.svg[..drawn.svg.len().min(200)]
        );
        let again = Instant::now();
        s.draw(&opts, Duration::from_secs(60)).unwrap();
        eprintln!(
            "preview: first draw {first:?}, next {:?}, {} Lua errors, {} bytes",
            again.elapsed(),
            drawn.lua_errors,
            drawn.svg.len()
        );
    }

    /// ED1: every moment the Your UI tab offers draws without a Lua error.
    #[test]
    fn every_moment_draws() {
        let tmp = tempfile::tempdir().unwrap();
        let s = Session::new(Arc::new(RwLock::new(host(Some(repo_data()), tmp.path())))).unwrap();
        for moment in ["standing", "combat", "stealth", "low_health"] {
            let opts = DrawOptions {
                class: Some("rogue".into()),
                art: false,
                moment,
                ui: None,
            };
            let drawn = s.draw(&opts, Duration::from_secs(60)).unwrap();
            assert!(
                drawn.svg.starts_with("<svg"),
                "{moment}: {}",
                &drawn.svg[..drawn.svg.len().min(200)]
            );
            assert_eq!(drawn.lua_errors, 0, "{moment}");
        }
    }

    #[test]
    fn a_runaway_draw_is_stopped() {
        let tmp = tempfile::tempdir().unwrap();
        let s = Session::new(Arc::new(RwLock::new(host(None, tmp.path())))).unwrap();
        s.deadline
            .set(Some(Instant::now() + Duration::from_millis(50)));
        let err = s.lua.load("while true do end").exec().unwrap_err();
        assert!(err.to_string().contains("took too long"), "{err}");
    }
}
