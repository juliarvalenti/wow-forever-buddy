//! Thin `#[tauri::command]` wrappers, one file per area (spec §8).
//! Commands only translate between Tauri and `AppCore`; logic lives in the core modules.

pub mod app;
pub mod backup;
pub mod game;
pub mod ingest;
pub mod install;
pub mod restore;
pub mod secrets;
pub mod sessions;
pub mod settings;
