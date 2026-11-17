//! Wealth over time: gold on every character, plus what their bags and bank would fetch.
//!
//! Two histories feed it:
//! - gold: the addon logs every change of a character's money with when it happened
//!   (`money_log`, entries `{ t, m }`), so the gold line is exact, even between reloads;
//! - items: `fbl-rs live` values each character's bags and bank at the latest prices on every
//!   rebuild and appends a line to `wealth.jsonl` when anything changed. Item value moves with the
//!   market, so it's a mark-to-market estimate, not money in hand.

use crate::sources::addon::Character;
use crate::sources::auctionator::PriceDb;
use crate::tools::prices::market_value;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

/// One character's wealth now.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CharacterWealth {
    /// "Name-Realm".
    pub key: String,
    pub name: String,
    pub level: Option<f64>,
    pub class: Option<String>,
    /// Copper on hand as of the last save.
    pub gold: i64,
    /// What bags and bank would fetch at the latest prices, copper.
    pub items: i64,
    /// When the character was last saved (unix seconds).
    pub saved_at: Option<f64>,
}

pub fn characters_now(chars: &[Character], prices: &PriceDb) -> Vec<CharacterWealth> {
    chars
        .iter()
        .map(|c| CharacterWealth {
            key: c.key.clone(),
            name: c.identity.name.clone().unwrap_or_else(|| c.key.split('-').next().unwrap_or("").to_string()),
            level: c.identity.level,
            class: c.identity.class.clone(),
            gold: c.identity.money.unwrap_or(0.0) as i64,
            items: market_value(c, prices).round() as i64,
            saved_at: c.saved_at,
        })
        .collect()
}

/// A character's logged money changes, oldest first: `(unix seconds, copper)`.
fn money_log(c: &Character) -> Vec<(f64, i64)> {
    let entries: Vec<&Value> = match c.raw.get("money_log") {
        Some(Value::Array(list)) => list.iter().collect(),
        Some(Value::Object(map)) => map.values().collect(), // a sparse Lua table
        _ => Vec::new(),
    };
    let mut log: Vec<(f64, i64)> =
        entries.into_iter().filter_map(|e| Some((e.get("t")?.as_f64()?, e.get("m")?.as_f64()? as i64))).collect();
    log.sort_by(|a, b| a.0.total_cmp(&b.0));
    log
}

/// Total gold on all characters over time, as `(unix seconds, copper)` steps.
///
/// Each character counts with its first known amount until its log starts (so a character whose
/// log begins later doesn't look like new money), then follows its log. A character with no log
/// counts with its saved gold throughout. The series ends with everyone's saved gold now.
pub fn gold_series(chars: &[Character]) -> Vec<(f64, i64)> {
    let logs: Vec<Vec<(f64, i64)>> = chars.iter().map(money_log).collect();
    let saved: Vec<i64> = chars.iter().map(|c| c.identity.money.unwrap_or(0.0) as i64).collect();
    let mut current: Vec<i64> = logs.iter().zip(&saved).map(|(log, &now)| log.first().map_or(now, |e| e.1)).collect();

    let mut events: Vec<(f64, usize, i64)> =
        logs.iter().enumerate().flat_map(|(i, log)| log.iter().map(move |&(t, m)| (t, i, m))).collect();
    events.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut series: Vec<(f64, i64)> = Vec::new();
    for (t, i, m) in events {
        current[i] = m;
        let total = current.iter().sum();
        match series.last_mut() {
            Some(last) if last.0 == t => last.1 = total,
            _ => series.push((t, total)),
        }
    }
    // Saved gold is the latest word for each character.
    let latest_save = chars.iter().filter_map(|c| c.saved_at).fold(f64::MIN, f64::max);
    let total_now: i64 = saved.iter().sum();
    if series.last().is_none_or(|last| last.1 != total_now) && latest_save > f64::MIN {
        let at = series.last().map_or(latest_save, |last| last.0.max(latest_save));
        series.push((at, total_now));
    }
    series
}

/// One recorded line of `wealth.jsonl`: every character's gold and item value at one moment.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub at: f64,
    /// key -> (gold, items), copper.
    pub chars: BTreeMap<String, (i64, i64)>,
}

impl Snapshot {
    pub fn of(at: f64, now: &[CharacterWealth]) -> Snapshot {
        Snapshot { at, chars: now.iter().map(|c| (c.key.clone(), (c.gold, c.items))).collect() }
    }

