//! UI1's render_ui: a UI document drawn on the player's screen, as a PNG.
//!
//! The document is JSON in the UI language (the addon's own `UI.lua` draws
//! it, and its `UICheck.lua` checks it). Here it's held to size
//! limits, handed to
//! the preview's sandbox as a Lua table built from the JSON (never source),
//! and drawn over the logged-in scene at a moment (standing, combat,
//! stealth, low health). A document the check refuses comes back as its
//! problems; one that passes comes back as a PNG (resvg).
//!
//! The same code path serves the agent connection (`agent`'s render_ui,
//! which hands the PNG back as image content) and the command line
//! (`<app> render <doc.json> --moment combat --out shot.png`).

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use resvg::usvg;
use serde_json::Value;

pub use super::sandbox::Problem;
use super::sandbox::{DrawOptions, Host, Session};
use super::{data, Moment};

/// Round 4 as a document: what an agent gets back to start from when it
/// asks for a render without one.
pub const EXAMPLE: &str = include_str!("../../../tools/addon-shots/ui-examples/round4.json");

/// The document's limits, before the language's own check. Round 4 is
/// about 4 KB.
pub const MAX_BYTES: usize = 64 * 1024;
const MAX_DEPTH: usize = 16;
const MAX_VALUES: usize = 5000;
const MAX_ITEMS: usize = 512;
const MAX_STRING: usize = 512;
const MAX_KEY: usize = 64;

/// A render takes a few seconds; this stops a runaway one.
const DRAW_LIMIT: Duration = Duration::from_secs(90);

/// What the agent hears when there's no WOWAPI data yet.
pub const NO_DATA: &str = "Forever Buddy can't draw yet: open the app's Your UI tab and \
download the preview data first (once, about 8 MB).";

/// The fonts a document can name that the addon carries: loaded into the
/// PNG's font database whether or not the SVG embeds them.
const ADDON_FONTS: [&str; 3] = [
    "Media/BarlowSCB.ttf",
    "Media/Geist.ttf",
    "Media/Marcellus.ttf",
];

/// What doesn't fit the limits, with JSON pointers like the language's
/// check gives. Empty when the document may go on to the check.
pub fn limits(doc: &Value) -> Vec<Problem> {
    let mut found = Vec::new();
    let size = serde_json::to_string(doc).map(|s| s.len()).unwrap_or(0);
    if size > MAX_BYTES {
        found.push(problem(
            "",
            "too-big",
            &format!("the document is {size} bytes; at most {MAX_BYTES} are drawn."),
        ));
        return found;
    }
    if !doc.is_object() {
        found.push(problem("", "not-an-object", "a document is a JSON object."));
        return found;
    }
    let mut values = 0;
    walk(doc, "", 0, &mut values, &mut found);
    found
}

fn problem(path: &str, code: &str, message: &str) -> Problem {
    Problem {
        path: path.to_string(),
        code: code.to_string(),
        message: message.to_string(),
    }
}

/// A JSON pointer's key, escaped (`~0`, `~1`).
fn pointer(path: &str, key: &str) -> String {
    format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"))
}

fn walk(v: &Value, path: &str, depth: usize, values: &mut usize, found: &mut Vec<Problem>) {
    if !found.is_empty() {
        return;
    }
    *values += 1;
    if *values > MAX_VALUES {
        found.push(problem(
            path,
            "too-many-values",
            &format!("more than {MAX_VALUES} values in all."),
        ));
        return;
    }
    if depth > MAX_DEPTH {
        found.push(problem(
            path,
            "too-deep",
            &format!("nested more than {MAX_DEPTH} deep."),
        ));
        return;
    }
    match v {
        Value::Null => found.push(problem(
            path,
            "null",
            "null isn't a value; leave the key out instead.",
        )),
        Value::String(s) if s.len() > MAX_STRING => found.push(problem(
            path,
            "string-too-long",
            &format!("a string of {} bytes; at most {MAX_STRING}.", s.len()),
        )),
        Value::Array(items) if items.len() > MAX_ITEMS => found.push(problem(
            path,
            "too-many-items",
            &format!("{} items; at most {MAX_ITEMS}.", items.len()),
        )),
        Value::Array(items) => {
            for (i, item) in items.iter().enumerate() {
                walk(item, &format!("{path}/{i}"), depth + 1, values, found);
            }
        }
        Value::Object(map) if map.len() > MAX_ITEMS => found.push(problem(
            path,
            "too-many-keys",
            &format!("{} keys; at most {MAX_ITEMS}.", map.len()),
        )),
        Value::Object(map) => {
            for (key, item) in map {
                let child = pointer(path, key);
                if key.is_empty() || key.len() > MAX_KEY {
                    found.push(problem(
                        &child,
                        "bad-key",
                        &format!("a key is 1 to {MAX_KEY} bytes."),
                    ));
                    return;
                }
                walk(item, &child, depth + 1, values, found);
            }
        }
        _ => {}
    }
}

