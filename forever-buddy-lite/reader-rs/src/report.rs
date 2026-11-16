//! The market analysis as text, one section per rule.

use crate::format::{money_short as money, percent_change as pct};

use crate::model::{SellAdvice, StaleReason, Verdict};
use crate::strategies::{Analysis, CheckItem, ItemMarket, Pick, SellPick, Trend};

/// What a verdict says, in a line.
pub fn verdict_text(v: &Verdict) -> String {
    match v {
        Verdict::Gap(g) => format!(
            "buy {} up to {} ({}), relist {}: +{}",
            g.qty,
            money(g.up_to as f64),
            money(g.cost as f64),
            money(g.relist as f64),
            money(g.profit as f64)
        ),
        Verdict::TooBig(g) => format!(
            "gap needs {}: buy {} up to {}, relist {} (+{})",
            money(g.cost as f64),
            g.qty,
            money(g.up_to as f64),
            money(g.relist as f64),
            money(g.profit as f64)
        ),
        Verdict::Tight { floor, wall, under_usual } => {
            let dip = under_usual.map_or(String::new(), |u| format!("; floor {}% under usual, a dip bet at best", (u * 100.0).round()));
            format!("no gap: floor {}, wall of {} at {}{dip}", money(*floor as f64), wall.qty, money(wall.price as f64))
        }
        Verdict::Yours { price } => format!("you're the cheapest at {}", money(*price as f64)),
        Verdict::Empty { held: true, usual } => {
            let usual = usual.map_or(String::new(), |u| format!(" (usual {})", money(u as f64)));
            format!("nobody is selling: list at your price{usual}")
        }
        Verdict::Empty { held: false, .. } => "nothing listed".into(),
        Verdict::Stale(StaleReason::Old(was)) => verdict_text(was),
        Verdict::Stale(StaleReason::BadCapture { floor, scan_now }) => {
            format!("ladder floor {} vs scan {}: doesn't add up", money(*floor as f64), money(*scan_now as f64))
        }
        Verdict::Todo => "search it at the AH".into(),
    }
}

/// Where to relist what you hold, in a line.
pub fn sell_text(s: &SellAdvice) -> String {
    let wait = s.under_usual.map_or(String::new(), |u| format!(" (under usual {}: maybe wait)", money(u as f64)));
    format!("relist yours at {}{wait}", money(s.relist as f64))
}

/// The worklist's five-character kind column.
fn kind_label(v: &Verdict) -> &'static str {
    match v {
        Verdict::Gap(_) => "GAP  ",
        Verdict::TooBig(_) => "BIG  ",
        Verdict::Tight { .. } => "tight",
        Verdict::Yours { .. } => "yours",
        Verdict::Stale(_) => "stale",
        Verdict::Todo => "todo ",
        Verdict::Empty { .. } => "empty",
    }
}

fn change(now: i64, usual: i64) -> String {
    pct(now as f64 / usual as f64 - 1.0)
}

fn buy_line(p: &Pick) -> String {
    let r = &p.item;
    format!(
        "{:<32} now {:>8}  usual {:>8}  {:>5}  profit/each {:>7}  listed {}",
        r.name,
        money(r.now as f64),
        money(r.usual as f64),
        change(r.now, r.usual),
        money(p.profit as f64),
        r.qty
    )
}

fn dip_line(p: &Pick) -> String {
    let r = &p.item;
    format!(
        "{:<32} now {:>8}  usual {:>8}  {:>5}  if it recovers {:>7}  listed {}",
        r.name,
        money(r.now as f64),
        money(r.usual as f64),
        change(r.now, r.usual),
        money(p.profit as f64),
        r.qty
    )
}

fn sell_line(p: &SellPick) -> String {
    let r = &p.item;
    format!(
        "{:<32} you have {:>4}  now {:>8}  usual {:>8}  {:>5}  ~{} after cut",
        r.name,
        r.held,
        money(r.now as f64),
        money(r.usual as f64),
        change(r.now, r.usual),
        money(p.worth as f64)
    )
}

