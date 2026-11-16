//! Do the market rules make money? Replays them on Auctionator's day-by-day history: a rule fires on
//! day d, you buy one at that day's low, you sell it on day d+1.
//!
//! "Buy the low, sell the next day" flatters any rule, so every rule is compared with a control that
//! buys every busy item. The sale is priced two ways:
//! - undercut: 1c under the next day's cheapest (you sell fast; conservative);
//! - patient: the next day's high cheapest (you wait for the floor to clear; optimistic).

use crate::config::Config;

use crate::sources::auctionator::{DayPrice, PriceDb};

/// Which trades count.
#[derive(Debug, Clone, PartialEq)]
pub struct BacktestOptions {
    /// Only busy days: at least this many up.
    pub min_listed: f64,
    /// Buy prices in this range (copper): too cheap isn't worth the time, too dear is out of budget.
    pub min_price: f64,
    pub max_price: f64,
}

impl Default for BacktestOptions {
    fn default() -> Self {
        BacktestOptions { min_listed: 100.0, min_price: 500.0, max_price: 50_000.0 }
    }
}

/// One day of one item, as the rules see it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Day {
    pub low: f64,
    pub high: f64,
    pub qty: f64,
}

impl From<&DayPrice> for Day {
    fn from(d: &DayPrice) -> Day {
        Day { low: d.cheapest(), high: d.high, qty: d.listed.unwrap_or(0.0) }
    }
}

/// A rule: given today and (when there is one) the day before, buy?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// Every busy item: the baseline.
    Control,
    /// A cheap listing under the day's own floor (low at most 80% of the high).
    Stray,
    /// The whole floor fell vs the day before (high at most 75% of yesterday's).
    Dip,
    /// A stray cheap listing while the market isn't falling (high at least 90% of yesterday's).
    StrayFlat,
}

impl Rule {
    pub const ALL: [Rule; 4] = [Rule::Control, Rule::Stray, Rule::Dip, Rule::StrayFlat];

    pub fn name(self) -> &'static str {
        match self {
            Rule::Control => "control",
            Rule::Stray => "stray",
            Rule::Dip => "dip",
            Rule::StrayFlat => "stray+flat",
        }
    }

    pub fn fires(self, today: &Day, yesterday: Option<&Day>) -> bool {
        let stray = today.low <= today.high * 0.8;
        match (self, yesterday) {
            (Rule::Control, _) => true,
            (Rule::Stray, _) => stray,
            (Rule::Dip, Some(y)) => today.high <= y.high * 0.75,
            (Rule::StrayFlat, Some(y)) => stray && today.high >= y.high * 0.9,
            (_, None) => false,
        }
    }
}

/// One simulated trade: bought at `buy`, profit after costs both ways of selling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trade {
    pub buy: f64,
    pub undercut: f64,
    pub patient: f64,
}

/// Every trade each rule would have made, by rule (in `Rule::ALL` order).
pub fn simulate(prices: &PriceDb, opts: &BacktestOptions, cfg: &Config) -> Vec<(Rule, Vec<Trade>)> {
    let mut trades: Vec<(Rule, Vec<Trade>)> = Rule::ALL.iter().map(|&r| (r, Vec::new())).collect();
    let sale = |price: f64| (price * cfg.keep_after_cut).floor() - cfg.deposit as f64;
    for entry in prices.realms.iter().flat_map(|r| r.items.iter()).filter(|e| e.has_history) {
        let days = &entry.days;
        for i in 0..days.len().saturating_sub(1) {
            // Only consecutive days: you sell the day after.
            if days[i + 1].day != days[i].day + 1 {
                continue;
            }
            let (today, next) = (Day::from(&days[i]), Day::from(&days[i + 1]));
            let yesterday = (i > 0).then(|| Day::from(&days[i - 1]));
            if today.qty < opts.min_listed || today.low < opts.min_price || today.low > opts.max_price {
                continue;
            }
            for (rule, list) in trades.iter_mut() {
                if rule.fires(&today, yesterday.as_ref()) {
                    list.push(Trade { buy: today.low, undercut: sale(next.low - 1.0) - today.low, patient: sale(next.high) - today.low });
                }
            }
        }
    }
    trades
}

