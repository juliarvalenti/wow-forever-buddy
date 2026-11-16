//! The price model: small pure functions for what an item is usually worth, and what a price ladder
//! says to do.

use crate::config::Config;
use crate::sources::addon::LadderLevel;
use crate::sources::auctionator::DayPrice;
use serde::Serialize;

/// The lower middle value (for an even count, the smaller of the two middle ones), so a short
/// history can't be inflated by one high day. None for no values.
pub fn lower_median(values: &[i64]) -> Option<i64> {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted.get(sorted.len().saturating_sub(1) / 2).copied()
}

/// What an item usually sells for: the median of each day's *high* cheapest price, today left out
/// (today is what we're comparing against). A day's low catches one stray cheap listing (Silk at 29c);
/// its high is what the floor mostly was. With a single day, that day is all there is.
pub fn usual_price(days: &[DayPrice]) -> Option<i64> {
    let highs: Vec<i64> = days.iter().map(|d| d.high as i64).collect();
    let past = if highs.len() > 1 { &highs[..highs.len() - 1] } else { &highs[..] };
    lower_median(past)
}

/// Buying the bottom of a ladder and relisting under the next level up.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Gap {
    /// How many price levels to buy out.
    pub levels: usize,
    /// Items bought.
    pub qty: i64,
    /// What they cost in total.
    pub cost: i64,
    /// The most expensive level bought.
    pub up_to: i64,
    /// The relist price each: 1c under the next level up.
    pub relist: i64,
    /// Profit after the cut and one deposit.
    pub profit: i64,
    /// How many are up at the next level (what you're competing with after relisting).
    pub next: i64,
}

/// The most profitable "buy the cheapest k levels, relist 1c under level k+1", or None.
///
/// Stops at your own listing (buying it back makes no sense) and at the budget. One deposit, since you
/// relist as one stack. The profit must clear `min_profit` and `min_return` of the cost.
pub fn best_gap(levels: &[LadderLevel], budget: Option<i64>, cfg: &Config) -> Option<Gap> {
    let mut best: Option<Gap> = None;
    let (mut qty, mut cost) = (0, 0);
    let last_buyable = levels.len().saturating_sub(1).min(cfg.gap_max_levels);
    for k in 0..last_buyable {
        let level = levels[k];
        if level.mine {
            break;
        }
        qty += level.qty;
        cost += level.price * level.qty;
        if budget.is_some_and(|b| cost > b) {
            break;
        }
        let next = levels[k + 1];
        let relist = next.price - 1;
        let profit = qty * cfg.proceeds(relist) - cost - cfg.deposit;
        let worth_it = profit >= cfg.min_profit && profit as f64 >= cost as f64 * cfg.min_return;
        if worth_it && best.as_ref().is_none_or(|b| profit > b.profit) {
            best = Some(Gap { levels: k + 1, qty, cost, up_to: level.price, relist, profit, next: next.qty });
        }
    }
    best
}

/// Where to relist what you hold: 1c under the first other seller near the wall.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SellAdvice {
    pub relist: i64,
    /// Set when the relist price is well under the usual price: maybe wait instead.
    pub under_usual: Option<i64>,
}

/// What one price ladder says.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Verdict {
    /// A gap within budget: do it.
    Gap(Gap),
    /// A gap, but it costs more than the budget.
    TooBig(Gap),
    /// No gap: the wall is the real price.
    Tight {
        floor: i64,
        wall: LadderLevel,
        /// The floor's discount to usual, when it's well under (a dip bet at best).
        under_usual: Option<f64>,
    },
    /// You're the cheapest listing.
    Yours { price: i64 },
    /// Nothing listed. With some held, you can list at your price.
    Empty { held: bool, usual: Option<i64> },
    /// The search wants redoing.
    Stale(StaleReason),
    /// Not searched yet.
    Todo,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StaleReason {
    /// The search is older than `ladder_stale_hours`; what it said then.
    Old(Box<Verdict>),
    /// The ladder's floor is far under the price scan's cheapest: a bad capture, or the scan is stale.
    BadCapture { floor: i64, scan_now: i64 },
}

impl Verdict {
    /// Worklist order: gaps first, then things to search, then the rest.
    pub fn rank(&self) -> u8 {
        match self {
            Verdict::Gap(_) => 0,
            Verdict::Todo => 1,
            Verdict::Stale(_) => 2,
            Verdict::TooBig(_) => 3,
            Verdict::Yours { .. } => 4,
            Verdict::Tight { .. } => 5,
            Verdict::Empty { .. } => 6,
        }
    }

    /// The gap behind this verdict, if any (a stale search keeps the gap it found).
    pub fn gap(&self) -> Option<&Gap> {
        match self {
            Verdict::Gap(g) | Verdict::TooBig(g) => Some(g),
            Verdict::Stale(StaleReason::Old(v)) => v.gap(),
            _ => None,
        }
    }
}

