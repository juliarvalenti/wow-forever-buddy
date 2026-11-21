//! The commands end to end against a fixture file (tests/fixture/ForeverBuddyLite.lua): two
//! characters, a gear scan, and Auctionator files written per test.

use base64::Engine;
use fbl::sources::Env;
use fbl::tools::{self, Args};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixture/ForeverBuddyLite.lua")
}

fn env() -> Env {
    Env::with_file(fixture())
}

fn args(pairs: &[(&str, &str)]) -> Args {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

fn run_with(tool: &str, a: &[(&str, &str)], e: &Env) -> Result<Value, String> {
    tools::call(tool, &args(a), e)
}

fn run(tool: &str, a: &[(&str, &str)]) -> Value {
    run_with(tool, a, &env()).unwrap()
}

/// A temp file that's removed when dropped.
struct TempFile(PathBuf);

impl TempFile {
    fn new(name: &str, contents: &[u8]) -> TempFile {
        let path = std::env::temp_dir().join(format!("fbl-rs-{name}-{}-{:?}.lua", std::process::id(), std::thread::current().id()));
        std::fs::write(&path, contents).unwrap();
        TempFile(path)
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// The fixture as text with LF line endings (git may check it out with CRLF on Windows).
fn fixture_text() -> String {
    std::fs::read_to_string(fixture()).unwrap().replace("\r\n", "\n")
}

/// Bytes as a Lua string body, escaped the way the game's %q does: every byte outside printable
/// ASCII, plus " and \, as \ddd.
fn lua_escape(bytes: &[u8], pad: bool) -> String {
    bytes
        .iter()
        .map(|&b| {
            if (32..127).contains(&b) && b != b'"' && b != b'\\' {
                (b as char).to_string()
            } else if pad {
                format!("\\{b:03}")
            } else {
                format!("\\{b}")
            }
        })
        .collect()
}

/// Enough CBOR to stand in for C_EncodingUtil.SerializeCBOR: maps, arrays, text as byte strings,
/// small ints. Object keys that are digits go out as numbers, like a Lua table's.
fn cbor(v: &Value) -> Vec<u8> {
    fn head(major: u8, n: u64) -> Vec<u8> {
        let m = major << 5;
        match n {
            0..=23 => vec![m | n as u8],
            24..=255 => vec![m | 24, n as u8],
            256..=65535 => vec![m | 25, (n >> 8) as u8, n as u8],
            _ => vec![m | 26, (n >> 24) as u8, (n >> 16) as u8, (n >> 8) as u8, n as u8],
        }
    }
    match v {
        Value::Number(n) => head(0, n.as_u64().unwrap()),
        Value::String(s) => [head(2, s.len() as u64), s.as_bytes().to_vec()].concat(),
        Value::Array(a) => [head(4, a.len() as u64), a.iter().flat_map(cbor).collect()].concat(),
        Value::Object(o) => {
            let mut out = head(5, o.len() as u64);
            for (k, x) in o {
                out.extend(match k.parse::<u64>() {
                    Ok(n) => head(0, n),
                    Err(_) => cbor(&Value::String(k.clone())),
                });
                out.extend(cbor(x));
            }
            out
        }
        _ => panic!("not needed"),
    }
}

/// A small price database: { version = 2, ["2589"] = { m = 125, h = { ["2509"] = 130 } }, ["5201"] = { m = 2500000 } }.
fn auctionator_blob() -> Vec<u8> {
    let mut b = vec![0xa3, 0x67];
    b.extend(b"version");
    b.extend([0x02, 0x64]);
    b.extend(b"2589");
    b.extend([0xa2, 0x61, 0x6d, 0x18, 0x7d, 0x61, 0x68, 0xa1, 0x64]);
    b.extend(b"2509");
    b.extend([0x18, 0x82, 0x64]);
    b.extend(b"5201");
    b.extend([0xa1, 0x61, 0x6d, 0x1a, 0x00, 0x26, 0x25, 0xa0]);
    b
}

#[test]
fn characters_newest_first() {
    let c = run("characters", &[]);
    let keys: Vec<&str> = c.as_array().unwrap().iter().map(|x| x["character"].as_str().unwrap()).collect();
    assert_eq!(keys, ["Testmage-Realm", "Testrogue-Realm"]);
    assert_eq!(c[0]["gold"], json!(12.35));
}

#[test]
fn character_by_first_name_and_unknown_names_list_the_known_ones() {
    assert_eq!(run("summary", &[("character", "testmage")])["level"], json!(20));
    let err = run_with("summary", &[("character", "nobody")], &env()).unwrap_err();
    assert!(err.contains("Known: Testmage-Realm"), "{err}");
}

#[test]
fn gear_by_slot() {
    let g = &run("gear", &[("character", "Testmage")])["gear"];
    assert_eq!(g["main_hand"]["name"], "Emberstone Staff");
    assert_eq!(g["main_hand"]["durability"], "80/90");
    assert_eq!(g["ranged"]["id"], json!(5198));
}

#[test]
fn find_across_characters_bags_and_bank() {
    let rows = run("find", &[("query", "linen")]);
    let got: Vec<(String, String, i64)> = rows
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["character"].as_str().unwrap().into(), r["where"].as_str().unwrap().into(), r["count"].as_i64().unwrap()))
        .collect();
    assert_eq!(got, [("Testmage-Realm".into(), "bags".into(), 17), ("Testrogue-Realm".into(), "bank".into(), 40)]);
    assert_eq!(run("find", &[("query", "17031")])[0]["name"], "Rune of Teleportation");
}

#[test]
fn quests_and_completed() {
    let q = run("quests", &[("character", "Testmage")]);
    assert_eq!(q["completed_count"], json!(3));
    assert_eq!(q["log"][0]["objectives"][0], "[ ] Mosshide Gnoll slain: 6/15");
    let c = run("completed", &[("character", "Testmage"), ("ids", "455, 999")]);
    assert_eq!(c["quests"], json!({"455": true, "999": false}));
}

#[test]
fn talent_trees_are_split_by_column_gaps_and_named_by_class() {
    let t = run("talents", &[("character", "Testmage")]);
    let trees: Vec<(String, i64)> =
        t["trees"].as_array().unwrap().iter().map(|x| (x["name"].as_str().unwrap().into(), x["points"].as_i64().unwrap())).collect();
    assert_eq!(trees, [("Arcane".into(), 9), ("Fire".into(), 0), ("Frost".into(), 2)]);
    assert_eq!(t["trees"][0]["talents"], json!(["Arcane Focus 5/5", "Improved Channeling 3/3", "Arcane Blast 1/1"]));
}

#[test]
fn skills_spells_reputation_stats_mail() {
    let who = [("character", "Testmage")];
    assert_eq!(run("skills", &who)["skills"], json!(["Tailoring 112/150", "Staves 95/100", "Cloth 1/1"]));
    assert_eq!(run("spells", &[("character", "Testmage"), ("query", "blink")])["spells"][0]["id"], json!(1953));
    assert_eq!(run("reputation", &who)["factions"][0], "Stormwind: Friendly 2500/6000");
    assert_eq!(run("stats", &who)["spell_power"]["arcane"], json!(24));
    assert_eq!(run("mail", &who)["letters"][0]["subject"], "A \"Return\" Customer");
}

#[test]
fn prices_and_bag_value_from_an_auctionator_file() {
    let lua = format!(
        "AUCTIONATOR_PRICE_DATABASE = {{\n[\"__dbversion\"] = 8,\n[\"Realm\"] = \"{}\",\n}}\nAUCTIONATOR_SAVEDVARS = {{\n[\"TimeOfLastReplicateScan\"] = 1794600000,\n}}\n",
        lua_escape(&auctionator_blob(), false)
    );
    let file = TempFile::new("auctionator", lua.as_bytes());
    let e = Env { fbl_auctionator: Some(file.0.clone()), ..env() };
    let p = run_with("prices", &[("query", "linen")], &e).unwrap();
    assert_eq!(p["prices"][0]["last"], "0g 1s 25c");
    assert_eq!(p["prices"][0]["history"][0]["day"], "2026-11-14");
    let v = run_with("value", &[("character", "Testmage")], &e).unwrap();
    assert_eq!(v["items"][0]["name"], "Linen Cloth");
    assert_eq!(v["items"][0]["total"], "0g 21s 25c"); // 17 x 1s 25c
}

#[test]
fn upgrades_skills_class_and_level_decide_whats_usable_weights_decide_whats_better() {
    let u = run("upgrades", &[("character", "Testmage")]);
    assert_eq!(u["role"], "caster");
    assert_eq!(u["tree"], "Arcane");
    // Elder's Hat: 6 Sta + 7 Int = 13 over an empty head.
    let head: Vec<Value> = u["upgrades"]["head"].as_array().unwrap().iter().map(|r| json!([r["name"], r["gain"], r["replaces"]])).collect();
    assert_eq!(head, [json!(["Elder's Hat of the Eagle", 13, "(empty)"])]);
    // Fire Staff: 30 sp x 0.8 = 24, Fire damage counts for nothing in Arcane; Emberstone is 8 + 24 x 0.8 = 27.2.
    assert!(u["upgrades"].get("main_hand").is_none());
    assert_eq!(u["not_usable"], json!({"needs Leather": 1, "class": 1, "needs Engineering 120": 1}));
    // Big Staff (req 30) appears only when looking ahead.
    assert_eq!(run("upgrades", &[("character", "Testmage"), ("ahead", "10")])["upgrades"]["main_hand"][0]["name"], "Big Staff");
    // With sp=2 the Fire Staff wins (60 > 8 + 48).
    assert_eq!(run("upgrades", &[("character", "Testmage"), ("weights", "sp=2")])["upgrades"]["main_hand"][0]["name"], "Fire Staff");
    assert_eq!(run("upgrades", &[("character", "Testmage"), ("budget", "0.01")])["upgrades"], json!({}));
}

#[test]
fn mcp_initialize_list_tools_call_one() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_fbl-rs"))
        .arg("mcp")
        .env("FBL_FILE", fixture())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for m in [
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "gear", "arguments": {"character": "Testmage"}}}),
        json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "summary", "arguments": {"character": "nobody"}}}),
    ] {
        writeln!(stdin, "{m}").unwrap();
    }
    drop(stdin); // end of input: the server exits
    let replies: Vec<Value> =
        BufReader::new(child.stdout.take().unwrap()).lines().map(|l| serde_json::from_str(&l.unwrap()).unwrap()).collect();
    child.wait().unwrap();
    let by_id = |id: i64| replies.iter().find(|r| r["id"] == json!(id)).unwrap();
    assert_eq!(replies.len(), 4); // the notification gets no reply
    assert_eq!(by_id(1)["result"]["serverInfo"]["name"], "forever-buddy-lite");
    assert_eq!(by_id(2)["result"]["tools"].as_array().unwrap().len(), tools::TOOLS.len());
    assert!(by_id(3)["result"]["content"][0]["text"].as_str().unwrap().contains("Emberstone Staff"));
    assert_eq!(by_id(4)["result"]["isError"], json!(true));
}

