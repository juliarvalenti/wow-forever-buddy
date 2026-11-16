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
