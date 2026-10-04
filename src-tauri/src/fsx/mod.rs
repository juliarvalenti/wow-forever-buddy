//! File-system primitives (spec §3–4). Game-file reads and writes get their
//! guarded entry points here in T4; this module currently holds the shared
//! atomic-replace used for the app's own files.

pub mod atomic;