fn trend_line(t: &Trend) -> String {
    let r = &t.item;
    let held = if r.held > 0 { format!("  (you have {})", r.held) } else { String::new() };
    format!("{:<32} price {:>5}  supply {:>5}  now {}{held}", r.name, pct(t.price_change), pct(t.supply_change), money(r.now as f64))
}

fn check_line(c: &CheckItem) -> String {
    let again = if matches!(c.verdict, Verdict::Stale(_)) { "search again; was: " } else { "" };
    let sell = c.sell.as_ref().map_or(String::new(), |s| format!("  | {}", sell_text(s)));
    let why: Vec<&str> = c.why.iter().map(|w| w.label()).collect();
    format!("{} {:<34} {again}{}{sell}  [{}]", kind_label(&c.verdict), c.name, verdict_text(&c.verdict), why.join(", "))
}

fn swing_line(r: &ItemMarket) -> String {
    format!(
        "{:<32} today {:>8} to {:>8}  now {:>8}  listed {}",
        r.name,
        money(r.low as f64),
        money(r.high as f64),
        money(r.now as f64),
        r.qty
    )
}

fn section<T>(out: &mut Vec<String>, title: &str, rows: &[T], top: usize, line: impl Fn(&T) -> String) {
    out.push(title.to_string());
    if rows.is_empty() {
        out.push("  (nothing)".into());
    }
    out.extend(rows.iter().take(top).map(|r| format!("  {}", line(r))));
    out.push(String::new());
}

/// The report: a header, then each rule's top `top` rows.
pub fn report(a: &Analysis, top: usize) -> String {
    let mut out = vec![
        format!(
            "Auctionator through {}: {} items, {} busy ({}+ listed). Budget {} a buy.",
            a.date,
            a.items,
            a.busy,
            a.min_listed,
            money(a.budget as f64)
        ),
        String::new(),
    ];
    section(&mut out, "BUY (a stray cheap listing on a busy market)", &a.buy, top, buy_line);
    section(&mut out, "DIP BETS (whole market down today; bet it comes back)", &a.dips, top, dip_line);
    section(&mut out, "SELL what you hold (above usual today)", &a.sell, top, sell_line);
    section(&mut out, "SQUEEZE (supply down, price up: sellers can ask more)", &a.squeeze, top, trend_line);
    section(&mut out, "FLOOD (supply up, price down: don't hold, buy later)", &a.flood, top, trend_line);
    section(&mut out, "CHECK THESE LADDERS (search each at the AH; verdict from the latest search)", &a.check, top, check_line);
    section(&mut out, "SWINGY (wide range today: buy near the low, list near the high)", &a.swingy, top, swing_line);
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::model::{best_gap, ladder_verdict};
    use crate::sources::addon::parse_ladder;

    #[test]
    fn verdict_lines() {
        let cfg = Config::default();
        let levels = parse_ladder("2100:5;3800:20;3900:40");
        let gap = best_gap(&levels, Some(50_000), &cfg).unwrap();
        assert_eq!(verdict_text(&Verdict::Gap(gap.clone())), "buy 5 up to 21s 00c (1g 5s), relist 37s 99c: +74s 45c");
        assert_eq!(verdict_text(&Verdict::TooBig(gap)), "gap needs 1g 5s: buy 5 up to 21s 00c, relist 37s 99c (+74s 45c)");
        let tight = ladder_verdict(&parse_ladder("1000:3;1010:50"), 50_000, Some(2000), 0, &cfg).verdict;
        assert_eq!(verdict_text(&tight), "no gap: floor 10s 00c, wall of 50 at 10s 10c; floor 50% under usual, a dip bet at best");
        assert_eq!(verdict_text(&Verdict::Empty { held: true, usual: Some(500) }), "nobody is selling: list at your price (usual 5s 00c)");
        assert_eq!(verdict_text(&Verdict::Empty { held: false, usual: Some(500) }), "nothing listed");
        assert_eq!(
            sell_text(&SellAdvice { relist: 989, under_usual: Some(2000) }),
            "relist yours at 9s 89c (under usual 20s 00c: maybe wait)"
        );
    }
}
