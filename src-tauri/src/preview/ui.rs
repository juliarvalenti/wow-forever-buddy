//! UI1's render_ui: a UI document drawn on the player's screen, as a PNG.
//!
//! The document is data in the prototype interpreter's format
//! (`tools/ui-proto`: the designer's round 4 interpreter, UF.lua, and its
//! spec). It arrives as JSON and is written out as a Lua table literal, the
//! interpreter's `Spec.lua`, so it can only ever be data: strings, numbers,
//! booleans and tables, within size limits. The preview's sandbox
//! (`sandbox`) draws the logged-in scene with the interpreter loaded, at a
//! moment (standing, combat, stealth, low health), and the SVG becomes a PNG
//! here (resvg).
//!
//! The same code path serves the agent connection (`agent`'s render_ui,
//! which hands the PNG back as image content) and the command line
//! (`<app> render <doc.json> --moment combat --out shot.png`).

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use resvg::usvg;
use serde_json::Value;

use super::sandbox::{DrawOptions, Host, Session};
use super::{data, Moment};

/// Round 4 as a document: the starting point an agent gets back when it
/// asks for a render without one.
pub const EXAMPLE: &str = include_str!("../../../tools/ui-proto/examples/round4.json");

/// The document's limits. Round 4 is under 5 KB and 300 values.
pub const MAX_BYTES: usize = 64 * 1024;
const MAX_DEPTH: usize = 16;
const MAX_VALUES: usize = 5000;
const MAX_ITEMS: usize = 256;
const MAX_STRING: usize = 512;
const MAX_KEY: usize = 64;

/// A render takes a few seconds; this stops a runaway one.
const DRAW_LIMIT: Duration = Duration::from_secs(90);

/// What the player sees when there's no WOWAPI data yet.
pub const NO_DATA: &str = "Forever Buddy can't draw yet: open the app's Your UI tab and \
download the preview data first (once, about 8 MB).";

/// The document as the interpreter's `Spec.lua`, or what's wrong with it,
/// at its path (`units[0].parts[1]: ...`).
pub fn spec_lua(doc: &Value) -> Result<String, String> {
    let size = serde_json::to_string(doc).map(|s| s.len()).unwrap_or(0);
    if size > MAX_BYTES {
        return Err(format!(
            "the document is {size} bytes; at most {MAX_BYTES} are drawn"
        ));
    }
    if !doc.is_object() {
        return Err("the document must be an object (the spec's top-level table)".into());
    }
    let mut out = String::from("FBUnitProtoSpec = ");
    let mut values = 0;
    write_lua(doc, "", 0, &mut values, &mut out)?;
    out.push('\n');
    Ok(out)
}

fn at(path: &str) -> &str {
    if path.is_empty() {
        "the document"
    } else {
        path
    }
}

fn write_lua(
    v: &Value,
    path: &str,
    depth: usize,
    values: &mut usize,
    out: &mut String,
) -> Result<(), String> {
    *values += 1;
    if *values > MAX_VALUES {
        return Err(format!(
            "{}: more than {MAX_VALUES} values in all",
            at(path)
        ));
    }
    if depth > MAX_DEPTH {
        return Err(format!("{}: nested more than {MAX_DEPTH} deep", at(path)));
    }
    match v {
        Value::Null => {
            return Err(format!(
                "{}: null isn't a value; leave the key out instead",
                at(path)
            ))
        }
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => match (n.as_i64(), n.as_f64()) {
            (Some(i), _) => out.push_str(&i.to_string()),
            (None, Some(f)) if f.is_finite() => out.push_str(&format!("{f:?}")),
            _ => return Err(format!("{}: not a number Lua can hold", at(path))),
        },
        Value::String(s) => {
            if s.len() > MAX_STRING {
                return Err(format!(
                    "{}: a string of {} bytes; at most {MAX_STRING}",
                    at(path),
                    s.len()
                ));
            }
            lua_string(s, out);
        }
        Value::Array(items) => {
            if items.len() > MAX_ITEMS {
                return Err(format!(
                    "{}: {} items; at most {MAX_ITEMS}",
                    at(path),
                    items.len()
                ));
            }
            out.push('{');
            for (i, item) in items.iter().enumerate() {
                write_lua(item, &format!("{path}[{i}]"), depth + 1, values, out)?;
                out.push(',');
            }
            out.push('}');
        }
        Value::Object(map) => {
            if map.len() > MAX_ITEMS {
                return Err(format!(
                    "{}: {} keys; at most {MAX_ITEMS}",
                    at(path),
                    map.len()
                ));
            }
            out.push('{');
            for (key, item) in map {
                let child = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                out.push_str(&lua_key(key).ok_or_else(|| {
                    format!(
                        "{child}: a key is a name (letters, digits, _; not starting with a digit) \
                         or a whole number such as an action slot (\"3\")"
                    )
                })?);
                out.push('=');
                write_lua(item, &child, depth + 1, values, out)?;
                out.push(',');
            }
            out.push('}');
        }
    }
    Ok(())
}