/// How a rule did, one way of selling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Outcome {
    /// Share of trades that made money.
    pub win_rate: f64,
    /// The median trade's return on what it cost.
    pub median_return: f64,
    /// Total profit over total spent.
    pub return_on_gold: f64,
}

pub fn outcome(trades: &[Trade], profit: impl Fn(&Trade) -> f64) -> Outcome {
    let n = trades.len() as f64;
    let spent: f64 = trades.iter().map(|t| t.buy).sum();
    let wins = trades.iter().filter(|t| profit(t) > 0.0).count() as f64;
    let mut returns: Vec<f64> = trades.iter().map(|t| profit(t) / t.buy).collect();
    returns.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_return = returns.get(returns.len().saturating_sub(1) / 2).copied().unwrap_or(0.0);
    Outcome { win_rate: wins / n, median_return, return_on_gold: trades.iter().map(&profit).sum::<f64>() / spent }
}

fn signed_percent(x: f64) -> String {
    format!("{}{:.1}%", if x > 0.0 { "+" } else { "" }, x * 100.0)
}

fn columns(o: Outcome) -> String {
    format!("{:>4}%  {:>6}  {:>8}", format!("{:.0}", 100.0 * o.win_rate), signed_percent(o.median_return), signed_percent(o.return_on_gold))
}

/// The backtest table.
pub fn report(trades: &[(Rule, Vec<Trade>)], opts: &BacktestOptions) -> String {
    let mut out = vec![
        format!(
            "Busy ({}+ listed), buy price {}s to {}g; one unit per signal.\n",
            opts.min_listed,
            opts.min_price / 100.0,
            opts.max_price / 1e4
        ),
        "rule          trades | undercut: win%  median  return on gold | patient: win%  median  return on gold".into(),
    ];
    for (rule, list) in trades {
        if list.is_empty() {
            out.push(format!("{:<13}      0", rule.name()));
            continue;
        }
        out.push(format!(
            "{:<13} {:>6} |     {}      |    {}",
            rule.name(),
            list.len(),
            columns(outcome(list, |t| t.undercut)),
            columns(outcome(list, |t| t.patient))
        ));
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::auctionator::realm_from_json;
    use serde_json::json;

    fn day(low: f64, high: f64) -> Day {
        Day { low, high, qty: 200.0 }
    }

    #[test]
    fn rules() {
        assert!(Rule::Stray.fires(&day(800.0, 1000.0), None));
        assert!(!Rule::Stray.fires(&day(900.0, 1000.0), None));
        assert!(Rule::Dip.fires(&day(700.0, 750.0), Some(&day(900.0, 1000.0))));
        assert!(!Rule::Dip.fires(&day(700.0, 750.0), None));
        assert!(Rule::StrayFlat.fires(&day(700.0, 950.0), Some(&day(900.0, 1000.0))));
        assert!(!Rule::StrayFlat.fires(&day(700.0, 850.0), Some(&day(900.0, 1000.0))));
    }

    #[test]
    fn trades_buy_today_and_sell_tomorrow() {
        let mut db = PriceDb::default();
        // Day 1: low 800, high 1000 (a stray). Day 2: low 1000, high 1200. Day 4 isn't the day after 2.
        db.realms.push(realm_from_json("R", &json!({"version": 2,
            "1": {"m": 1, "h": {"1": 1000, "2": 1200, "4": 1000}, "l": {"1": 800, "2": 1000, "4": 900}, "a": {"1": 200, "2": 200, "4": 200}}})));
        let trades = simulate(&db, &BacktestOptions::default(), &Config::default());
        let (rule, control) = &trades[0];
        assert_eq!(*rule, Rule::Control);
        // undercut: floor(999 x .95) - 100 - 800 = 49; patient: floor(1200 x .95) - 100 - 800 = 240.
        assert_eq!(control, &[Trade { buy: 800.0, undercut: 49.0, patient: 240.0 }]);
        assert_eq!(trades[1].1.len(), 1); // stray fired
        assert!(trades[2].1.is_empty()); // no day before: no dip
        let text = report(&trades, &BacktestOptions::default());
        assert!(text.starts_with("Busy (100+ listed), buy price 5s to 5g; one unit per signal.\n\nrule"));
        assert!(text.contains("control            1 |      100%   +6.1%     +6.1%      |     100%  +30.0%    +30.0%"));
        assert!(text.ends_with("stray+flat         0"));
    }
}
