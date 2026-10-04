use std::path::Path;

fn main() {
    // Tauri would embed its manifest as a resource in the app binary only; we
    // embed it ourselves below so test binaries get it too.
    let windows = tauri_build::WindowsAttributes::new_without_app_manifest();
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("failed to run tauri-build");
    embed_windows_manifest();
}

/// Tauri needs the Common-Controls v6 manifest (comctl32 v6). If it's only in
/// the app binary, `cargo test` binaries crash on Windows at load time with
/// STATUS_ENTRYPOINT_NOT_FOUND (0xc0000139). Passing it to the linker puts it
/// in every binary, tests included.
fn embed_windows_manifest() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os != "windows" || target_env != "msvc" {
        return;
    }
    let manifest = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("windows-app-manifest.xml");
    println!("cargo:rerun-if-changed=windows-app-manifest.xml");
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
}