/// What a ladder says, plus where to relist when you hold some.
#[derive(Debug, Clone, PartialEq)]
pub struct LadderReading {
    pub verdict: Verdict,
    pub sell: Option<SellAdvice>,
}

/// The biggest of the cheapest few levels: where most supply sits, the price buyers actually pay.
pub fn wall(levels: &[LadderLevel], cfg: &Config) -> Option<LadderLevel> {
    // On a tie the cheaper (earlier) level wins.
    levels.iter().take(cfg.wall_levels).copied().reduce(|a, b| if b.qty > a.qty { b } else { a })
}

/// Reads a ladder (cheapest first) into one verdict. `held`: how many you have to sell; `usual`: the
/// item's usual price, when known.
pub fn ladder_verdict(levels: &[LadderLevel], budget: i64, usual: Option<i64>, held: i64, cfg: &Config) -> LadderReading {
    let usual = usual.filter(|&u| u != 0);
    let Some(wall) = wall(levels, cfg) else {
        return LadderReading { verdict: Verdict::Empty { held: held > 0, usual }, sell: None };
    };
    let sell = if held > 0 { sell_advice(levels, wall, usual, cfg) } else { None };
    let verdict = if levels[0].mine {
        Verdict::Yours { price: levels[0].price }
    } else if let Some(g) = best_gap(levels, Some(budget), cfg) {
        Verdict::Gap(g)
    } else if let Some(g) = best_gap(levels, None, cfg) {
        Verdict::TooBig(g)
    } else {
        let floor = levels[0].price;
        let under_usual = usual.filter(|&u| floor as f64 <= u as f64 * cfg.under_usual_note).map(|u| 1.0 - floor as f64 / u as f64);
        Verdict::Tight { floor, wall, under_usual }
    };
    LadderReading { verdict, sell }
}

