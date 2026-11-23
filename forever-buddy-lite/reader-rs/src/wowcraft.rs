//! Helpers for wowcraft.io, a fan-made profession leveling planner with a "Forever" mode
//! (`fbl-rs wowcraft`). Two things it publishes are useful next to our own planner:
//!
//! - Its Forever recipe data (skill colours, reagents with item ids, where the recipe comes from,
//!   trainer cost), bundled in the planner page's script as `JSON.parse('[...]')` arrays, one per
//!   profession, plus a list of recipe ids the site leaves out. We read the same arrays the page
//!   does and apply the same filters.
//! - `/api/profession-costs`: its estimated cost to level a profession to 300 on a realm and
//!   faction, as a history. A benchmark for `fbl-rs skillup`.
//!
//! Only these commands touch the network, only when run, and they cache what they fetch under
//! Documents/FBL Market/wowcraft. Requests say they come from fbl-rs (no browser user agent).

use regex::Regex;
use serde_json::{Map, Value};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

const BASE: &str = "https://wowcraft.io";
const USER_AGENT: &str = concat!("fbl-rs/", env!("CARGO_PKG_VERSION"), " (personal profession planner)");
pub const PROFESSIONS: [&str; 7] = ["Alchemy", "Blacksmithing", "Cooking", "Enchanting", "Engineering", "Leatherworking", "Tailoring"];
pub const REALMS: [&str; 8] = [
    "Forever PvE US",
    "Forever PvE EU",
    "Forever PvP US",
    "Forever PvP EU",
    "Forever RP US",
    "Forever RP EU",
    "Forever RP-PvP US",
    "Forever RP-PvP EU",
];

/// "tailoring" -> "Tailoring", or an error listing the choices.
pub fn profession_name(input: &str) -> Result<&'static str, String> {
    PROFESSIONS
        .iter()
        .find(|p| p.eq_ignore_ascii_case(input.trim()))
        .copied()
        .ok_or_else(|| format!("\"{input}\" isn't a wowcraft profession: {}.", PROFESSIONS.join(", ")))
}

pub fn cache_dir() -> PathBuf {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_default();
    home.join("Documents").join("FBL Market").join("wowcraft")
}

fn get(url: &str) -> Result<String, String> {
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(30)).user_agent(USER_AGENT).build();
    agent.get(url).call().map_err(|e| format!("wowcraft.io: {e}"))?.into_string().map_err(|e| format!("wowcraft.io: {e}"))
}

/// A cached file if it's younger than `max_age`.
fn fresh(path: &PathBuf, max_age: Duration) -> Option<String> {
    let age = std::fs::metadata(path).ok()?.modified().ok().and_then(|m| SystemTime::now().duration_since(m).ok())?;
    if age > max_age {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

fn save(path: &PathBuf, text: &str) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, text);
}

/// The body of a JS single-quoted string literal, unescaped (no eval).
pub fn unescape_js(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                out.extend(u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32));
            }
            Some('x') => {
                let hex: String = chars.by_ref().take(2).collect();
                out.extend(u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32));
            }
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

/// The text of the string literal starting at `start` (just after its opening quote), up to the
/// matching unescaped `quote`.
fn literal_at(js: &str, start: usize, quote: char) -> Option<&str> {
    let bytes = js.as_bytes();
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b if b == quote as u8 => return Some(&js[start..i]),
            _ => i += 1,
        }
    }
    None
}

