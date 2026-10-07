// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // An agent client starting us as its MCP server (P2): serve stdio and
    // exit, before Tauri and the single-instance check ever start.
    if std::env::args().nth(1).as_deref() == Some(wow_forever_buddy_lib::agent::FLAG) {
        std::process::exit(wow_forever_buddy_lib::agent::serve_stdio());
    }
    wow_forever_buddy_lib::run()
}