/// Relist 1c under the first other seller priced near the wall.
fn sell_advice(levels: &[LadderLevel], wall: LadderLevel, usual: Option<i64>, cfg: &Config) -> Option<SellAdvice> {
    let first_other = levels.iter().find(|l| !l.mine && l.price as f64 >= wall.price as f64 * cfg.wall_near)?;
    let relist = first_other.price - 1;
    let under_usual = usual.filter(|&u| (relist as f64) < u as f64 * cfg.under_usual_note);
    Some(SellAdvice { relist, under_usual })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level(price: i64, qty: i64) -> LadderLevel {
        LadderLevel { price, qty, mine: false }
    }

    fn day(day: i64, high: f64) -> DayPrice {
        DayPrice { day, high, low: None, listed: None }
    }

    #[test]
    fn lower_median_takes_the_smaller_middle() {
        assert_eq!(lower_median(&[3, 1, 2]), Some(2));
        assert_eq!(lower_median(&[4, 1, 3, 2]), Some(2));
        assert_eq!(lower_median(&[7]), Some(7));
        assert_eq!(lower_median(&[]), None);
    }

    #[test]
    fn usual_leaves_today_out() {
        // Today's 100 doesn't count: median of 200, 210, 190.
        assert_eq!(usual_price(&[day(1, 200.0), day(2, 210.0), day(3, 190.0), day(4, 100.0)]), Some(200));
        assert_eq!(usual_price(&[day(1, 300.0)]), Some(300));
        assert_eq!(usual_price(&[]), None);
    }

    #[test]
    fn gap_buys_the_cheap_level_and_relists_under_the_next() {
        // 21s x5, 38s x20, 39s x40: buy the 5 at 21s (1g 5s), relist at 37s 99c.
        // 5 x floor(3799 x 0.95) = 5 x 3609 = 18045, less 10500 cost and a 1s deposit: 7445.
        let levels = [level(2100, 5), level(3800, 20), level(3900, 40)];
        let g = best_gap(&levels, Some(50_000), &Config::default()).unwrap();
        assert_eq!(g, Gap { levels: 1, qty: 5, cost: 10_500, up_to: 2100, relist: 3799, profit: 7445, next: 20 });
    }

    #[test]
    fn gap_needs_min_profit_and_min_return() {
        let cfg = Config::default();
        // 1 at 1000, next 1300: floor(1299 x .95) = 1234 - 1000 - 100 = 134 < 2s.
        assert_eq!(best_gap(&[level(1000, 1), level(1300, 1)], None, &cfg), None);
        // 10 at 10000, next 10500: 10 x 9974 - 100000 - 100 = -360: no.
        assert_eq!(best_gap(&[level(10_000, 10), level(10_500, 5)], None, &cfg), None);
        // 2s profit but under 15% of cost: 1 at 5000, next 5600 -> 5319 - 5100 = 219 < 750.
        assert_eq!(best_gap(&[level(5000, 1), level(5600, 1)], None, &cfg), None);
    }

    #[test]
    fn gap_stops_at_your_listing_and_the_budget() {
        let cfg = Config::default();
        let mut mine = level(2100, 5);
        mine.mine = true;
        assert_eq!(best_gap(&[mine, level(3800, 20)], None, &cfg), None);
        let levels = [level(2100, 5), level(3800, 20), level(3900, 40)];
        assert_eq!(best_gap(&levels, Some(10_000), &cfg), None); // 1g 5s is over a 1g budget
        assert_eq!(best_gap(&[level(2100, 5)], None, &cfg), None); // nothing to relist under
    }

    #[test]
    fn gap_considers_at_most_six_levels() {
        let cfg = Config::default();
        // Six cheap levels of 1 at 100, then one at 10000: buying all seven would be needed.
        let mut levels: Vec<LadderLevel> = (0..7).map(|_| level(100, 1)).collect();
        levels.push(level(10_000, 1));
        assert_eq!(best_gap(&levels, None, &cfg), None);
        levels.remove(0);
        assert_eq!(best_gap(&levels, None, &cfg).unwrap().levels, 6);
    }

    #[test]
    fn verdict_gap_or_too_big() {
        let cfg = Config::default();
        let levels = [level(2100, 5), level(3800, 20), level(3900, 40)];
        let r = ladder_verdict(&levels, 50_000, None, 0, &cfg);
        assert_eq!(r.verdict.gap().unwrap().profit, 7445);
        assert!(matches!(r.verdict, Verdict::Gap(_)));
        let r = ladder_verdict(&levels, 10_000, None, 0, &cfg);
        assert!(matches!(r.verdict, Verdict::TooBig(Gap { profit: 7445, .. })));
        assert_eq!(r.sell, None);
    }

    #[test]
    fn verdict_tight_reports_the_wall_and_a_dip() {
        let cfg = Config::default();
        let levels = [level(1000, 3), level(1010, 50), level(1020, 10)];
        let r = ladder_verdict(&levels, 50_000, Some(2000), 0, &cfg);
        assert_eq!(r.verdict, Verdict::Tight { floor: 1000, wall: level(1010, 50), under_usual: Some(0.5) });
        let r = ladder_verdict(&levels, 50_000, Some(1100), 0, &cfg);
        assert_eq!(r.verdict, Verdict::Tight { floor: 1000, wall: level(1010, 50), under_usual: None });
    }

    #[test]
    fn verdict_yours_and_empty() {
        let cfg = Config::default();
        let mut mine = level(1000, 3);
        mine.mine = true;
        assert_eq!(ladder_verdict(&[mine, level(2000, 5)], 50_000, None, 0, &cfg).verdict, Verdict::Yours { price: 1000 });
        assert_eq!(ladder_verdict(&[], 50_000, Some(500), 2, &cfg).verdict, Verdict::Empty { held: true, usual: Some(500) });
        assert_eq!(ladder_verdict(&[], 50_000, None, 0, &cfg).verdict, Verdict::Empty { held: false, usual: None });
    }

    #[test]
    fn sell_advice_undercuts_the_wall_not_a_stray() {
        let cfg = Config::default();
        // A stray at 500 (buy that instead); the wall is 40 at 1000: relist at 999.
        let levels = [level(500, 1), level(990, 2), level(1000, 40)];
        let r = ladder_verdict(&levels, 50_000, Some(1000), 3, &cfg);
        assert_eq!(r.sell, Some(SellAdvice { relist: 989, under_usual: None }));
        // Well under usual: say so.
        let r = ladder_verdict(&levels, 50_000, Some(2000), 3, &cfg);
        assert_eq!(r.sell, Some(SellAdvice { relist: 989, under_usual: Some(2000) }));
        // Your own listing isn't the one to undercut.
        let mut mine = level(990, 2);
        mine.mine = true;
        let r = ladder_verdict(&[level(500, 1), mine, level(1000, 40)], 50_000, None, 3, &cfg);
        assert_eq!(r.sell.unwrap().relist, 999);
    }

    #[test]
    fn worklist_rank_and_stale_gaps() {
        let g = Gap { levels: 1, qty: 1, cost: 1, up_to: 1, relist: 1, profit: 9, next: 1 };
        let stale = Verdict::Stale(StaleReason::Old(Box::new(Verdict::Gap(g.clone()))));
        assert_eq!(stale.gap(), Some(&g));
        // Verdicts serialize for `market --json`, a stale one with what it said before.
        let json = serde_json::to_value(&stale).unwrap();
        assert_eq!(json["kind"], "stale");
        assert_eq!(json["old"]["kind"], "gap");
        let bad = Verdict::Stale(StaleReason::BadCapture { floor: 1, scan_now: 9 });
        assert_eq!(serde_json::to_value(&bad).unwrap()["bad_capture"]["scan_now"], 9);
        let mut kinds = [Verdict::Todo, Verdict::Gap(g), Verdict::Empty { held: false, usual: None }, stale];
        kinds.sort_by_key(Verdict::rank);
        assert!(matches!(kinds[0], Verdict::Gap(_)));
        assert!(matches!(kinds[3], Verdict::Empty { .. }));
    }
}
