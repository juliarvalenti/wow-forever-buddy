//! File-system primitives (spec §3–4).
//!
//! - `read::safe_read` is the only way game files are read.
//! - `atomic::atomic_replace` is the atomic write underneath both app-owned
//!   files and (via the guarded writer in T6) game files.
//! - `relpath::RelPath` is how commands name game files, so they can't escape
//!   the flavor folder.

pub mod atomic;
#[allow(dead_code)] // first callers: install detection (T5) and the backup store (T7)
pub mod read;
#[allow(dead_code)] // first callers: the write gate (T6) and backups (T7)
pub mod relpath;