/// A key as Lua writes it: a name as is, digits as a number (JSON's keys are
/// strings, and the interpreter looks action slots up by number).
fn lua_key(key: &str) -> Option<String> {
    if key.is_empty() || key.len() > MAX_KEY {
        return None;
    }
    if key.bytes().all(|b| b.is_ascii_digit()) {
        return (key.len() <= 6).then(|| format!("[{}]", key.trim_start_matches('0').max("0")));
    }
    let mut bytes = key.bytes();
    let first = bytes.next()?;
    let name = (first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_');
    (name && !LUA_WORDS.contains(&key)).then(|| key.to_string())
}

const LUA_WORDS: [&str; 21] = [
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "if", "in", "local",
    "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

/// A Lua string literal: quotes, backslashes and control bytes escaped.
fn lua_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\{:03}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// A render to make.
pub struct Request<'a> {
    /// WOWAPI's data folder (the preview's, downloaded in the app).
    pub data: &'a Path,
    /// A folder the scene may write to.
    pub scratch: &'a Path,
    pub doc: &'a Value,
    pub moment: Moment,
    /// A fixture character's class ("rogue", the default).
    pub class: Option<String>,
    /// The PNG's size against the 1920x1080 screen.
    pub scale: f32,
}

#[derive(Debug)]
pub struct Rendered {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// Lua errors in the scene (the interpreter's, or the emulator's gaps).
    pub lua_errors: u32,
}

/// Draws `req.doc` at `req.moment` and returns it as a PNG.
pub fn render(req: &Request) -> Result<Rendered, String> {
    let spec = spec_lua(req.doc)?;
    if !data::is_ready(req.data) {
        return Err(NO_DATA.into());
    }
    let host = Host {
        data: Some(req.data.to_path_buf()),
        scratch: req.scratch.to_path_buf(),
        extract: None,
        slots: Default::default(),
        ui: Some(spec.into_bytes()),
    };
    let session = Session::new(Arc::new(RwLock::new(host))).map_err(|e| e.to_string())?;
    let drawn = session.draw(
        &DrawOptions {
            class: Some(req.class.clone().unwrap_or_else(|| "rogue".into())),
            art: false,
            moment: req.moment.as_str(),
            ui: true,
        },
        DRAW_LIMIT,
    )?;
    let (png, width, height) = png(&drawn.svg, req.scale)?;
    Ok(Rendered {
        png,
        width,
        height,
        lua_errors: drawn.lua_errors,
    })
}

/// An SVG from the preview as a PNG, scaled. Its fonts are embedded as
/// `@font-face` data URLs, which resvg doesn't read, so each is loaded into
/// the font database and its family name put where the SVG names it.
pub fn png(svg: &str, scale: f32) -> Result<(Vec<u8>, u32, u32), String> {
    let mut db = usvg::fontdb::Database::new();
    db.load_system_fonts();
    let svg = embed_fonts(svg, &mut db);
    let opt = usvg::Options {
        fontdb: Arc::new(db),
        ..Default::default()
    };
    let tree = usvg::Tree::from_str(&svg, &opt).map_err(|e| format!("the frame's SVG: {e}"))?;
    let size = tree
        .size()
        .to_int_size()
        .scale_by(scale)
        .ok_or("the frame has no size")?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width(), size.height())
        .ok_or("the frame has no size")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let bytes = pixmap.encode_png().map_err(|e| e.to_string())?;
    Ok((bytes, size.width(), size.height()))
}

