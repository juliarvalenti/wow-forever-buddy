//! UI1's render_ui: an agent sends a UI document and gets back pictures of
//! the player's screen with it drawn, as image content it can look at, in
//! one or several moments and for a test character's class. The render is
//! `preview::ui`'s, the same as the app's `render` command line. It reads
//! none of the player's data (the scene's character is a fixture) and writes
//! only in a temporary folder.
//!
//! UI3: `get_ui_spec` hands back the language reference this build was made
//! with, so an agent learns the language from the tool, not the repo.

use serde::Deserialize;
use serde_json::{json, Value};

use super::Paths;
use crate::preview::{ui, Moment};

pub const NAME: &str = "render_ui";
pub const SPEC: &str = "get_ui_spec";

/// The UI language's reference (`design/specs/ui-language.md`), as built.
const LANGUAGE: &str = include_str!("../../../design/specs/ui-language.md");

const DESCRIPTION: &str = "Draws a UI document on a test WoW: Forever screen and returns one picture per moment. \
A document is a JSON object: `ui` (1), `hide` (Blizzard frames to fade out) and `parts` (plates, bars, text, pips, \
icons, auras, action bars), placed by `anchor` [point, \"screen\", \"char\" or another part's name, relativePoint, \
x, y] in 1080p pixels, bound to unit data (health, power, combo points, auras), with states (combat, stealth, \
low health at `lowAt`, target) that change any property. get_ui_spec returns the whole language; read it before \
your first document. Leave `doc` out to draw round 4 and get its document back as a starting point. `moments` \
draws several at once (standing, combat, stealth, low_health, no_target: out of combat with nothing targeted, \
hidden: combat with health, power and aura timing hidden from the addon as the game does; `all` for every one), \
`class` picks the test character (warrior, paladin, hunter, rogue, priest, shaman, mage, warlock or druid in cat \
form; rogue by default; an unknown one is refused with the classes there are), `scale` sizes the pictures \
(0.25 to 1 of the 1920x1080 screen; two thirds by default). A document that doesn't pass the check comes back as \
its problems instead, each with a JSON pointer into the document and how to fix it. Only data: 64 KB at most. \
Without the MCP, the app's command line draws the same: `<app> render doc.json --moment all --class rogue`.";

pub fn list() -> Vec<Value> {
    vec![
        json!({
            "name": NAME,
            "description": DESCRIPTION,
            "inputSchema": {
                "type": "object",
                "properties": {
                    "doc": { "type": "object", "description": "The UI document. Leave out for the round 4 example." },
                    "moment": { "type": "string", "description": "One moment: standing (default), combat, stealth, low_health, no_target or hidden." },
                    "moments": { "type": "array", "items": { "type": "string" }, "maxItems": 6,
                                 "description": "Several moments in one call, e.g. [\"standing\", \"combat\"], or [\"all\"]." },
                    "class": { "type": "string", "description": "The test character's class; rogue by default." },
                    "scale": { "type": "number", "minimum": 0.25, "maximum": 1,
                               "description": "Picture size against the 1920x1080 screen; 0.667 by default." },
                },
                "additionalProperties": false,
            },
            "annotations": { "readOnlyHint": true, "openWorldHint": false },
        }),
        json!({
            "name": SPEC,
            "description": "The UI language's full reference, as this version of Forever Buddy reads it: every part, property, binding, state and limit, with examples. Read it before writing a document for render_ui or propose_ui.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
            "annotations": { "readOnlyHint": true, "openWorldHint": false },
        }),
    ]
}

pub fn exists(name: &str) -> bool {
    name == NAME || name == SPEC
}

/// Whether `name` draws pictures (the rest of this module answers in text).
pub fn draws(name: &str) -> bool {
    name == NAME
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    doc: Option<Value>,
    moment: Option<String>,
    moments: Option<Vec<String>>,
    class: Option<String>,
    scale: Option<f32>,
}

/// The reference, for `get_ui_spec`.
pub fn spec() -> Value {
    json!({ "ui_language": LANGUAGE })
}

