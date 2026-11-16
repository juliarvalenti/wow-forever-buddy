//! Every market threshold in one place. Prices are in copper (100 copper = 1 silver, 100 silver = 1 gold).

/// Thresholds for the market analysis. `Config::default()` is what the JS reader used.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    // ------------------------------------------------------------ costs of trading
    /// What you keep of a sale: the auction house takes a 5% cut.
    pub keep_after_cut: f64,
    /// The deposit for one listing (lost if it doesn't sell, so counted as a cost): 1s.
    pub deposit: i64,

    // ------------------------------------------------------------ what's worth doing
    /// The least profit a trade must make after costs: 2s. Below that it isn't worth the time.
    pub min_profit: i64,
    /// A ladder gap must also return this share of what you spend (15%): a 2s profit on a 5g buy is
    /// too thin a margin for the risk that the market moves.
    pub min_return: f64,
    /// The most you'd spend on one buy, by default: 5g.
    pub budget: i64,

    // ------------------------------------------------------------ which markets count
    /// A market is "busy" with at least this many up today: enough buyers that a resale sells.
    pub min_listed: i64,
    /// ...and at least this many days of history, so "usual" means something.
    pub min_days: usize,
    /// Items sold by this many vendors or more are unlimited goods: never suggested.
    pub vendor_cutoff: usize,

    // ------------------------------------------------------------ rules
    /// Buy: the cheapest now is at most this share of usual (20% under).
    pub cheap_factor: f64,
    /// Dips: the whole market is down. The cheapest now at most 75% of usual...
    pub dip_factor: f64,
    /// ...and even today's high (the floor most of the day) at most 90% of usual...
    pub dip_high_factor: f64,
    /// ...with at least this many days of history.
    pub dip_min_days: usize,
    /// Sell: what you hold is listed at least 15% over its usual today...
    pub sell_factor: f64,
    /// ...on a market with at least this many up (someone is trading it).
    pub sell_min_listed: i64,
    /// Squeeze: supply fell at least 20% since the first day...
    pub squeeze_supply: f64,
    /// ...while the floor rose at least 15%.
    pub squeeze_price: f64,
    /// Flood: supply rose at least 30%...
    pub flood_supply: f64,
    /// ...while the floor fell at least 15%.
    pub flood_price: f64,
    /// Swingy: today's high at least 1.5x today's low...
    pub swing_ratio: f64,
    /// ...a spread of at least 3s...
    pub swing_min_spread: i64,
    /// ...and the high no more than twice the usual (a high that far up is a mispriced listing, not a range).
    pub swing_max_over_usual: f64,

    // ------------------------------------------------------------ price ladders
    /// A gap buys at most the cheapest this many levels.
    pub gap_max_levels: usize,
    /// The "wall" (the real price) is the biggest of the cheapest this many levels.
    pub wall_levels: usize,
    /// Relist under the first level at least 90% of the wall's price, so a stray cheap listing
    /// (buy that instead) doesn't set your price.
    pub wall_near: f64,
    /// When a relist price or the floor is this far under usual (80%), say so.
    pub under_usual_note: f64,
    /// Gaps only count from searches this recent (hours): the ladder moves.
    pub gap_fresh_hours: f64,
    /// A searched ladder older than this (hours) wants a fresh search.
    pub ladder_stale_hours: f64,
    /// Items searched this recently (hours) join the worklist even when no rule picked them.
    pub searched_recent_hours: f64,
    /// A ladder whose floor is under this share of the price scan's cheapest is a bad capture.
    pub bad_capture_factor: f64,
    /// Levels kept per ladder for the item view.
    pub ladder_view_levels: usize,

    // ------------------------------------------------------------ the worklist
    /// How many of each rule's picks go on the "check these ladders" list.
    pub check_cheap: usize,
    pub check_held: usize,
    pub check_swingy: usize,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            keep_after_cut: 0.95,
            deposit: 100,
            min_profit: 200,
            min_return: 0.15,
            budget: 50_000,
            min_listed: 100,
            min_days: 2,
            vendor_cutoff: 15,
            cheap_factor: 0.8,
            dip_factor: 0.75,
            dip_high_factor: 0.9,
            dip_min_days: 3,
            sell_factor: 1.15,
            sell_min_listed: 20,
            squeeze_supply: -0.2,
            squeeze_price: 0.15,
            flood_supply: 0.3,
            flood_price: -0.15,
            swing_ratio: 1.5,
            swing_min_spread: 300,
            swing_max_over_usual: 2.0,
            gap_max_levels: 6,
            wall_levels: 6,
            wall_near: 0.9,
            under_usual_note: 0.8,
            gap_fresh_hours: 3.0,
            ladder_stale_hours: 2.0,
            searched_recent_hours: 6.0,
            bad_capture_factor: 0.3,
            ladder_view_levels: 25,
            check_cheap: 12,
            check_held: 6,
            check_swingy: 6,
        }
    }
}

impl Config {
    /// What one item sold at `price` brings in after the cut (copper rounds down).
    pub fn proceeds(&self, price: i64) -> i64 {
        (price as f64 * self.keep_after_cut).floor() as i64
    }
}