/// `<name>=JSON.parse('...')` (or with double quotes) parsed as JSON.
fn json_var(js: &str, name: &str) -> Result<Value, String> {
    let re = Regex::new(&format!(r#"(?:^|[^\w$]){}=JSON\.parse\((['"])"#, regex::escape(name))).map_err(|e| e.to_string())?;
    for m in re.captures_iter(js) {
        let whole = m.get(0).expect("match");
        let quote = m[1].chars().next().unwrap_or('\'');
        if let Some(body) = literal_at(js, whole.end(), quote) {
            if let Ok(v) = serde_json::from_str::<Value>(&unescape_js(body)) {
                return Ok(v);
            }
        }
    }
    Err(format!("couldn't find the data `{name}` in wowcraft's page (the site may have changed)"))
}

/// The Forever recipes per profession from the planner page's script, filtered the way the page
/// filters them (left-out ids, Blacksmithing's "Hard Gold" entries).
pub fn extract_recipes(js: &str) -> Result<Map<String, Value>, String> {
    let changed = "wowcraft's page layout changed: `fbl-rs wowcraft` needs updating";
    let map_re = Regex::new(r"Forever:\{(Alchemy:[^}]*(?:\}[^}]*)*?)\}\}").map_err(|e| e.to_string())?;
    let body = map_re.captures(js).map(|c| c[1].to_string()).ok_or(changed)?;
    // Left-out recipe ids: `let X=new Set(C);function F(e){return e.filter(e=>!X.has(e.id))}`.
    let set_re = Regex::new(r"=new Set\(([\w$]+)\);function ([\w$]+)\(e\)\{return e\.filter\(").map_err(|e| e.to_string())?;
    let (excluded, filter_fn) = match set_re.captures(js) {
        Some(c) => {
            let ids: Vec<i64> = json_var(js, &c[1])?.as_array().map(|a| a.iter().filter_map(Value::as_i64).collect()).unwrap_or_default();
            (ids, c[2].to_string())
        }
        None => (Vec::new(), String::new()),
    };
    let entry_re = Regex::new(r"([A-Za-z]+):(?:([\w$]+)\(([\w$]+)\)|([\w$]+)|\[\])").map_err(|e| e.to_string())?;
    let names = item_names(js);
    let mut out = Map::new();
    for c in entry_re.captures_iter(&body) {
        let profession = c[1].to_string();
        if !PROFESSIONS.contains(&profession.as_str()) {
            continue;
        }
        let (var, filtered) = match (c.get(2), c.get(3), c.get(4)) {
            (Some(f), Some(v), _) => (v.as_str(), f.as_str() == filter_fn),
            (_, _, Some(v)) => (v.as_str(), false),
            _ => {
                out.insert(profession, Value::Array(Vec::new()));
                continue;
            }
        };
        let recipes = json_var(js, var)?;
        let list: Vec<Value> = recipes
            .as_array()
            .ok_or(changed)?
            .iter()
            .filter(|r| !(filtered && r["id"].as_i64().is_some_and(|id| excluded.contains(&id))))
            .filter(|r| !(profession == "Blacksmithing" && r["name"].as_str().is_some_and(|n| n.starts_with("Hard Gold"))))
            .cloned()
            .map(|r| with_names(r, &names))
            .collect();
        out.insert(profession, Value::Array(list));
    }
    if out.is_empty() {
        return Err(changed.to_string());
    }
    Ok(out)
}

/// Item names by id from anywhere in the page's data (reagent lists and crafted items of every
/// game version): the Forever recipes list reagents by id only.
fn item_names(js: &str) -> std::collections::HashMap<String, String> {
    let mut names = std::collections::HashMap::new();
    let res = [r#"\{"id":(\d+),"name":"((?:[^"\\]|\\.)+)","quantity""#, r#""produces":\{"id":(\d+),"name":"((?:[^"\\]|\\.)+)""#];
    for re in res.iter().filter_map(|p| Regex::new(p).ok()) {
        for c in re.captures_iter(js) {
            names.entry(c[1].to_string()).or_insert_with(|| unescape_js(&c[2]));
        }
    }
    names
}

/// Adds `inputItems` (names from `names`) when a recipe only has `materials`, and `skill`: the
/// level it's learnt at (`minSkill`, or its lowest colour when that's missing).
fn with_names(mut r: Value, names: &std::collections::HashMap<String, String>) -> Value {
    if r["inputItems"].is_null() {
        if let Some(m) = r["materials"].as_object() {
            let list: Vec<Value> = m
                .iter()
                .map(|(id, n)| {
                    let name = names.get(id).cloned().unwrap_or_else(|| format!("item {id}"));
                    serde_json::json!({ "id": id.parse::<i64>().unwrap_or(0), "name": name, "quantity": n })
                })
                .collect();
            r["inputItems"] = Value::Array(list);
        }
    }
    let lowest_colour = ["orange", "yellow", "green", "gray"].iter().filter_map(|k| r["difficulty"][k].as_f64()).find(|v| *v > 0.0);
    let skill = r["minSkill"].as_f64().filter(|v| *v > 0.0).or(lowest_colour).unwrap_or(0.0);
    r["skill"] = crate::json::number(skill);
    r
}

/// All Forever recipes, from the cache when it's under a day old.
pub fn recipes(refresh: bool) -> Result<Map<String, Value>, String> {
    let path = cache_dir().join("recipes.json");
    if !refresh {
        if let Some(Ok(Value::Object(m))) = fresh(&path, Duration::from_secs(86400)).map(|t| serde_json::from_str(&t)) {
            return Ok(m);
        }
    }
    let html = get(&format!("{BASE}/engineering"))?;
    let chunk_re = Regex::new(r#"/_next/static/chunks/app/%5Bprofession%5D/page-[0-9a-f]+\.js[^"' >]*"#).map_err(|e| e.to_string())?;
    let chunk = chunk_re.find(&html).ok_or("couldn't find wowcraft's planner script (the site may have changed)")?;
    let js = get(&format!("{BASE}{}", chunk.as_str()))?;
    let all = extract_recipes(&js)?;
    save(&path, &serde_json::to_string(&all).unwrap_or_default());
    Ok(all)
}

fn colours(r: &Value) -> String {
    let d = &r["difficulty"];
    let n = |k: &str| d[k].as_f64().map_or("-".to_string(), |v| format!("{v}"));
    format!("{}/{}/{}/{}", n("orange"), n("yellow"), n("green"), n("gray"))
}

fn source(r: &Value) -> String {
    let s = &r["source"];
    match s["type"].as_str() {
        Some("trainer") => match s["cost"].as_f64() {
            Some(c) if c > 0.0 => format!("trainer {}", crate::format::money_long(c)),
            _ => "trainer".to_string(),
        },
        Some("item") => format!("recipe: {}", s["recipeItemName"].as_str().unwrap_or("?")),
        Some(other) => other.to_string(),
        None => "?".to_string(),
    }
}

fn reagents(r: &Value) -> String {
    if let Some(list) = r["inputItems"].as_array() {
        return list
            .iter()
            .map(|i| format!("{}x {}", i["quantity"].as_f64().unwrap_or(1.0), i["name"].as_str().unwrap_or("?")))
            .collect::<Vec<_>>()
            .join(", ");
    }
    r["materials"].as_object().map_or(String::new(), |m| m.iter().map(|(id, n)| format!("{}x item {id}", n)).collect::<Vec<_>>().join(", "))
}

/// `fbl-rs wowcraft recipes <profession> [filter] [--json] [--refresh]`.
pub fn recipes_report(
    profession: &str,
    filter: Option<&str>,
    json: bool,
    refresh: bool,
    name_of: &dyn Fn(u32) -> Option<String>,
) -> Result<String, String> {
    let p = profession_name(profession)?;
    let all = recipes(refresh)?;
    let list = all.get(p).and_then(Value::as_array).cloned().unwrap_or_default();
    let f = filter.map(str::to_lowercase);
    let mut list: Vec<Value> =
        list.into_iter().filter(|r| f.as_ref().is_none_or(|f| r["name"].as_str().unwrap_or("").to_lowercase().contains(f))).collect();
    list.sort_by(|a, b| a["skill"].as_f64().unwrap_or(0.0).total_cmp(&b["skill"].as_f64().unwrap_or(0.0)));
    // Reagents the page only knows by id: named from the local QuestieDB when it's installed.
    for r in &mut list {
        if let Some(items) = r["inputItems"].as_array_mut() {
            for i in items {
                if i["name"].as_str().is_some_and(|n| n.starts_with("item ")) {
                    if let Some(n) = i["id"].as_u64().and_then(|id| name_of(id as u32)) {
                        i["name"] = Value::String(n);
                    }
                }
            }
        }
    }
    if json {
        return serde_json::to_string_pretty(&list).map_err(|e| e.to_string());
    }
    let mut out = vec![format!("{p} on wowcraft.io (Forever): {} recipes. Skill colours are orange/yellow/green/grey.", list.len())];
    for r in &list {
        out.push(format!(
            "  {:>3}  {:<40} {:<15} {:<28} {}",
            r["skill"].as_f64().unwrap_or(0.0),
            r["name"].as_str().unwrap_or("?"),
            colours(r),
            source(r),
            reagents(r)
        ));
    }
    Ok(out.join("\n"))
}

/// `fbl-rs wowcraft cost <profession> [--realm "Forever PvP US"] [--faction Alliance]`.
pub fn cost_report(profession: &str, realm: &str, faction: &str) -> Result<String, String> {
    let p = profession_name(profession)?;
    let realm = REALMS
        .iter()
        .find(|r| r.eq_ignore_ascii_case(realm.trim()))
        .ok_or_else(|| format!("\"{realm}\" isn't a wowcraft realm: {}.", REALMS.join(", ")))?;
    let faction =
        ["Alliance", "Horde"].into_iter().find(|f| f.eq_ignore_ascii_case(faction.trim())).ok_or("--faction is Alliance or Horde")?;
    let key = format!("cost-{p}-{realm}-{faction}.json").replace(' ', "_");
    let path = cache_dir().join(key);
    let text = match fresh(&path, Duration::from_secs(3600)) {
        Some(t) => t,
        None => {
            let q = |s: &str| s.replace(' ', "%20");
            let t = get(&format!("{BASE}/api/profession-costs?profession={p}&realm={}&faction={faction}", q(realm)))?;
            save(&path, &t);
            t
        }
    };
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("wowcraft.io sent something unexpected: {e}"))?;
    let points: Vec<(String, f64)> = v["points"]
        .as_array()
        .map(|a| a.iter().filter_map(|p| Some((p["at"].as_str()?.to_string(), p["copper"].as_f64()?))).collect())
        .unwrap_or_default();
    let Some((last_at, last)) = points.last().cloned() else {
        return Ok(format!("wowcraft.io has no cost history for {p} on {realm} ({faction})."));
    };
    let (lo, hi) = points.iter().fold((f64::MAX, 0.0f64), |(lo, hi), (_, c)| (lo.min(*c), hi.max(*c)));
    let first = &points[0];
    let money = crate::format::money_long;
    Ok(format!(
        "{p} to 300 on wowcraft.io, {realm} ({faction}): {} as of {last_at}.\n  {} points since {}: first {}, lowest {}, highest {}.\n  Their cost assumes buying everything; `fbl-rs skillup` counts what you already hold.",
        money(last),
        points.len(),
        first.0,
        money(first.1),
        money(lo),
        money(hi)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"var a=1;let s=JSON.parse('[{"id":1,"name":"Lion\'s Brew","minSkill":1,"difficulty":{"orange":1,"yellow":40,"green":60,"gray":80},"source":{"type":"trainer","cost":50}}]'),q=JSON.parse('[{"id":2,"name":"Copper Bar","minSkill":1},{"id":3,"name":"Hard Gold Bar","minSkill":5},{"id":9,"name":"Gone","minSkill":5}]'),C=JSON.parse("[9,10]");let tL=new Set(C);function tC(e){return e.filter(e=>!tL.has(e.id))}let tB={Vanilla:{Alchemy:s},Forever:{Alchemy:tC(s),Blacksmithing:tC(q).filter(e=>{var t;return!(null==(t=e.name)?void 0:t.startsWith("Hard Gold"))}),Cooking:[],Tailoring:s}};"#;

    #[test]
    fn unescapes_js_strings() {
        assert_eq!(unescape_js(r"Lion\'s A\x42\\n"), "Lion's AB\\n");
    }

    #[test]
    fn extracts_forever_recipes_with_the_page_filters() {
        let all = extract_recipes(PAGE).expect("extract");
        assert_eq!(all["Alchemy"][0]["name"], "Lion's Brew");
        let bs: Vec<&str> = all["Blacksmithing"].as_array().unwrap().iter().map(|r| r["name"].as_str().unwrap()).collect();
        assert_eq!(bs, vec!["Copper Bar"]); // id 9 left out, Hard Gold filtered
        assert_eq!(all["Cooking"].as_array().unwrap().len(), 0);
        assert_eq!(all["Tailoring"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn names_professions_and_sources() {
        assert_eq!(profession_name("tailoring").unwrap(), "Tailoring");
        assert!(profession_name("fishing").is_err());
        let all = extract_recipes(PAGE).unwrap();
        assert!(source(&all["Alchemy"][0]).starts_with("trainer"));
        assert_eq!(colours(&all["Alchemy"][0]), "1/40/60/80");
        assert_eq!(all["Alchemy"][0]["skill"], 1);
    }
}