#[test]
fn ah_scan_stored_packed_reads_like_the_plain_one() {
    // A Lua sequence that came out of CBOR as a map keyed 1..n.
    let items = json!({"7357:1205": {"id": 7357, "name": "Elder's Hat of the Eagle", "slot": "INVTYPE_HEAD", "class": 4, "subtype": "Cloth",
        "buyout": 4500, "listings": 2, "tip": {"1": "+6 Stamina", "2": "+7 Intellect", "3": "Requires Level 20"}}});
    let raw = cbor(&items);
    let mut raw_deflated = Vec::new();
    {
        let mut e = flate2::write::DeflateEncoder::new(&mut raw_deflated, flate2::Compression::default());
        e.write_all(&raw).unwrap();
        e.finish().unwrap();
    }
    let mut zlib_deflated = Vec::new();
    {
        let mut e = flate2::write::ZlibEncoder::new(&mut zlib_deflated, flate2::Compression::default());
        e.write_all(&raw).unwrap();
        e.finish().unwrap();
    }
    let b64 = |b: &[u8]| base64::engine::general_purpose::STANDARD.encode(b);
    let packed = b64(&raw_deflated);

    let ah = fbl::sources::addon::unpack_ah(&json!({"at": 5, "count": 1, "packed": packed})).unwrap();
    let items_read = ah.items.unwrap();
    assert_eq!(items_read[0].1.tip, ["+6 Stamina", "+7 Intellect", "Requires Level 20"]);
    // zlib-wrapped works too, and the old plain table passes through.
    let zlib = fbl::sources::addon::unpack_ah(&json!({"packed": b64(&zlib_deflated)})).unwrap();
    assert_eq!(zlib.items.unwrap()[0].1.buyout, Some(4500.0));
    let plain = fbl::sources::addon::unpack_ah(&json!({"items": {"a": {"id": 1}}})).unwrap();
    assert_eq!(plain.items.unwrap()[0].0, "a");

    let text = fixture_text();
    let start = text.find("[\"ah\"] = {").unwrap();
    let end = text.find("\n\t[\"characters\"]").unwrap();
    let replacement = format!("[\"ah\"] = {{ [\"at\"] = 1794600000, [\"complete\"] = false, [\"packed\"] = \"{packed}\" }},\n\t[\"ahError\"] = \"browse failed\",");
    let file = TempFile::new("packed", format!("{}{}{}", &text[..start], replacement, &text[end..]).as_bytes());
    let u = run_with("upgrades", &[("character", "Testmage")], &Env::with_file(&file.0)).unwrap();
    assert_eq!(u["upgrades"]["head"][0]["name"], "Elder's Hat of the Eagle");
    assert!(u["scan_note"].as_str().unwrap().contains("stopped early"));
    assert_eq!(u["scan_error"], "browse failed");
}

