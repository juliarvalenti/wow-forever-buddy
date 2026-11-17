//! Opportunities in the latest Auctionator scan: one function per rule, and `analyze` to run them all.
//!
//! Every rule works on [`ItemMarket`] rows: per item, today's cheapest ("now"), its usual price, and
//! today's and the first day's lows and supply. Costs are the 5% cut and a 1s deposit (see [`Config`]).

use crate::config::Config;
use crate::model::{self, Gap, LadderReading, SellAdvice, StaleReason, Verdict};
use crate::sources::addon::{parse_ladder, LadderLevel, LadderSnapshot};
use crate::sources::auctionator::{PriceDb, PriceEntry};
use crate::sources::hidden::HiddenItem;
use crate::sources::lists::OrderedMap;
use serde::Serialize;
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};

/// What the analysis reads (see `sources::market_inputs`).
#[derive(Debug, Default)]
pub struct MarketInputs {
    pub prices: PriceDb,
    /// Price ladders by item key, oldest search first.
    pub ladders: OrderedMap<Vec<LadderSnapshot>>,
    pub names: ItemNames,
    /// Unlimited vendor goods: left out.
    pub vendor_items: HashSet<u32>,
    /// Items you've hidden: left out.
    pub hidden: BTreeMap<u32, HiddenItem>,
    /// How many of each item your characters can sell.
    pub held: HashMap<u32, i64>,
    /// The time to judge ladder ages by (seconds since 1970).
    pub now: f64,
}

/// Item names: QuestieDB's first, then names seen in the gear scan and your characters' items.
#[derive(Debug, Default, Clone)]
pub struct ItemNames {
    pub questie: HashMap<u32, String>,
    pub known: HashMap<u32, String>,
}

impl ItemNames {
    pub fn of(&self, id: u32) -> String {
        self.questie.get(&id).or_else(|| self.known.get(&id)).cloned().unwrap_or_else(|| format!("item {id}"))
    }
}

/// One item's market today.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ItemMarket {
    pub id: u32,
    pub name: String,
    /// The cheapest listing in the latest scan.
    pub now: i64,
    /// What it usually sells for (see `model::usual_price`).
    pub usual: i64,
    /// Days of history.
    pub days: usize,
    /// Today's cheapest at its lowest and highest scans.
    pub low: i64,
    pub high: i64,
    /// How many are up today, and on the first day of history.
    pub qty: i64,
    pub first_qty: i64,
    /// The first day's low.
    pub first_low: i64,
    /// How many you hold.
    pub held: i64,
}

impl ItemMarket {
    /// Enough sellers and history to trade on.
    pub fn is_busy(&self, cfg: &Config) -> bool {
        self.qty >= cfg.min_listed && self.days >= cfg.min_days
    }
}

/// A buy, with the profit after costs on one item.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Pick {
    #[serde(flatten)]
    pub item: ItemMarket,
    pub profit: i64,
}

/// Something you hold that sells well today.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SellPick {
    #[serde(flatten)]
    pub item: ItemMarket,
    /// What all you hold would bring after the cut, at today's price.
    pub worth: i64,
}

/// How supply and price moved since the first day of history (as fractions: -0.2 is 20% down).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Trend {
    #[serde(flatten)]
    pub item: ItemMarket,
    pub supply_change: f64,
    pub price_change: f64,
}

/// A gap from a fresh ladder.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GapPick {
    #[serde(flatten)]
    pub gap: Gap,
    pub id: u32,
    pub key: String,
    pub name: String,
    /// When the ladder was searched, and how many were up.
    pub at: f64,
    pub total: Option<f64>,
    pub usual: Option<i64>,
}

/// Why an item is on the worklist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Reason {
    CheapNow,
    Swingy,
    Searched,
}

impl Reason {
    pub fn label(self) -> &'static str {
        match self {
            Reason::CheapNow => "cheap now",
            Reason::Swingy => "swingy",
            Reason::Searched => "searched",
        }
    }
}

/// An item to search at the auction house, with what its latest ladder said.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CheckItem {
    pub id: u32,
    pub name: String,
    pub why: Vec<Reason>,
    pub usual: Option<i64>,
    pub now: Option<i64>,
    pub held: i64,
    /// When the ladder was searched.
    pub at: Option<f64>,
    pub verdict: Verdict,
    pub sell: Option<SellAdvice>,
}

