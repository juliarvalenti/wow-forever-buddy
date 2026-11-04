//! UI1's render_ui: an agent sends a UI document and gets back a picture of
//! the player's screen with it drawn, as image content it can look at. The
//! render is `preview::ui`'s, the same as the app's `render` command line.
//! It reads none of the player's data (the scene's character is a fixture)
//! and writes only in a temporary folder.

use serde::Deserialize;
use serde_json::{json, Value};

use super::Paths;
use crate::preview::{ui, Moment};

pub const NAME: &str = "render_ui";

const DESCRIPTION: &str = "Draws a UI document on the player's WoW: Forever screen and returns the picture. \
The document is the prototype UI format: a JSON object with `units` (player and target, each a list of \
[name, part] entries: plate, bar, text, pips), `backdrop` (screen-wide parts such as an actionbar grid or a \
tracked buff icon) and `hide` (Blizzard frames to fade out). Sizes are UI units (1 = 1080/768 screen pixels), \
anchors are [point, \"screen\" or a part's name, relativePoint, x, y]. Leave `doc` out to draw the round 4 \
example and get its document back as a starting point. `moment` draws the screen standing, in combat, in \
stealth or at low health; the character is a level 34 rogue with a hostile target. Only data is accepted: \
strings, numbers, booleans, lists and objects, 64 KB at most.";

pub fn list() -> Vec<Value> {
    vec![json!({
        "name": NAME,
        "description": DESCRIPTION,
        "inputSchema": {
            "type": "object",
            "properties": {
                "doc": { "type": "object", "description": "The UI document. Leave out for the round 4 example." },
                "moment": { "type": "string", "enum": ["standing", "combat", "stealth", "low_health"],
                            "description": "What's happening on screen; standing by default." },
            },
            "additionalProperties": false,
        },
        "annotations": { "readOnlyHint": true, "openWorldHint": false },
    })]
}

pub fn exists(name: &str) -> bool {
    name == NAME
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Args {
    doc: Option<Value>,
    moment: Option<Moment>,
}

/// The PNG and the text to go with it.
pub fn call(paths: &Paths, raw: &Value) -> Result<(Vec<u8>, String), String> {
    let raw = if raw.is_null() {
        json!({})
    } else {
        raw.clone()
    };
    let args: Args = serde_json::from_value(raw).map_err(|e| format!("Bad arguments: {e}"))?;
    let example = args.doc.is_none();
    let doc = match args.doc {
        Some(d) => d,
        None => serde_json::from_str(ui::EXAMPLE).map_err(|e| e.to_string())?,
    };
    let moment = args.moment.unwrap_or_default();
    let scratch = tempfile::tempdir().map_err(|e| e.to_string())?;
    let r = ui::render(&ui::Request {
        data: &paths.preview_data,
        scratch: scratch.path(),
        doc: &doc,
        moment,
        class: None,
        // 1280x720: enough to read the numbers, small enough to look at.
        scale: 2.0 / 3.0,
    })?;
    let mut text = format!(
        "Drawn {} ({}x{}, the 1920x1080 screen at two thirds), {} Lua errors in the scene.",
        moment.as_str(),
        r.width,
        r.height,
        r.lua_errors
    );
    if example {
        text.push_str("\n\nThe round 4 example document, to start from:\n");
        text.push_str(ui::EXAMPLE);
    }
    Ok((r.png, text))
}