#[test]
fn flips_under_the_usual_daily_low_listed_often_worth_the_cut() {
    // 2589 usually 200c, now 100c: a flip. 2592 now at its usual price. 2840 has one day only.
    let day = json!({"2509": 200, "2510": 210, "2511": 190, "2512": 100});
    let db = json!({"version": 2,
        "2589": {"m": 100, "h": day, "l": day, "a": {"2509": 20, "2510": 20, "2511": 20, "2512": 20}},
        "2592": {"m": 200, "h": {"2509": 200, "2510": 200, "2511": 200, "2512": 200}, "a": {"2509": 9, "2510": 9, "2511": 9, "2512": 9}},
        "2840": {"m": 10, "h": {"2512": 500}, "a": {"2512": 50}}});
    let lua = format!("AUCTIONATOR_PRICE_DATABASE = {{\n[\"__dbversion\"] = 8,\n[\"Realm\"] = \"{}\",\n}}\n", lua_escape(&cbor(&db), true));
    let file = TempFile::new("flips", lua.as_bytes());
    let e = Env { fbl_auctionator: Some(file.0.clone()), ..env() };
    let f = run_with("flips", &[("min_profit", "0")], &e).unwrap();
    let got: Vec<Value> = f["flips"].as_array().unwrap().iter().map(|x| json!([x["id"], x["name"], x["share"], x["profit"]])).collect();
    assert_eq!(got, [json!([2589, "Linen Cloth", "50%", "0g 0s 90c"])]);
    assert!(f.get("note").is_none());
    let short = run_with("flips", &[("min_days", "9")], &e).unwrap();
    assert!(short["note"].as_str().unwrap().contains("Only 4 day"));
}

#[test]
fn schema_2_moves_the_scan_error_under_debug_and_it_still_reaches_upgrades() {
    let text = fixture_text().replace("[\"version\"] = 1,", "[\"schema\"] = 2,\n\t[\"debug\"] = { [\"ah_error\"] = \"browse failed\" },");
    let file = TempFile::new("schema2", text.as_bytes());
    let u = run_with("upgrades", &[("character", "Testmage")], &Env::with_file(&file.0)).unwrap();
    assert_eq!(u["scan_error"], "browse failed");
    assert_eq!(u["upgrades"]["head"][0]["name"], "Elder's Hat of the Eagle");
}

#[test]
fn every_tool_runs_on_the_fixture() {
    for t in tools::TOOLS {
        let a: &[(&str, &str)] = match t.name {
            "find" | "prices" => &[("query", "linen")],
            _ => &[],
        };
        let result = run_with(t.name, a, &env());
        // prices needs Auctionator data; recipes, trainers and vendors need data the fixture doesn't have.
        match t.name {
            "prices" | "recipes" | "trainers" | "vendors" | "flips" | "value" => {}
            _ => assert!(result.is_ok(), "{}: {:?}", t.name, result),
        }
    }
}