/// The sorted sections the worklist draws its picks from.
#[derive(Debug, Clone, Default)]
pub struct RulePicks {
    pub buy: Vec<Pick>,
    pub sell: Vec<SellPick>,
    pub swingy: Vec<ItemMarket>,
}

/// The latest ladder for one item.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LadderView {
    pub at: f64,
    pub total: Option<f64>,
    pub levels: Vec<LadderLevel>,
}

/// Everything `analyze` finds.
#[derive(Debug, Clone, Serialize)]
pub struct Analysis {
    /// The latest day with prices ("2026-11-14").
    pub date: String,
    pub items: usize,
    pub busy: usize,
    pub budget: i64,
    pub min_listed: i64,
    pub skipped_vendor: usize,
    pub hidden: Vec<(u32, String)>,
    pub scan_at: Option<f64>,
    pub buy: Vec<Pick>,
    pub dips: Vec<Pick>,
    pub sell: Vec<SellPick>,
    pub squeeze: Vec<Trend>,
    pub flood: Vec<Trend>,
    pub swingy: Vec<ItemMarket>,
    pub gaps: Vec<GapPick>,
    pub check: Vec<CheckItem>,
    pub ladders: BTreeMap<u32, LadderView>,
}

// ---------------------------------------------------------------- rows

/// One row per traded item: skips items with no price now, no history, hidden ones and unlimited
/// vendor goods. Returns the rows, the latest day seen, and how many vendor goods were skipped.
pub fn market_rows(inputs: &MarketInputs) -> (Vec<ItemMarket>, i64, usize) {
    let mut rows = Vec::new();
    let (mut last_day, mut skipped_vendor) = (0, 0);
    for realm in &inputs.prices.realms {
        for entry in &realm.items {
            let Some(id) = entry.id() else { continue };
            if !entry.has_history || entry.now.unwrap_or(0.0) == 0.0 || inputs.hidden.contains_key(&id) {
                continue;
            }
            // Sold by vendors: unlimited at a fixed price, so the auction house can't be traded.
            if inputs.vendor_items.contains(&id) {
                skipped_vendor += 1;
                continue;
            }
            let Some(row) = market_row(id, entry, inputs) else { continue };
            last_day = last_day.max(entry.days.last().map_or(0, |d| d.day));
            rows.push(row);
        }
    }
    (rows, last_day, skipped_vendor)
}

fn market_row(id: u32, entry: &PriceEntry, inputs: &MarketInputs) -> Option<ItemMarket> {
    let (first, today) = (entry.days.first()?, entry.days.last()?);
    Some(ItemMarket {
        id,
        name: inputs.names.of(id),
        now: entry.now? as i64,
        usual: model::usual_price(&entry.days)?,
        days: entry.days.len(),
        low: today.cheapest() as i64,
        high: today.high as i64,
        qty: today.listed.unwrap_or(0.0) as i64,
        first_qty: first.listed.unwrap_or(0.0) as i64,
        first_low: first.cheapest() as i64,
        held: inputs.held.get(&id).copied().unwrap_or(0),
    })
}

// ---------------------------------------------------------------- rules

/// Sort key for "best first, but a busier market counts for more": profit (or spread) times the
/// order of magnitude of supply, so 3s on a market of 1000 beats 4s on a market of 20.
fn busy_weighted(value: i64, qty: i64) -> f64 {
    value as f64 * (qty as f64).log10()
}

fn descending(a: f64, b: f64) -> Ordering {
    b.partial_cmp(&a).unwrap_or(Ordering::Equal)
}

/// Buy: a stray cheap listing on a busy market. The cheapest now is well under usual, and you'd
/// resell at the usual price or today's high, whichever is lower (today's high shows the market
/// didn't move: the cheap one is a stray, not a new price). Best first.
pub fn buy(rows: &[ItemMarket], budget: i64, cfg: &Config) -> Vec<Pick> {
    let mut picks: Vec<Pick> = rows
        .iter()
        .filter(|r| r.is_busy(cfg))
        .map(|r| Pick { item: r.clone(), profit: cfg.proceeds(r.usual.min(r.high)) - cfg.deposit - r.now })
        .filter(|p| {
            let r = &p.item;
            r.now as f64 <= r.usual as f64 * cfg.cheap_factor && p.profit >= cfg.min_profit && r.now <= budget
        })
        .collect();
    picks.sort_by(|a, b| descending(busy_weighted(a.profit, a.item.qty), busy_weighted(b.profit, b.item.qty)));
    picks
}

