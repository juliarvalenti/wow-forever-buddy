//! The running game: detecting WoW (spec §2) and the write gate that keeps
//! every game-file change safe (spec §4).

#[allow(dead_code)]
// wired up once the backup store provides the snapshot (T7); first caller is restore (T9)
pub mod gate;
pub mod process;