/// The moments asked for: `moments`, else `moment`, else standing.
fn wanted(a: &Args) -> Result<Vec<Moment>, String> {
    match (&a.moments, &a.moment) {
        (Some(list), _) if !list.is_empty() => ui::moments(&list.join(",")),
        (_, Some(one)) => ui::moments(one),
        _ => Ok(vec![Moment::Standing]),
    }
}

/// A PNG and the moment it shows.
pub type Picture = (Vec<u8>, String);

/// The pictures (each labelled with its moment) and the text to go with them.
pub fn call(paths: &Paths, raw: &Value) -> Result<(Vec<Picture>, String), String> {
    let raw = if raw.is_null() {
        json!({})
    } else {
        raw.clone()
    };
    let args: Args = serde_json::from_value(raw).map_err(|e| format!("Bad arguments: {e}"))?;
    let moments = wanted(&args)?;
    let scale = args.scale.unwrap_or(2.0 / 3.0);
    if !(0.25..=1.0).contains(&scale) {
        return Err("scale is a number from 0.25 to 1.".into());
    }
    let example = args.doc.is_none();
    let doc = match args.doc {
        Some(d) => d,
        None => serde_json::from_str(ui::EXAMPLE).map_err(|e| e.to_string())?,
    };
    let scratch = tempfile::tempdir().map_err(|e| e.to_string())?;
    let outcome = ui::render_moments(
        &ui::Request {
            data: &paths.preview_data,
            scratch: scratch.path(),
            doc: &doc,
            moment: moments[0],
            class: args.class.clone(),
            scale,
        },
        &moments,
    )?;
    let drawn = match outcome {
        ui::Many::Drawn(all) => all,
        ui::Many::Refused(problems) => {
            return Err(format!(
                "Not drawn: the document has {} problem{}.\n{}",
                problems.len(),
                if problems.len() == 1 { "" } else { "s" },
                ui::problem_lines(&problems)
            ))
        }
    };
    let class = args.class.as_deref().unwrap_or("rogue").to_lowercase();
    let mut lines = vec![format!(
        "Drawn for the test {class}, {} picture{} in this order:",
        drawn.len(),
        if drawn.len() == 1 { "" } else { "s" }
    )];
    let mut pictures = Vec::new();
    for (moment, r) in drawn {
        lines.push(format!(
            "- {} ({}x{}), {} Lua errors in the scene",
            moment.as_str(),
            r.width,
            r.height,
            r.lua_errors
        ));
        pictures.push((r.png, moment.as_str().to_string()));
    }
    let mut text = lines.join("\n");
    if example {
        text.push_str("\n\nThe round 4 example document, to start from:\n");
        text.push_str(ui::EXAMPLE);
    }
    Ok((pictures, text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moments_take_aliases_and_suggest_the_right_name() {
        let a = |moment: Option<&str>, moments: Option<Vec<&str>>| Args {
            doc: None,
            moment: moment.map(str::to_string),
            moments: moments.map(|v| v.into_iter().map(str::to_string).collect()),
            class: None,
            scale: None,
        };
        assert_eq!(wanted(&a(None, None)).unwrap(), [Moment::Standing]);
        assert_eq!(
            wanted(&a(Some("lowhealth"), None)).unwrap(),
            [Moment::LowHealth]
        );
        assert_eq!(
            wanted(&a(None, Some(vec!["Low Health", "combat", "combat"]))).unwrap(),
            [Moment::LowHealth, Moment::Combat]
        );
        assert_eq!(
            wanted(&a(None, Some(vec!["notarget", "no-target", "Hidden"]))).unwrap(),
            [Moment::NoTarget, Moment::Hidden]
        );
        assert_eq!(wanted(&a(None, Some(vec!["all"]))).unwrap(), Moment::ALL);
        let e = wanted(&a(Some("stelth"), None)).unwrap_err();
        assert!(e.contains("Did you mean stealth?"), "{e}");
        assert!(
            e.contains("standing, combat, stealth, low_health, no_target, hidden"),
            "{e}"
        );
    }

    #[test]
    fn the_spec_is_the_language_reference() {
        let s = spec()["ui_language"].as_str().unwrap().to_string();
        assert!(s.len() > 1000 && s.contains("parts"), "{}", &s[..200]);
    }
}
