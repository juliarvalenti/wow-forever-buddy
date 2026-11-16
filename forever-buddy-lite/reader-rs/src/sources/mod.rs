//! Where the data comes from, typed. Each submodule reads one kind of file and returns structs; past
//! this layer nothing looks things up by string key except pass-through display data.
//!
//! Files are found the way the JS reader finds them, from environment variables:
//! - `FBL_FILE`: one ForeverBuddyLite.lua; otherwise every `WTF/Account/*/SavedVariables/ForeverBuddyLite.lua`
//!   under `FBL_WTF` (default: the usual Forever install).
//! - `FBL_AUCTIONATOR`: one Auctionator.lua; otherwise the one next to each addon file.
//! - `QUESTIEDB_DIR`: the QuestieDB addon folder (item names, vendors).
//! - `FBL_HIDDEN`: the hidden-items list (default `~/Documents/FBL Market/hidden.json`).

pub mod addon;
pub mod auctionator;
pub mod hidden;
pub mod links;
pub mod lists;
pub mod questie;

use crate::config::Config;
use crate::strategies::{ItemNames, MarketInputs};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Everything the market analysis reads: Auctionator's prices, the addon's ladders and what your
/// characters hold, QuestieDB's names and vendor lists, and your hidden list.
pub fn market_inputs(env: &Env, cfg: &Config, now: f64) -> Result<MarketInputs, String> {
    let addon = addon::load(env)?;
    let questie = questie::load(&env.questiedb_dir(), cfg.vendor_cutoff);
    let mut known: HashMap<u32, String> = HashMap::new();
    let mut held: HashMap<u32, i64> = HashMap::new();
    let scanned = addon.ah.iter().flat_map(|ah| ah.items.iter().flatten());
    for (_, item) in scanned {
        if let (Some(id), Some(name)) = (item.id, &item.name) {
            known.insert(id as u32, name.clone());
        }
    }
    // What you can sell: bags, bank and mail; not worn, not soulbound.
    for c in addon.characters.values() {
        for row in c.item_rows(addon::Places::ALL) {
            let Some(id) = row.id.filter(|&id| id > 0.0) else { continue };
            if let Some(name) = &row.name {
                known.insert(id as u32, name.clone());
            }
            if row.is_worn() || row.is_bound() {
                continue;
            }
            let count = if row.count != 0.0 { row.count } else { 1.0 };
            *held.entry(id as u32).or_insert(0) += count as i64;
        }
    }
    Ok(MarketInputs {
        prices: auctionator::load(env)?,
        ladders: addon.ladders,
        names: ItemNames { questie: questie.names, known },
        vendor_items: questie.vendor_items,
        hidden: hidden::load(&env.hidden_file()),
        held,
        now,
    })
}

const DEFAULT_GAME_DIR: &str = "C:/Program Files (x86)/World of Warcraft/_forever_";

/// The paths the reader is pointed at; `from_process` reads them from the environment.
#[derive(Debug, Clone, Default)]
pub struct Env {
    pub fbl_file: Option<PathBuf>,
    pub fbl_wtf: Option<PathBuf>,
    pub fbl_auctionator: Option<PathBuf>,
    pub questiedb_dir: Option<PathBuf>,
    pub fbl_hidden: Option<PathBuf>,
}

impl Env {
    pub fn from_process() -> Env {
        let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
        Env {
            fbl_file: var("FBL_FILE"),
            fbl_wtf: var("FBL_WTF"),
            fbl_auctionator: var("FBL_AUCTIONATOR"),
            questiedb_dir: var("QUESTIEDB_DIR"),
            fbl_hidden: var("FBL_HIDDEN"),
        }
    }

    /// Only the addon file, the way the tests point at the fixture.
    pub fn with_file(file: impl Into<PathBuf>) -> Env {
        Env { fbl_file: Some(file.into()), ..Env::default() }
    }

    fn accounts_dir(&self) -> PathBuf {
        let wtf = self.fbl_wtf.clone().unwrap_or_else(|| Path::new(DEFAULT_GAME_DIR).join("WTF"));
        wtf.join("Account")
    }

    pub fn questiedb_dir(&self) -> PathBuf {
        self.questiedb_dir.clone().unwrap_or_else(|| Path::new(DEFAULT_GAME_DIR).join("Interface/AddOns/QuestieDB"))
    }

    pub fn hidden_file(&self) -> PathBuf {
        self.fbl_hidden.clone().unwrap_or_else(|| home_dir().join("Documents").join("FBL Market").join("hidden.json"))
    }

    /// The addon's SavedVariables files: `FBL_FILE`, or one per account that has one.
    pub fn addon_files(&self) -> Vec<PathBuf> {
        if let Some(f) = &self.fbl_file {
            return vec![f.clone()];
        }
        self.per_account("ForeverBuddyLite.lua")
    }

    /// `SavedVariables/<name>` in every account folder that has it.
    fn per_account(&self, name: &str) -> Vec<PathBuf> {
        let Ok(dir) = std::fs::read_dir(self.accounts_dir()) else { return Vec::new() };
        let mut accounts: Vec<PathBuf> = dir.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        accounts.sort();
        accounts.into_iter().map(|a| native(a.join("SavedVariables").join(name))).filter(|f| f.exists()).collect()
    }

    /// Auctionator's SavedVariables: `FBL_AUCTIONATOR`, or the file next to each addon file, or (when
    /// the addon has no files yet) every account's.
    pub fn auctionator_files(&self) -> Vec<PathBuf> {
        if let Some(f) = &self.fbl_auctionator {
            return vec![f.clone()];
        }
        let beside: Vec<PathBuf> =
            self.addon_files().iter().filter_map(|f| f.parent().map(|dir| dir.join("Auctionator.lua"))).filter(|f| f.exists()).collect();
        if beside.is_empty() && self.fbl_file.is_none() {
            return self.per_account("Auctionator.lua");
        }
        beside
    }
}

/// A joined path with the platform's separators throughout (the default game folder is written with
/// forward slashes; Node's path.join normalizes them on Windows, so the paths shown match).
fn native(p: PathBuf) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(p.to_string_lossy().replace('/', "\\"))
    } else {
        p
    }
}

fn home_dir() -> PathBuf {
    std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_default()
}
