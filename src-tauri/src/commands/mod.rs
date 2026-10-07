//! Thin `#[tauri::command]` wrappers, one file per area (spec §8).
//! Commands only translate between Tauri and `AppCore`; logic lives in the core modules.

pub mod addon;
pub mod addons;
pub mod adventures;
pub mod agent;
pub mod ah;
pub mod app;
pub mod approvals;
pub mod backup;
pub mod characters;
pub mod cleanup;
pub mod game;
pub mod goals;
pub mod icons;
pub mod ingest;
pub mod install;
pub mod ledger;
pub mod lists;
pub mod macros;
pub mod notes;
pub mod plans;
pub mod restore;
pub mod secrets;
pub mod sessions;
pub mod settings;
pub mod tidy;