/// Dip bets: the whole market is down today (even its high is under usual), not one stray listing.
/// A bet that it comes back to usual, not a sure profit.
pub fn dips(rows: &[ItemMarket], budget: i64, cfg: &Config) -> Vec<Pick> {
    let mut picks: Vec<Pick> = rows
        .iter()
        .filter(|r| {
            r.is_busy(cfg)
                && r.days >= cfg.dip_min_days
                && r.now as f64 <= r.usual as f64 * cfg.dip_factor
                && r.high as f64 <= r.usual as f64 * cfg.dip_high_factor
                && r.now <= budget
        })
        .map(|r| Pick { item: r.clone(), profit: cfg.proceeds(r.usual) - cfg.deposit - r.now })
        .filter(|p| p.profit >= cfg.min_profit)
        .collect();
    picks.sort_by(|a, b| descending(busy_weighted(a.profit, a.item.qty), busy_weighted(b.profit, b.item.qty)));
    picks
}

/// Whether you hold some of an item that lists well over its usual today, on a market people trade.
fn sells_high(r: &ItemMarket, cfg: &Config) -> bool {
    r.held > 0 && r.days >= cfg.min_days && r.now as f64 >= r.usual as f64 * cfg.sell_factor && r.qty >= cfg.sell_min_listed
}

/// Sell: things you hold that list above their usual today, most valuable first.
pub fn sell(rows: &[ItemMarket], cfg: &Config) -> Vec<SellPick> {
    let mut picks: Vec<SellPick> =
        rows.iter().filter(|r| sells_high(r, cfg)).map(|r| SellPick { item: r.clone(), worth: cfg.proceeds(r.now) * r.held }).collect();
    picks.sort_by_key(|p| std::cmp::Reverse(p.worth));
    picks
}

/// Supply and price change since the first day, for busy items that had both then.
fn trends(rows: &[ItemMarket], cfg: &Config) -> Vec<Trend> {
    rows.iter()
        .filter(|r| r.is_busy(cfg) && r.first_qty > 0 && r.first_low > 0)
        .map(|r| Trend {
            item: r.clone(),
            supply_change: r.qty as f64 / r.first_qty as f64 - 1.0,
            price_change: r.low as f64 / r.first_low as f64 - 1.0,
        })
        .collect()
}

/// Squeeze: supply falling while the price rises. Someone's buying up; sellers can ask more.
pub fn squeeze(rows: &[ItemMarket], cfg: &Config) -> Vec<Trend> {
    let mut picks: Vec<Trend> =
        trends(rows, cfg).into_iter().filter(|t| t.supply_change <= cfg.squeeze_supply && t.price_change >= cfg.squeeze_price).collect();
    picks.sort_by(|a, b| descending(a.price_change - a.supply_change, b.price_change - b.supply_change));
    picks
}

/// Flood: supply rising while the price falls. Don't hold these; buy later.
pub fn flood(rows: &[ItemMarket], cfg: &Config) -> Vec<Trend> {
    let mut picks: Vec<Trend> =
        trends(rows, cfg).into_iter().filter(|t| t.supply_change >= cfg.flood_supply && t.price_change <= cfg.flood_price).collect();
    picks.sort_by(|a, b| descending(b.price_change - b.supply_change, a.price_change - a.supply_change));
    picks
}

/// Whether today's range is wide enough to buy near the low and list near the high, on a busy market,
/// with the high still a believable price (not a mispriced listing far over usual).
fn swings(r: &ItemMarket, budget: i64, cfg: &Config) -> bool {
    r.is_busy(cfg)
        && r.high as f64 >= r.low as f64 * cfg.swing_ratio
        && r.high - r.low >= cfg.swing_min_spread
        && r.low <= budget
        && r.high as f64 <= r.usual as f64 * cfg.swing_max_over_usual
}

/// Swingy: the widest daily ranges on busy markets, widest first.
pub fn swingy(rows: &[ItemMarket], budget: i64, cfg: &Config) -> Vec<ItemMarket> {
    let mut picks: Vec<ItemMarket> = rows.iter().filter(|r| swings(r, budget, cfg)).cloned().collect();
    picks.sort_by(|a, b| descending(busy_weighted(a.high - a.low, a.qty), busy_weighted(b.high - b.low, b.qty)));
    picks
}

/// The item id in a ladder key ("7357:1205" -> 7357).
fn ladder_id(key: &str) -> Option<u32> {
    key.split(':').next()?.parse().ok()
}

