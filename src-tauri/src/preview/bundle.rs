//! The code the preview runs, compiled into the app like the addon
//! (`addon::FILES`): the WOWAPI emulator and the logged-in scene, as they
//! are in `tools/`. Nothing here is Blizzard's: WOWAPI's `data/` folder is
//! downloaded on request (`data`), and the game's own UI only ever comes
//! from the player's files with the art switch on.

/// `(path from the repo root, contents)`.
pub type File = (&'static str, &'static [u8]);

macro_rules! files {
    ($($path:literal),* $(,)?) => {
        &[$(($path, include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../", $path)) as &[u8])),*]
    };
}

/// WOWAPI (`tools/wowapi/wowapi`, every `.lua`) and the scene
/// (`tools/addon-shots`: the scene, its fixtures and layouts, and the
/// preview's entry point). `bundle_has_every_tool_file` keeps this in step
/// with the folders.
pub const TOOLS: &[File] = files![
    "tools/wowapi/wowapi/animation.lua",
    "tools/wowapi/wowapi/api.lua",
    "tools/wowapi/wowapi/assets.lua",
    "tools/wowapi/wowapi/blp.lua",
    "tools/wowapi/wowapi/calibrate.lua",
    "tools/wowapi/wowapi/compat.lua",
    "tools/wowapi/wowapi/docs.lua",
    "tools/wowapi/wowapi/fakeapi.lua",
    "tools/wowapi/wowapi/faker.lua",
    "tools/wowapi/wowapi/framexml.lua",
    "tools/wowapi/wowapi/host.lua",
    "tools/wowapi/wowapi/index.lua",
    "tools/wowapi/wowapi/init.lua",
    "tools/wowapi/wowapi/input.lua",
    "tools/wowapi/wowapi/installer.lua",
    "tools/wowapi/wowapi/layout.lua",
    "tools/wowapi/wowapi/nameplates.lua",
    "tools/wowapi/wowapi/png.lua",
    "tools/wowapi/wowapi/realclient.lua",
    "tools/wowapi/wowapi/render.lua",
    "tools/wowapi/wowapi/resources.lua",
    "tools/wowapi/wowapi/secrets.lua",
    "tools/wowapi/wowapi/secure.lua",
    "tools/wowapi/wowapi/serialize.lua",
    "tools/wowapi/wowapi/settingspanel.lua",
    "tools/wowapi/wowapi/signing.lua",
    "tools/wowapi/wowapi/sim.lua",
    "tools/wowapi/wowapi/templates.lua",
    "tools/wowapi/wowapi/testing.lua",
    "tools/wowapi/wowapi/tga.lua",
    "tools/wowapi/wowapi/toc.lua",
    "tools/wowapi/wowapi/widgets.lua",
    "tools/wowapi/wowapi/xml.lua",
    "tools/addon-shots/editmode_string.lua",
    "tools/addon-shots/fixtures.lua",
    "tools/addon-shots/login_scene.lua",
    "tools/addon-shots/preview.lua",
    "tools/addon-shots/layouts/centered.lua",
    // UI1: checking a UI document (render_ui). The addon's UI.lua draws it.
    "tools/addon-shots/uicheck.lua",
];

/// A bundled tool file by its path from the repo root.
pub fn tool(path: &str) -> Option<&'static [u8]> {
    TOOLS.iter().find(|(p, _)| *p == path).map(|(_, b)| *b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn lua_files(dir: &str) -> Vec<String> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut out: Vec<String> = std::fs::read_dir(root.join(dir))
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".lua"))
            .map(|n| format!("{dir}/{n}"))
            .collect();
        out.sort();
        out
    }

    #[test]
    fn bundle_has_every_wowapi_file() {
        let bundled: Vec<String> = TOOLS
            .iter()
            .map(|(p, _)| p.to_string())
            .filter(|p| p.starts_with("tools/wowapi/wowapi/"))
            .collect();
        assert_eq!(
            bundled,
            lua_files("tools/wowapi/wowapi"),
            "add the new file to preview::bundle::TOOLS"
        );
    }

    #[test]
    fn bundle_has_the_scene_it_needs() {
        for module in [
            "login_scene",
            "fixtures",
            "editmode_string",
            "preview",
            "uicheck",
        ] {
            assert!(
                tool(&format!("tools/addon-shots/{module}.lua")).is_some(),
                "{module}"
            );
        }
        for layout in lua_files("tools/addon-shots/layouts") {
            assert!(tool(&layout).is_some(), "{layout}");
        }
    }
}