/// A document's problems without drawing it: the limits, then the
/// language's own check (`uicheck.lua`) in a sandbox of its own. Needs no
/// WOWAPI data. Empty when it would draw.
pub fn check(doc: &Value) -> Result<Vec<Problem>, String> {
    let problems = limits(doc);
    if !problems.is_empty() {
        return Ok(problems);
    }
    let scratch = tempfile::tempdir().map_err(|e| e.to_string())?;
    let host = Host {
        data: None,
        scratch: scratch.path().to_path_buf(),
        extract: None,
        slots: Default::default(),
    };
    let session = Session::new(Arc::new(RwLock::new(host))).map_err(|e| e.to_string())?;
    session.check_ui(doc)
}

/// The problems as lines an agent can read: `/parts/3/kind  unknown-kind: ...`.
pub fn problem_lines(problems: &[Problem]) -> String {
    problems
        .iter()
        .map(|p| {
            format!(
                "{}  {}: {}",
                if p.path.is_empty() { "/" } else { &p.path },
                p.code,
                p.message
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
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

/// A render's outcome: the picture, or why the document can't be drawn.
#[derive(Debug)]
pub enum Outcome {
    Drawn(Rendered),
    Refused(Vec<Problem>),
}

/// Checks `req.doc` and draws it at `req.moment`.
pub fn render(req: &Request) -> Result<Outcome, String> {
    let problems = limits(req.doc);
    if !problems.is_empty() {
        return Ok(Outcome::Refused(problems));
    }
    if !data::is_ready(req.data) {
        return Err(NO_DATA.into());
    }
    let host = Host {
        data: Some(req.data.to_path_buf()),
        scratch: req.scratch.to_path_buf(),
        extract: None,
        slots: Default::default(),
    };
    let session = Session::new(Arc::new(RwLock::new(host))).map_err(|e| e.to_string())?;
    let drawn = session.draw(
        &DrawOptions {
            class: Some(req.class.clone().unwrap_or_else(|| "rogue".into())),
            art: false,
            moment: req.moment.as_str(),
            ui: Some(req.doc.clone()),
        },
        DRAW_LIMIT,
    )?;
    if !drawn.problems.is_empty() {
        return Ok(Outcome::Refused(drawn.problems));
    }
    let (png, width, height) = png(&drawn.svg, req.scale)?;
    Ok(Outcome::Drawn(Rendered {
        png,
        width,
        height,
        lua_errors: drawn.lua_errors,
    }))
}

/// An SVG from the preview as a PNG, scaled. The addon's fonts (Barlow,
/// Geist, Marcellus) go into the font database; the SVG's own embedded
/// `@font-face` fonts, which resvg doesn't read, are loaded too, and their
/// family names put where the SVG names them.
pub fn png(svg: &str, scale: f32) -> Result<(Vec<u8>, u32, u32), String> {
    let mut db = usvg::fontdb::Database::new();
    db.load_system_fonts();
    for name in ADDON_FONTS {
        if let Some((_, bytes)) = crate::addon::bundled_files()
            .iter()
            .find(|(f, _)| *f == name)
        {
            db.load_font_data(bytes.to_vec());
        }
    }
    let svg = embed_fonts(svg, &mut db);
    // Images come from data: URLs only. An href naming a file (which usvg
    // would otherwise read) draws nothing, so no change to the renderer can
    // put a local file into a picture an agent gets back.
    let opt = usvg::Options {
        fontdb: Arc::new(db),
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: usvg::ImageHrefResolver::default_data_resolver(),
            resolve_string: Box::new(|_, _| None),
        },
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
    let outcome = render(&Request {
        data: &data,
        scratch: scratch.path(),
        doc: &doc,
        moment,
        class,
        scale,
    })?;
    let r = match outcome {
        Outcome::Drawn(r) => r,
        Outcome::Refused(problems) => {
            return Err(format!(
                "{} isn't drawn:\n{}",
                doc_path.display(),
                problem_lines(&problems)
            ))
        }
    };
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

    fn example() -> Value {
        serde_json::from_str(EXAMPLE).unwrap()
    }

    #[test]
    fn what_doesnt_fit_is_refused_at_its_pointer() {
        let codes = |doc: Value| -> Vec<(String, String)> {
            limits(&doc).into_iter().map(|p| (p.path, p.code)).collect()
        };
        assert!(limits(&example()).is_empty(), "the example fits");
        assert_eq!(codes(json!([1])), [("".into(), "not-an-object".into())]);
        assert_eq!(
            codes(json!({ "parts": [{ "anchor": null }] })),
            [("/parts/0/anchor".into(), "null".into())]
        );
        assert_eq!(
            codes(json!({ "a/b": { "c": "x".repeat(600) } })),
            [("/a~1b/c".into(), "string-too-long".into())]
        );
        let mut deep = json!(1);
        for _ in 0..20 {
            deep = json!({ "x": deep });
        }
        assert_eq!(codes(deep)[0].1, "too-deep");
        assert_eq!(
            codes(json!({ "x": "z".repeat(70_000) })),
            [("".into(), "too-big".into())]
        );
    }

    fn request<'a>(
        data: &'a Path,
        scratch: &'a Path,
        doc: &'a Value,
        moment: Moment,
    ) -> Request<'a> {
        Request {
            data,
            scratch,
            doc,
            moment,
            class: None,
            scale: 0.5,
        }
    }

    /// The ticket's check: round 4 drawn in the sandbox comes back as a PNG,
    /// in every moment, with no Lua error.
    #[test]
    fn round4_renders_to_a_png_in_every_moment() {
        let tmp = tempfile::tempdir().unwrap();
        let data = super::super::sandbox::tests::repo_data();
        let doc = example();
        for moment in [
            Moment::Standing,
            Moment::Combat,
            Moment::Stealth,
            Moment::LowHealth,
        ] {
            let Outcome::Drawn(r) = render(&request(&data, tmp.path(), &doc, moment)).unwrap()
            else {
                panic!("{moment:?}: refused");
            };
            assert_eq!(&r.png[1..4], b"PNG", "{moment:?}");
            assert_eq!((r.width, r.height), (960, 540), "{moment:?}");
            assert_eq!(r.lua_errors, 0, "{moment:?}");
        }
    }

    /// A document the language's check refuses comes back as its problems,
    /// with the check's own pointer, and nothing is drawn.
    #[test]
    fn the_checks_problems_come_back() {
        let tmp = tempfile::tempdir().unwrap();
        let data = super::super::sandbox::tests::repo_data();
        let mut doc = example();
        doc["parts"][0]["kind"] = json!("hologram");
        let Outcome::Refused(problems) =
            render(&request(&data, tmp.path(), &doc, Moment::Standing)).unwrap()
        else {
            panic!("drawn");
        };
        assert!(
            problems.iter().any(|p| p.path == "/parts/0/kind"),
            "{}",
            problem_lines(&problems)
        );
    }

    /// propose_ui's check needs no WOWAPI data: its sandbox has none (the
    /// host's `data` is `None`), yet the language's check runs through
    /// `preview.check` and finds a broken document's problems.
    #[test]
    fn the_check_runs_without_wowapi_data() {
        assert!(check(&example()).unwrap().is_empty(), "round 4 passes");
        let mut doc = example();
        doc["parts"][0]["kind"] = json!("hologram");
        let problems = check(&doc).unwrap();
        assert!(
            problems.iter().any(|p| p.path == "/parts/0/kind"),
            "{}",
            problem_lines(&problems)
        );
    }

    #[test]
    fn without_data_it_says_where_to_get_it() {
        let tmp = tempfile::tempdir().unwrap();
        let doc = example();
        let none = tmp.path().join("none");
        let err = render(&request(&none, tmp.path(), &doc, Moment::Standing)).unwrap_err();
        assert_eq!(err, NO_DATA);
    }

    /// Security's line: an image named by a file path isn't read; one in a
    /// data: URL still draws.
    #[test]
    fn only_data_urls_become_images() {
        use base64::Engine;
        let tmp = tempfile::tempdir().unwrap();
        let mut red = resvg::tiny_skia::Pixmap::new(4, 4).unwrap();
        red.fill(resvg::tiny_skia::Color::from_rgba8(255, 0, 0, 255));
        let bytes = red.encode_png().unwrap();
        let file = tmp.path().join("red.png");
        std::fs::write(&file, &bytes).unwrap();
        let svg = |href: &str| {
            format!(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"4\" height=\"4\">\
                 <image href=\"{href}\" width=\"4\" height=\"4\"/></svg>"
            )
        };
        let first_pixel = |svg: &str| {
            let (png, _, _) = png(svg, 1.0).unwrap();
            let img = resvg::tiny_skia::Pixmap::decode_png(&png).unwrap();
            img.pixel(1, 1).unwrap().red()
        };
        assert_eq!(
            first_pixel(&svg(&file.to_string_lossy())),
            0,
            "a file isn't read"
        );
        let data = format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(&bytes)
        );
        assert_eq!(first_pixel(&svg(&data)), 255, "a data: URL draws");
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