    fn to_line(&self) -> String {
        let chars: serde_json::Map<String, Value> =
            self.chars.iter().map(|(k, (g, i))| (k.clone(), json!({ "gold": g, "items": i }))).collect();
        json!({ "at": self.at, "chars": chars }).to_string()
    }

    fn from_line(line: &str) -> Option<Snapshot> {
        let v: Value = serde_json::from_str(line).ok()?;
        let chars = v
            .get("chars")?
            .as_object()?
            .iter()
            .filter_map(|(k, c)| Some((k.clone(), (c.get("gold")?.as_i64()?, c.get("items")?.as_i64()?))))
            .collect();
        Some(Snapshot { at: v.get("at")?.as_f64()?, chars })
    }
}

/// Every snapshot recorded so far (unreadable lines are skipped).
pub fn read_history(file: &Path) -> Vec<Snapshot> {
    std::fs::read_to_string(file).map_or_else(|_| Vec::new(), |text| text.lines().filter_map(Snapshot::from_line).collect())
}

/// Appends a snapshot unless every character's gold and item value are the same as last time.
/// Returns whether it wrote one.
pub fn record(file: &Path, snapshot: &Snapshot) -> std::io::Result<bool> {
    if read_history(file).last().is_some_and(|last| last.chars == snapshot.chars) {
        return Ok(false);
    }
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(file)?;
    writeln!(f, "{}", snapshot.to_line())?;
    Ok(true)
}

/// `/wealth.json`: the characters now, the gold steps, and the recorded gold + items history.
pub fn wealth_json(chars: &[Character], prices: &PriceDb, history_file: &Path) -> Value {
    let now = characters_now(chars, prices);
    let history: Vec<Value> = read_history(history_file)
        .iter()
        .map(|s| {
            let gold: i64 = s.chars.values().map(|c| c.0).sum();
            let items: i64 = s.chars.values().map(|c| c.1).sum();
            json!([s.at, gold, items])
        })
        .collect();
    let gold: Vec<Value> = gold_series(chars).into_iter().map(|(t, g)| json!([t, g])).collect();
    json!({ "characters": now, "gold": gold, "history": history })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::addon::Identity;

    fn character(key: &str, money: f64, saved_at: f64, log: Value) -> Character {
        Character {
            key: key.into(),
            saved_at: Some(saved_at),
            identity: Identity { money: Some(money), ..Default::default() },
            raw: json!({ "money_log": log }),
            ..Default::default()
        }
    }

    #[test]
    fn gold_series_adds_characters_and_steps_on_each_change() {
        let a = character("A-R", 300.0, 50.0, json!([{ "t": 10, "m": 100 }, { "t": 30, "m": 300 }]));
        let b = character("B-R", 1000.0, 40.0, json!([{ "t": 20, "m": 1000 }]));
        // Before B's log starts, B counts with its first amount (1000), not zero.
        assert_eq!(gold_series(&[a, b]), vec![(10.0, 1100), (20.0, 1100), (30.0, 1300)]);
    }

    #[test]
    fn gold_series_without_logs_is_the_saved_gold() {
        let a = character("A-R", 500.0, 50.0, Value::Null);
        assert_eq!(gold_series(&[a]), vec![(50.0, 500)]);
    }

    #[test]
    fn gold_series_ends_with_saved_gold_when_the_log_lags() {
        let a = character("A-R", 700.0, 90.0, json!([{ "t": 10, "m": 100 }]));
        assert_eq!(gold_series(&[a]), vec![(10.0, 100), (90.0, 700)]);
    }

    #[test]
    fn record_skips_unchanged_snapshots() {
        let file = std::env::temp_dir().join(format!("fbl-wealth-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&file);
        let mut s = Snapshot { at: 1.0, chars: BTreeMap::from([("A-R".to_string(), (100, 50))]) };
        assert!(record(&file, &s).unwrap());
        s.at = 2.0;
        assert!(!record(&file, &s).unwrap()); // same amounts
        s.chars.insert("A-R".into(), (120, 50));
        assert!(record(&file, &s).unwrap());
        let back = read_history(&file);
        assert_eq!(back.len(), 2);
        assert_eq!(back[1].chars["A-R"], (120, 50));
        std::fs::remove_file(file).unwrap();
    }
}