/// Takes out each `<style>@font-face{font-family:NAME;src:url(data:...)}</style>`,
/// loads its font, and names the font's own family wherever the SVG says
/// `font-family="NAME, ..."`.
fn embed_fonts(svg: &str, db: &mut usvg::fontdb::Database) -> String {
    use base64::Engine;
    const OPEN: &str = "<style>@font-face{font-family:";
    let mut out = String::with_capacity(svg.len());
    let mut names: Vec<(String, String)> = Vec::new();
    let mut rest = svg;
    while let Some(start) = rest.find(OPEN) {
        out.push_str(&rest[..start]);
        let after = &rest[start + OPEN.len()..];
        let Some(end) = after.find("</style>") else {
            out.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let rule = &after[..end];
        rest = &after[end + "</style>".len()..];
        let name = rule.split(';').next().unwrap_or("").trim();
        let data = rule
            .split_once("base64,")
            .map(|(_, d)| d.trim_end_matches(['}', ')']))
            .and_then(|d| base64::engine::general_purpose::STANDARD.decode(d).ok());
        if let (false, Some(bytes)) = (name.is_empty(), data) {
            let ids = db.load_font_source(usvg::fontdb::Source::Binary(Arc::new(bytes)));
            if let Some(family) = ids
                .first()
                .and_then(|id| db.face(*id))
                .and_then(|f| f.families.first())
            {
                names.push((name.to_string(), family.0.clone()));
            }
        }
    }
    out.push_str(rest);
    for (name, family) in names {
        for end in [",", "\""] {
            out = out.replace(
                &format!("font-family=\"{name}{end}"),
                &format!("font-family=\"'{family}'{end}"),
            );
        }
    }
    out
}

/// The app's preview data folder, where the app downloads WOWAPI's data.
pub fn data_dir(local_data: &Path) -> PathBuf {
    super::dir(local_data).join("data")
}

const USAGE: &str = "usage: render <doc.json> [--moment standing|combat|stealth|low_health] \
[--class rogue] [--out shot.png] [--scale 1] [--data <WOWAPI data folder>]";

/// `<app> render ...`: the same render to a file. Returns the exit code.
pub fn cli(args: &[String], local_data: Option<PathBuf>) -> i32 {
    match cli_inner(args, local_data) {
        Ok(msg) => {
            println!("{msg}");
            0
        }
        Err(e) => {
            eprintln!("render: {e}");
            1
        }
    }
}

fn cli_inner(args: &[String], local_data: Option<PathBuf>) -> Result<String, String> {
    let mut doc_path = None;
    let (mut moment, mut class, mut out, mut scale, mut data) = (
        Moment::Standing,
        None,
        PathBuf::from("ui.png"),
        1.0f32,
        None,
    );
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{a} needs a value; {USAGE}"))
        };
        match a.as_str() {
            "--moment" => {
                moment = match value()?.as_str() {
                    "standing" => Moment::Standing,
                    "combat" => Moment::Combat,
                    "stealth" => Moment::Stealth,
                    "low_health" => Moment::LowHealth,
                    m => return Err(format!("no moment \"{m}\"; {USAGE}")),
                }
            }
            "--class" => class = Some(value()?),
            "--out" => out = PathBuf::from(value()?),
            "--scale" => {
                scale = value()?
                    .parse()
                    .ok()
                    .filter(|s: &f32| (0.1..=2.0).contains(s))
                    .ok_or("--scale is a number from 0.1 to 2")?
            }
            "--data" => data = Some(PathBuf::from(value()?)),
            _ if a.starts_with("--") => return Err(format!("no option {a}; {USAGE}")),
            _ => doc_path = Some(PathBuf::from(a)),
        }
    }
    let doc_path = doc_path.ok_or(USAGE)?;
    let text = std::fs::read(&doc_path).map_err(|e| format!("{}: {e}", doc_path.display()))?;
    let doc: Value = serde_json::from_slice(&text)
        .map_err(|e| format!("{}: not JSON ({e})", doc_path.display()))?;
    let data = data
        .or_else(|| local_data.map(|d| data_dir(&d)))
        .ok_or(NO_DATA)?;
    let scratch = tempfile::tempdir().map_err(|e| e.to_string())?;
    let r = render(&Request {
        data: &data,
        scratch: scratch.path(),
        doc: &doc,
        moment,
        class,
        scale,
    })?;
    std::fs::write(&out, &r.png).map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(format!(
        "{} ({}x{}, {} Lua errors)",
        out.display(),
        r.width,
        r.height,
        r.lua_errors
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_document_becomes_a_lua_table_of_data_only() {
        let doc = json!({
            "px": 0.711,
            "hide": ["PlayerFrame"],
            "cooldowns": { "3": [0.6, "6"] },
            "text": "say \"hi\"\n\\ end ]]",
            "on": true,
        });
        let lua = spec_lua(&doc).unwrap();
        assert!(lua.starts_with("FBUnitProtoSpec = {"), "{lua}");
        assert!(lua.contains("cooldowns={[3]={0.6,\"6\",},}"), "{lua}");
        assert!(lua.contains(r#"text="say \"hi\"\n\\ end ]]""#), "{lua}");
        // It runs as Lua and reads back the same.
        let l = mlua::Lua::new();
        l.load(&lua).exec().unwrap();
        let back: String = l
            .load("return FBUnitProtoSpec.text .. FBUnitProtoSpec.cooldowns[3][2]")
            .eval()
            .unwrap();
        assert_eq!(back, "say \"hi\"\n\\ end ]]6");
    }

    #[test]
    fn what_isnt_data_is_refused_at_its_path() {
        let err = |doc: Value| spec_lua(&doc).unwrap_err();
        assert!(err(json!([1, 2])).contains("must be an object"));
        assert_eq!(
            err(json!({ "units": [{ "parts": [null] }] })),
            "units[0].parts[0]: null isn't a value; leave the key out instead"
        );
        assert!(err(json!({ "a b": 1 })).starts_with("a b: a key is a name"));
        assert!(err(json!({ "end": 1 })).starts_with("end: a key is a name"));
        assert!(err(json!({ "x": "y".repeat(600) })).contains("at most 512"));
        let mut deep = json!(1);
        for _ in 0..20 {
            deep = json!({ "x": deep });
        }
        assert!(err(deep).contains("nested more than 16 deep"));
        assert!(err(json!({ "x": "z".repeat(70_000) })).contains("bytes; at most"));
    }

    #[test]
    fn the_example_is_a_valid_document() {
        let doc: Value = serde_json::from_str(EXAMPLE).unwrap();
        let lua = spec_lua(&doc).unwrap();
        let l = mlua::Lua::new();
        l.load(&lua).exec().unwrap();
        let n: i64 = l.load("return #FBUnitProtoSpec.units").eval().unwrap();
        assert_eq!(n, 2, "player and target");
    }

    /// The ticket's check: the example drawn in the sandbox comes back as a
    /// PNG with the round 4 frames on it, in every moment, with no Lua error.
    #[test]
    fn the_example_renders_to_a_png_in_every_moment() {
        let tmp = tempfile::tempdir().unwrap();
        let data = super::super::sandbox::tests::repo_data();
        let doc: Value = serde_json::from_str(EXAMPLE).unwrap();
        for moment in [
            Moment::Standing,
            Moment::Combat,
            Moment::Stealth,
            Moment::LowHealth,
        ] {
            let r = render(&Request {
                data: &data,
                scratch: tmp.path(),
                doc: &doc,
                moment,
                class: None,
                scale: 0.5,
            })
            .unwrap();
            assert_eq!(&r.png[1..4], b"PNG", "{moment:?}");
            assert_eq!((r.width, r.height), (960, 540), "{moment:?}");
            assert_eq!(r.lua_errors, 0, "{moment:?}");
        }
    }

    #[test]
    fn without_data_it_says_where_to_get_it() {
        let tmp = tempfile::tempdir().unwrap();
        let doc: Value = serde_json::from_str(EXAMPLE).unwrap();
        let err = render(&Request {
            data: &tmp.path().join("none"),
            scratch: tmp.path(),
            doc: &doc,
            moment: Moment::Standing,
            class: None,
            scale: 1.0,
        })
        .unwrap_err();
        assert_eq!(err, NO_DATA);
    }

    #[test]
    fn embedded_fonts_are_named_by_their_own_family() {
        use base64::Engine;
        let ttf = crate::addon::bundled_files()
            .iter()
            .find(|(f, _)| *f == "Media/BarlowSCB.ttf")
            .unwrap()
            .1;
        let svg = format!(
            "<svg><style>@font-face{{font-family:wf1;src:url(data:font/ttf;base64,{})}}</style>\
             <text font-family=\"wf1, serif\">1.2 K</text></svg>",
            base64::engine::general_purpose::STANDARD.encode(ttf)
        );
        let mut db = usvg::fontdb::Database::new();
        let out = embed_fonts(&svg, &mut db);
        assert!(!out.contains("@font-face"), "{out}");
        assert!(out.contains("font-family=\"'Barlow"), "{out}");
    }
}
