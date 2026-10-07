//! Thin `#[tauri::command]` wrappers, one file per area (spec §8).
//! Commands only translate between Tauri and `AppCore`; logic lives in the core modules.

pub mod addon;
pub mod addons;
pub mod adventures;
pub mod ah;
pub mod app;
pub mod backup;
pub mod characters;
pub mod game;
pub mod icons;
pub mod ingest;
pub mod install;
pub mod ledger;
pub mod macros;
pub mod notes;
pub mod restore;
pub mod secrets;
pub mod sessions;
pub mod settings;
