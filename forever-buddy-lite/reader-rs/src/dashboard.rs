//! The JSON the web dashboard (forever-buddy-lite/reader/web) reads from `/data.json`:
//! `{ items, opps, stamp }`. This module is the contract with web/lib/*.js: field names here are
//! the ones the components use.

use crate::catalog::Catalog;
use crate::model::{Gap, StaleReason, Verdict};
use crate::report::{sell_text, verdict_text};
use crate::strategies::{Analysis, CheckItem, ItemMarket, LadderView};
use serde_json::{json, Map, Value};

/// A market row: the item's numbers (`qty0`/`low0` are the first day's supply and low).
fn row(r: &ItemMarket) -> Map<String, Value> {
    let v = json!({
        "id": r.id, "name": r.name, "now": r.now, "usual": r.usual, "days": r.days, "low": r.low, "high": r.high,
        "qty": r.qty, "qty0": r.first_qty, "low0": r.first_low, "held": r.held,
    });
    match v {
        Value::Object(o) => o,
        _ => unreachable!(),
    }
}

fn with(mut base: Map<String, Value>, extra: Value) -> Value {
    if let Value::Object(e) = extra {
        base.extend(e);
    }
    Value::Object(base)
}

fn gap(g: &Gap) -> Value {
    json!({ "levels": g.levels, "qty": g.qty, "cost": g.cost, "upTo": g.up_to, "relist": g.relist, "profit": g.profit, "next": g.next })
}

/// The kind name the web app groups by (web/lib/round.js VERDICT).
pub fn kind_name(v: &Verdict) -> &'static str {
    match v {
        Verdict::Gap(_) => "gap",
        Verdict::TooBig(_) => "big",
        Verdict::Tight { .. } => "tight",
        Verdict::Yours { .. } => "yours",
        Verdict::Empty { .. } => "empty",
        Verdict::Stale(_) => "stale",
        Verdict::Todo => "todo",
    }
}

fn check_item(c: &CheckItem) -> Value {
    let mut o = json!({
        "id": c.id, "name": c.name, "why": c.why.iter().map(|w| w.label()).collect::<Vec<_>>(),
        "usual": c.usual, "now": c.now, "held": c.held, "at": c.at,
        "kind": kind_name(&c.verdict), "text": verdict_text(&c.verdict),
    });
    // What the ladder itself said, when there was one to read (a stale search keeps its old verdict).
    let said = match &c.verdict {
        Verdict::Todo | Verdict::Stale(StaleReason::BadCapture { .. }) => None,
        Verdict::Stale(StaleReason::Old(was)) => Some(was.as_ref()),
        v => Some(v),
    };
    if let Some(v) = said {
        o["verdict"] = json!(kind_name(v));
    }
    if let Some(g) = c.verdict.gap() {
        o["gap"] = gap(g);
    }
    if let Some(s) = &c.sell {
        o["sell"] = json!({ "relist": s.relist, "text": sell_text(s) });
    }
    o
}

fn ladder(l: &LadderView) -> Value {
    json!({ "at": l.at, "total": l.total, "levels": l.levels })
}

/// The analysis as the dashboard reads it.
pub fn opps_json(a: &Analysis) -> Value {
    let rows = |list: &[ItemMarket]| list.iter().map(|r| Value::Object(row(r))).collect::<Vec<_>>();
    json!({
        "date": a.date, "items": a.items, "busy": a.busy, "budget": a.budget, "minListed": a.min_listed,
        "skippedVendor": a.skipped_vendor,
        "hidden": a.hidden.iter().map(|(id, name)| json!({ "id": id, "name": name })).collect::<Vec<_>>(),
        "scanAt": a.scan_at,
        "buy": a.buy.iter().map(|p| with(row(&p.item), json!({ "profit": p.profit }))).collect::<Vec<_>>(),
        "dips": a.dips.iter().map(|p| with(row(&p.item), json!({ "profit": p.profit }))).collect::<Vec<_>>(),
        "sell": a.sell.iter().map(|p| with(row(&p.item), json!({ "worth": p.worth }))).collect::<Vec<_>>(),
        "squeeze": a.squeeze.iter().map(|t| with(row(&t.item), json!({ "dq": t.supply_change, "dp": t.price_change }))).collect::<Vec<_>>(),
        "flood": a.flood.iter().map(|t| with(row(&t.item), json!({ "dq": t.supply_change, "dp": t.price_change }))).collect::<Vec<_>>(),
        "swingy": rows(&a.swingy),
        "gaps": a.gaps.iter().map(|g| with(match gap(&g.gap) { Value::Object(o) => o, _ => unreachable!() },
            json!({ "id": g.id, "key": g.key, "name": g.name, "at": g.at, "total": g.total, "usual": g.usual }))).collect::<Vec<_>>(),
        "check": a.check.iter().map(check_item).collect::<Vec<_>>(),
        "ladders": a.ladders.iter().map(|(id, l)| (id.to_string(), ladder(l))).collect::<Map<_, _>>(),
    })
}

/// `/data.json`: the item catalog, the analysis, and the last day the scans cover.
pub fn data_json(catalog: &Catalog, a: &Analysis) -> Value {
    json!({ "items": catalog.items, "opps": opps_json(a), "stamp": catalog.stamp })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::sources::addon::LadderSnapshot;
    use crate::sources::auctionator::realm_from_json;
    use crate::strategies::{analyze, MarketInputs};

    /// The field names web/lib and web/components read.
    #[test]
    fn opps_carry_the_fields_the_web_app_reads() {
        let now = 1_000_000.0;
        let mut inputs = MarketInputs { now, ..MarketInputs::default() };
        let busy = json!({"m": 5000, "h": {"2508": 10_000, "2509": 10_000, "2510": 10_000}, "a": {"2508": 200, "2509": 200, "2510": 200}});
        inputs.prices.realms.push(realm_from_json("Realm", &json!({"version": 2, "10": busy})));
        inputs
            .ladders
            .insert("10".into(), vec![LadderSnapshot { at: Some(now - 60.0), total: Some(25.0), levels: "5000:5;9000:20".into() }]);
        let opps = opps_json(&analyze(&inputs, 50_000, &Config::default()));
        let top = [
            "date",
            "items",
            "busy",
            "budget",
            "minListed",
            "skippedVendor",
            "hidden",
            "scanAt",
            "buy",
            "dips",
            "sell",
            "squeeze",
            "flood",
            "swingy",
            "gaps",
            "check",
            "ladders",
        ];
        for key in top {
            assert!(opps.get(key).is_some(), "{key}");
        }
        for key in ["id", "name", "now", "usual", "qty", "held", "low", "high", "profit"] {
            assert!(opps["buy"][0].get(key).is_some(), "buy.{key}");
        }
        let check = &opps["check"][0];
        assert_eq!(check["kind"], "gap");
        assert_eq!(check["verdict"], "gap");
        assert_eq!(check["why"], json!(["cheap now"]));
        assert_eq!(check["gap"]["upTo"], 5000);
        assert!(check["text"].as_str().unwrap().starts_with("buy 5 up to"));
        assert_eq!(opps["ladders"]["10"]["levels"][1], json!({"price": 9000, "qty": 20, "mine": false}));
        assert_eq!(opps["gaps"][0]["key"], "10");
    }
}
