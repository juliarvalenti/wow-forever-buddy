//! The trade ledger: what each item cost you, what it sold for, and what's left, from the addon's
//! journal of AH mail invoices.
//!
//! Sales are matched to buys oldest first (FIFO): each sold unit takes the cost of the earliest unit
//! still unsold. A sale with no buy left to match (something you looted or crafted) counts as sold
//! "without a buy" and stays out of the profit, which would otherwise look free. A sale's net is its
//! price less the AH's cut; the listing deposit comes back with a sale, so it nets out. What's still
//! held is valued at today's cheapest listing, after the cut, so you can see what selling it now
//! would bring against what you paid.

use crate::sources::addon::JournalEntry;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, VecDeque};

const AH_KEEP: f64 = 0.95;

/// One item's trading.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ItemLedger {
    pub item: String,
    /// Item id, when the name is known to the price data.
    pub id: Option<u32>,
    pub bought: f64,
    /// Copper paid in total.
    pub spent: f64,
    pub sold: f64,
    /// Copper received in total, after the cut.
    pub earned: f64,
    /// Profit on units sold that you'd bought: their net less what they cost.
    pub realized: f64,
    /// Units sold with no buy left to match them.
    pub sold_unmatched: f64,
    /// Units bought and not yet sold, and what they cost.
    pub held: f64,
    pub held_cost: f64,
    /// Today's cheapest listing (copper each), and what the held units would bring at it after the cut.
    pub now: Option<f64>,
    pub held_value: Option<f64>,
    /// When it was last traded (unix seconds).
    pub last: f64,
}

impl ItemLedger {
    pub fn avg_cost(&self) -> Option<f64> {
        (self.bought > 0.0).then(|| self.spent / self.bought)
    }
}

/// Every item's ledger, most recently traded first. `prices`: item name -> (id, cheapest now).
pub fn ledger(entries: &[JournalEntry], prices: &HashMap<String, (u32, f64)>) -> Vec<ItemLedger> {
    let mut by_item: BTreeMap<&str, Vec<&JournalEntry>> = BTreeMap::new();
    for e in entries {
        by_item.entry(e.item.as_str()).or_default().push(e);
    }
    let mut out: Vec<ItemLedger> = by_item
        .into_iter()
        .map(|(item, mut trades)| {
            trades.sort_by(|a, b| a.t.total_cmp(&b.t));
            let mut l = ItemLedger { item: item.to_string(), ..ItemLedger::default() };
            // Unsold units: (count, cost each), oldest first.
            let mut lots: VecDeque<(f64, f64)> = VecDeque::new();
            for t in trades {
                let count = if t.count > 0.0 { t.count } else { 1.0 };
                l.last = l.last.max(t.t);
                if t.kind == "buy" {
                    l.bought += count;
                    l.spent += t.price;
                    lots.push_back((count, t.price / count));
                    continue;
                }
                let net = t.price - t.cut;
                l.sold += count;
                l.earned += net;
                let each = net / count;
                let mut left = count;
                while left > 0.0 {
                    let Some(lot) = lots.front_mut() else {
                        l.sold_unmatched += left;
                        break;
                    };
                    let take = left.min(lot.0);
                    l.realized += take * (each - lot.1);
                    lot.0 -= take;
                    left -= take;
                    if lot.0 <= 0.0 {
                        lots.pop_front();
                    }
                }
            }
            l.held = lots.iter().map(|lot| lot.0).sum();
            l.held_cost = lots.iter().map(|lot| lot.0 * lot.1).sum();
            if let Some(&(id, now)) = prices.get(item) {
                l.id = Some(id);
                if now > 0.0 {
                    l.now = Some(now);
                    l.held_value = Some((l.held * now * AH_KEEP).floor());
                }
            }
            l
        })
        .collect();
    out.sort_by(|a, b| b.last.total_cmp(&a.last));
    out
}

/// `/journal.json`: totals, each item's ledger, and the latest trades.
pub fn journal_json(entries: &[JournalEntry], prices: &HashMap<String, (u32, f64)>) -> Value {
    let items = ledger(entries, prices);
    let sum = |f: fn(&ItemLedger) -> f64| items.iter().map(f).sum::<f64>();
    let held_value: f64 = items.iter().filter_map(|l| l.held_value).sum();
    let held_cost_priced: f64 = items.iter().filter(|l| l.held_value.is_some()).map(|l| l.held_cost).sum();
    let rows: Vec<Value> = items
        .iter()
        .map(|l| {
            let mut v = serde_json::to_value(l).unwrap_or(Value::Null);
            v["avgCost"] = json!(l.avg_cost());
            v
        })
        .collect();
    let recent: Vec<Value> = entries
        .iter()
        .rev()
        .take(60)
        .map(|e| {
            json!({ "t": e.t, "kind": e.kind, "item": e.item, "count": e.count, "price": e.price, "cut": e.cut,
                         "id": prices.get(&e.item).map(|p| p.0) })
        })
        .collect();
    json!({
        "totals": {
            "spent": sum(|l| l.spent),
            "earned": sum(|l| l.earned),
            "realized": sum(|l| l.realized),
            "heldCost": sum(|l| l.held_cost),
            "heldValue": held_value,
            "unrealized": held_value - held_cost_priced,
            "trades": entries.len(),
        },
        "items": rows,
        "recent": recent,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(t: f64, kind: &str, item: &str, count: f64, price: f64, cut: f64) -> JournalEntry {
        JournalEntry { t, kind: kind.into(), item: item.into(), count, price, cut, ..JournalEntry::default() }
    }

    #[test]
    fn sales_take_the_oldest_cost_first() {
        // 4 at 1g15s each, then 2 at 1g; sell 5 at 2g each (5% cut).
        let entries = [
            e(1.0, "buy", "Elixir", 4.0, 46_000.0, 0.0),
            e(2.0, "buy", "Elixir", 2.0, 20_000.0, 0.0),
            e(3.0, "sell", "Elixir", 5.0, 100_000.0, 5_000.0),
        ];
        let prices = HashMap::from([("Elixir".to_string(), (13453, 15_000.0))]);
        let l = &ledger(&entries, &prices)[0];
        assert_eq!((l.bought, l.sold, l.held), (6.0, 5.0, 1.0));
        // Net 19,000 each: 4 x (19,000 - 11,500) + 1 x (19,000 - 10,000).
        assert_eq!(l.realized, 4.0 * 7_500.0 + 9_000.0);
        assert_eq!(l.held_cost, 10_000.0);
        assert_eq!(l.held_value, Some(14_250.0));
        assert_eq!(l.id, Some(13453));
    }

    #[test]
    fn selling_what_you_never_bought_isnt_profit() {
        let entries = [e(1.0, "sell", "Linen Cloth", 20.0, 2_000.0, 100.0)];
        let l = &ledger(&entries, &HashMap::new())[0];
        assert_eq!((l.sold_unmatched, l.realized, l.earned), (20.0, 0.0, 1_900.0));
    }
}