/// Gaps from fresh ladders (searched within `gap_fresh_hours`), most profit first.
pub fn gaps(inputs: &MarketInputs, rows: &[ItemMarket], budget: i64, cfg: &Config) -> Vec<GapPick> {
    let usual: HashMap<u32, i64> = rows.iter().map(|r| (r.id, r.usual)).collect();
    let mut picks = Vec::new();
    for (key, log) in inputs.ladders.iter() {
        let Some(last) = log.last() else { continue };
        let Some(at) = last.at.filter(|&at| at != 0.0) else { continue };
        if inputs.now - at > cfg.gap_fresh_hours * 3600.0 {
            continue;
        }
        let Some(gap) = model::best_gap(&parse_ladder(&last.levels), Some(budget), cfg) else { continue };
        let Some(id) = ladder_id(key) else { continue };
        picks.push(GapPick { gap, id, key: key.clone(), name: inputs.names.of(id), at, total: last.total, usual: usual.get(&id).copied() });
    }
    picks.sort_by_key(|p| std::cmp::Reverse(p.gap.profit));
    picks
}

/// The latest ladder per item id (an item can have several keys, one per suffix).
pub fn latest_ladders(inputs: &MarketInputs, cfg: &Config) -> BTreeMap<u32, LadderView> {
    let mut out: BTreeMap<u32, LadderView> = BTreeMap::new();
    for (key, log) in inputs.ladders.iter() {
        let Some(last) = log.last() else { continue };
        let (Some(at), Some(id)) = (last.at.filter(|&at| at != 0.0), ladder_id(key)) else { continue };
        if out.get(&id).is_none_or(|old| at > old.at) {
            let mut levels = parse_ladder(&last.levels);
            levels.truncate(cfg.ladder_view_levels);
            out.insert(id, LadderView { at, total: last.total, levels });
        }
    }
    out
}

/// The worklist: what to search at the auction house today, and what each searched ladder says.
///
/// Items come from the rules (the best few of buy and swingy) and from recent searches. Each gets
/// the verdict of its latest ladder; one older than `ladder_stale_hours` is marked stale, and so is
/// one whose floor doesn't match the price scan.
///
/// Items you hold are left off: the worklist is for deciding what to buy, and a held item there
/// either nagged for a fresh search or showed "buy more" for what you were selling. The sell
/// section lists them, and each item's page keeps its ladder and where to relist.
pub fn check_list(
    inputs: &MarketInputs,
    rows: &[ItemMarket],
    picks: &RulePicks,
    ladders: &BTreeMap<u32, LadderView>,
    budget: i64,
    cfg: &Config,
) -> Vec<CheckItem> {
    // (id, name, reasons), in the order first wanted.
    let mut wanted: Vec<(u32, String, Vec<Reason>)> = Vec::new();
    fn want(wanted: &mut Vec<(u32, String, Vec<Reason>)>, id: u32, name: &str, why: Reason) {
        match wanted.iter_mut().find(|(i, _, _)| *i == id) {
            Some((_, _, reasons)) if reasons.contains(&why) => {}
            Some((_, _, reasons)) => reasons.push(why),
            None => wanted.push((id, name.to_string(), vec![why])),
        }
    }
    for p in picks.buy.iter().take(cfg.check_cheap) {
        want(&mut wanted, p.item.id, &p.item.name, Reason::CheapNow);
    }
    for r in picks.swingy.iter().take(cfg.check_swingy) {
        want(&mut wanted, r.id, &r.name, Reason::Swingy);
    }
    let by_id: HashMap<u32, &ItemMarket> = rows.iter().map(|r| (r.id, r)).collect();
    for (&id, ladder) in ladders {
        let recent = inputs.now - ladder.at < cfg.searched_recent_hours * 3600.0;
        let excluded = inputs.hidden.contains_key(&id) || inputs.vendor_items.contains(&id);
        if recent && !excluded && !wanted.iter().any(|(i, _, _)| *i == id) {
            let name = by_id.get(&id).map_or_else(|| inputs.names.of(id), |r| r.name.clone());
            want(&mut wanted, id, &name, Reason::Searched);
        }
    }
    let mut items: Vec<CheckItem> = wanted
        .into_iter()
        .filter_map(|(id, name, why)| {
            let row = by_id.get(&id).copied();
            let ladder = ladders.get(&id);
            if row.is_some_and(|r| r.held > 0) {
                return None;
            }
            let reading = read_ladder(row, ladder, inputs.now, budget, cfg);
            Some(CheckItem {
                id,
                name,
                why,
                usual: row.map(|r| r.usual),
                now: row.map(|r| r.now),
                held: row.map_or(0, |r| r.held),
                at: ladder.map(|l| l.at),
                verdict: reading.verdict,
                sell: reading.sell,
            })
        })
        .collect();
    let profit = |c: &CheckItem| c.verdict.gap().map_or(0, |g| g.profit);
    items.sort_by(|a, b| a.verdict.rank().cmp(&b.verdict.rank()).then(profit(b).cmp(&profit(a))));
    items
}

/// The verdict for one worklist item.
fn read_ladder(row: Option<&ItemMarket>, ladder: Option<&LadderView>, now: f64, budget: i64, cfg: &Config) -> LadderReading {
    let Some(ladder) = ladder else { return LadderReading { verdict: Verdict::Todo, sell: None } };
    // A floor far under the price scan's cheapest is a bad capture (or the scan is stale).
    if let (Some(r), Some(floor)) = (row, ladder.levels.first()) {
        if (floor.price as f64) < r.now as f64 * cfg.bad_capture_factor {
            let verdict = Verdict::Stale(StaleReason::BadCapture { floor: floor.price, scan_now: r.now });
            return LadderReading { verdict, sell: None };
        }
    }
    let reading = model::ladder_verdict(&ladder.levels, budget, row.map(|r| r.usual), row.map_or(0, |r| r.held), cfg);
    let old = now - ladder.at > cfg.ladder_stale_hours * 3600.0;
    if old && !matches!(reading.verdict, Verdict::Empty { .. }) {
        let verdict = Verdict::Stale(StaleReason::Old(Box::new(reading.verdict)));
        return LadderReading { verdict, sell: reading.sell };
    }
    reading
}

/// Runs every rule. `budget`: the most to spend on one buy, in copper.
pub fn analyze(inputs: &MarketInputs, budget: i64, cfg: &Config) -> Analysis {
    let (rows, last_day, skipped_vendor) = market_rows(inputs);
    let ladders = latest_ladders(inputs, cfg);
    let picks = RulePicks { buy: buy(&rows, budget, cfg), sell: sell(&rows, cfg), swingy: swingy(&rows, budget, cfg) };
    let check = check_list(inputs, &rows, &picks, &ladders, budget, cfg);
    Analysis {
        date: crate::format::auctionator_date(last_day),
        items: rows.len(),
        busy: rows.iter().filter(|r| r.is_busy(cfg)).count(),
        budget,
        min_listed: cfg.min_listed,
        skipped_vendor,
        hidden: inputs
            .hidden
            .iter()
            .map(|(&id, h)| (id, h.name.clone().filter(|n| !n.is_empty()).unwrap_or_else(|| inputs.names.of(id))))
            .collect(),
        scan_at: inputs.prices.scan_at,
        dips: dips(&rows, budget, cfg),
        squeeze: squeeze(&rows, cfg),
        flood: flood(&rows, cfg),
        gaps: gaps(inputs, &rows, budget, cfg),
        buy: picks.buy,
        sell: picks.sell,
        swingy: picks.swingy,
        check,
        ladders,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::auctionator::realm_from_json;
    use serde_json::json;

    fn row(id: u32, now: i64, usual: i64, low: i64, high: i64, qty: i64) -> ItemMarket {
        ItemMarket { id, name: format!("item {id}"), now, usual, days: 5, low, high, qty, first_qty: qty, first_low: low, held: 0 }
    }

    #[test]
    fn buy_wants_a_stray_under_a_steady_high() {
        let cfg = Config::default();
        // Usual 10s, today's high 10s, one at 5s: resell at 10s -> 9500 - 100 - 5000 = 4400.
        let picks = buy(&[row(1, 5000, 10_000, 5000, 10_000, 200)], 50_000, &cfg);
        assert_eq!(picks.len(), 1);
        assert_eq!(picks[0].profit, 4400);
        // The whole market fell (high 6s): resell only at 6s -> 5700 - 100 - 5000 = 600: still in.
        assert_eq!(buy(&[row(1, 5000, 10_000, 5000, 6000, 200)], 50_000, &cfg)[0].profit, 600);
        // Not cheap enough (over 80% of usual), quiet market, over budget: out.
        assert!(buy(&[row(1, 8500, 10_000, 8500, 10_000, 200)], 50_000, &cfg).is_empty());
        assert!(buy(&[row(1, 5000, 10_000, 5000, 10_000, 99)], 50_000, &cfg).is_empty());
        assert!(buy(&[row(1, 5000, 10_000, 5000, 10_000, 200)], 4999, &cfg).is_empty());
    }

    #[test]
    fn buy_ranks_profit_by_market_size() {
        let cfg = Config::default();
        let small = row(1, 5000, 10_000, 5000, 10_000, 100); // 4400 x 2
        let big = row(2, 6000, 10_000, 6000, 10_000, 10_000); // 3400 x 4
        let picks = buy(&[small, big], 50_000, &cfg);
        assert_eq!(picks.iter().map(|p| p.item.id).collect::<Vec<_>>(), [2, 1]);
    }

    #[test]
    fn dips_need_the_whole_market_down() {
        let cfg = Config::default();
        assert_eq!(dips(&[row(1, 7000, 10_000, 7000, 8000, 200)], 50_000, &cfg)[0].profit, 9500 - 100 - 7000);
        assert!(dips(&[row(1, 7000, 10_000, 7000, 9500, 200)], 50_000, &cfg).is_empty()); // high near usual: a stray
        let mut young = row(1, 7000, 10_000, 7000, 8000, 200);
        young.days = 2;
        assert!(dips(&[young], 50_000, &cfg).is_empty());
    }

    #[test]
    fn sell_what_you_hold_above_usual() {
        let cfg = Config::default();
        let mut r = row(1, 12_000, 10_000, 12_000, 12_000, 20);
        r.held = 3;
        assert_eq!(sell(&[r.clone()], &cfg)[0].worth, 11_400 * 3);
        r.now = 11_000; // only 10% over
        assert!(sell(&[r], &cfg).is_empty());
    }

    #[test]
    fn squeeze_and_flood() {
        let cfg = Config::default();
        let mut squeezed = row(1, 1200, 1000, 1200, 1200, 150);
        (squeezed.first_qty, squeezed.first_low) = (300, 1000); // supply -50%, price +20%
        let mut flooded = row(2, 800, 1000, 800, 800, 400);
        (flooded.first_qty, flooded.first_low) = (200, 1000); // supply +100%, price -20%
        let rows = [squeezed, flooded];
        assert_eq!(squeeze(&rows, &cfg).iter().map(|t| t.item.id).collect::<Vec<_>>(), [1]);
        let f = flood(&rows, &cfg);
        assert_eq!(f.iter().map(|t| t.item.id).collect::<Vec<_>>(), [2]);
        assert!((f[0].supply_change - 1.0).abs() < 1e-9);
    }

    #[test]
    fn swingy_wants_a_wide_believable_range() {
        let cfg = Config::default();
        assert_eq!(swingy(&[row(1, 600, 1000, 600, 1000, 200)], 50_000, &cfg).len(), 1);
        assert!(swingy(&[row(1, 600, 1000, 600, 800, 200)], 50_000, &cfg).is_empty()); // under 1.5x
        assert!(swingy(&[row(1, 100, 1000, 100, 2500, 200)], 50_000, &cfg).is_empty()); // high over 2x usual
        assert!(swingy(&[row(1, 100, 300, 100, 350, 200)], 50_000, &cfg).is_empty());
        // spread under 3s
    }

    fn inputs_with(prices: serde_json::Value, ladders: &[(&str, f64, &str)], now: f64) -> MarketInputs {
        let mut inputs = MarketInputs { now, ..MarketInputs::default() };
        inputs.prices.realms.push(realm_from_json("Realm", &prices));
        for (key, at, levels) in ladders {
            inputs.ladders.insert(key.to_string(), vec![LadderSnapshot { at: Some(*at), total: None, levels: levels.to_string() }]);
        }
        inputs
    }

    #[test]
    fn rows_skip_hidden_vendor_and_unpriced_items() {
        let day = |h: i64| json!({"2509": h, "2510": h});
        let mut inputs = inputs_with(
            json!({"version": 2, "1": {"m": 100, "h": day(100)}, "2": {"m": 100, "h": day(100)}, "3": {"m": 100, "h": day(100)}, "4": {"h": day(100)}, "5": {"m": 100}}),
            &[],
            0.0,
        );
        inputs.hidden.insert(2, HiddenItem::default());
        inputs.vendor_items.insert(3);
        let (rows, last_day, skipped) = market_rows(&inputs);
        assert_eq!(rows.iter().map(|r| r.id).collect::<Vec<_>>(), [1]);
        assert_eq!((last_day, skipped), (2510, 1));
    }

    #[test]
    fn worklist_from_rules_and_searches() {
        let now = 1_000_000.0;
        // Item 10: cheap now (5s vs usual 10s), with a fresh gap ladder. Item 20: searched 1h ago, tight.
        // Item 30: cheap now, never searched. Item 40: searched 5h ago: stale.
        let busy = |m: i64| json!({"m": m, "h": {"2509": 10_000, "2510": 10_000}, "a": {"2509": 200, "2510": 200}});
        let inputs = inputs_with(
            json!({"version": 2, "10": busy(5000), "30": busy(5000)}),
            &[
                ("10", now - 600.0, "5000:5;9000:20"),
                ("20", now - 3600.0, "1000:3;1010:50"),
                ("40:7", now - 5.0 * 3600.0, "2100:5;3800:20;3900:40"),
            ],
            now,
        );
        let a = analyze(&inputs, 50_000, &Config::default());
        let kinds: Vec<(u32, u8)> = a.check.iter().map(|c| (c.id, c.verdict.rank())).collect();
        assert_eq!(kinds, [(10, 0), (30, 1), (40, 2), (20, 5)]);
        assert_eq!(a.check[0].why, [Reason::CheapNow]);
        assert_eq!(a.check[2].why, [Reason::Searched]);
        assert_eq!(a.check[2].verdict.gap().unwrap().profit, 7445); // stale, but remembers its gap
        assert_eq!(a.gaps.iter().map(|g| g.id).collect::<Vec<_>>(), [10]); // 40's ladder is too old for gaps
        assert_eq!(a.ladders[&40].levels.len(), 3);
        assert_eq!(a.date, "2026-11-15");
    }

    #[test]
    fn the_worklist_takes_each_rules_best_picks() {
        // Two cheap items; item 2 pays more. With room for one pick, item 2 makes the list, not item 1.
        let busy = |m: i64| json!({"m": m, "h": {"2509": 10_000, "2510": 10_000}, "a": {"2509": 200, "2510": 200}});
        let inputs = inputs_with(json!({"version": 2, "1": busy(7900), "2": busy(5000)}), &[], 0.0);
        let cfg = Config { check_cheap: 1, ..Config::default() };
        let a = analyze(&inputs, 50_000, &cfg);
        assert_eq!(a.buy.iter().map(|p| p.item.id).collect::<Vec<_>>(), [2, 1]);
        assert_eq!(a.check.iter().map(|c| c.id).collect::<Vec<_>>(), [2]);
    }

    #[test]
    fn what_you_hold_stays_off_the_worklist() {
        let now = 1_000_000.0;
        // All cheap now and held. Item 1 searched 30 minutes ago, item 2 five hours ago, item 3 never.
        // Item 4: cheap now, not held: the only one listed.
        let busy = |m: i64| json!({"m": m, "h": {"2509": 10_000, "2510": 10_000}, "a": {"2509": 200, "2510": 200}});
        let mut inputs = inputs_with(
            json!({"version": 2, "1": busy(5000), "2": busy(5000), "3": busy(5000), "4": busy(5000)}),
            &[("1", now - 1800.0, "5000:5;9000:20"), ("2", now - 5.0 * 3600.0, "5000:5;9000:20")],
            now,
        );
        for id in [1, 2, 3] {
            inputs.held.insert(id, 4);
        }
        let a = analyze(&inputs, 50_000, &Config::default());
        assert_eq!(a.check.iter().map(|c| c.id).collect::<Vec<_>>(), [4]);
        assert_eq!(a.ladders[&1].levels.len(), 2); // the item page still has its ladder
    }

    #[test]
    fn a_ladder_far_under_the_scan_is_a_bad_capture() {
        let cfg = Config::default();
        let r = row(1, 10_000, 10_000, 10_000, 10_000, 200);
        let ladder = LadderView { at: 0.0, total: None, levels: parse_ladder("2000:5;9000:9") };
        let reading = read_ladder(Some(&r), Some(&ladder), 0.0, 50_000, &cfg);
        assert_eq!(reading.verdict, Verdict::Stale(StaleReason::BadCapture { floor: 2000, scan_now: 10_000 }));
    }
}
