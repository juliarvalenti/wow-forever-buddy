//! fbl-rs: the command line.
//!
//! ```text
//! fbl-rs <command> [character] [query] [--name value]   a character command, printing JSON
//! fbl-rs market [--budget 5] [--min-listed 100] [--top 12] [--json]
//! fbl-rs live [--port 4747] [--budget 5] [--min-listed 100] [--no-open] [--archive dir] [--web dir]
//! fbl-rs backtest [--min-listed 100] [--min 500] [--max 50000]
//! fbl-rs snipes [--cap 0.25] [--ratio 0.8] [--deposit 0.01] [--min-profit 0.02] [--count 50]
//!               [--min-listed 300] [--name "FBL Snipes"] [--out file | --dir folder]
//! fbl-rs skillup [character] [profession] [--to 225] [--from N] [--craft 20]
//! fbl-rs questxp [character] [min level] [max level] [zone] | [character] --chain "Quest name"
//! fbl-rs bis [character] [--levels 40,45,50,60] [--all]
//! fbl-rs npc <name or title> [--zone name] [--json]      where an NPC stands (needs QuestieDB)
//! fbl-rs mcp                                            the commands as an MCP server on stdio
//! ```
//!
//! Files come from FBL_FILE / FBL_WTF / FBL_AUCTIONATOR / QUESTIEDB_DIR / FBL_HIDDEN / WOWSIMS_DB
//! (see `sources`).

use fbl::backtest::{self, BacktestOptions};
use fbl::config::Config;
use fbl::live::{Live, LiveOptions};
use fbl::snipes::{self, SnipeOptions};
use fbl::sources::{self, addon, auctionator, questie, Env};
use fbl::tools::{self, Args, TOOLS};
use fbl::{bis, format, json, mcp, npcs, questxp, report, skillup, strategies};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

/// Flags that take no value.
const SWITCHES: [&str; 3] = ["json", "no-open", "all"];

/// A subcommand's arguments: `--name value` flags (and switches), and the rest in order.
struct Cli {
    flags: HashMap<String, String>,
    positional: Vec<String>,
}

impl Cli {
    fn parse(args: &[String]) -> Cli {
        let mut cli = Cli { flags: HashMap::new(), positional: Vec::new() };
        let mut i = 0;
        while i < args.len() {
            match args[i].strip_prefix("--") {
                Some(name) if SWITCHES.contains(&name) => {
                    cli.flags.insert(name.into(), String::new());
                    i += 1;
                }
                Some(name) => {
                    cli.flags.insert(name.into(), args.get(i + 1).cloned().unwrap_or_default());
                    i += 2;
                }
                None => {
                    cli.positional.push(args[i].clone());
                    i += 1;
                }
            }
        }
        cli
    }

    fn flag(&self, name: &str) -> Option<&str> {
        self.flags.get(name).map(String::as_str)
    }

    fn has(&self, name: &str) -> bool {
        self.flags.contains_key(name)
    }

    fn number(&self, name: &str, default: f64) -> Result<f64, String> {
        match self.flag(name) {
            None => Ok(default),
            Some(v) => v.parse().map_err(|_| format!("--{name} wants a number, not \"{v}\"")),
        }
    }

    fn arg(&self, i: usize) -> Option<&str> {
        self.positional.get(i).map(String::as_str)
    }
}

fn usage() {
    println!("usage: fbl-rs <command> [character] [query] [--name value]\n\ncommands:");
    for t in TOOLS {
        println!("  {:<11} {}", t.name, t.description);
    }
    let more = [
        ("market", "Auction house picks from Auctionator's scans: buy, dips, sell, squeeze, flood, ladders, swingy."),
        ("live", "The dashboard: watches the game's files, archives each scan, serves http://localhost:4747."),
        ("backtest", "Replays the market rules on past days against a control."),
        ("snipes", "A Point Blank Sniper shopping list: busy items capped under their usual price."),
        ("skillup", "The cheapest way up a profession, priced from the last scan."),
        ("questxp", "XP-dense zones, hubs and chains for a character (needs QuestieDB)."),
        ("bis", "Best in slot by level for an agility melee (needs the WoWSims Forever database)."),
        ("npc", "Where an NPC stands, by name or title (\"first aid\", \"Fahrad\"); --zone narrows it (needs QuestieDB)."),
        ("mcp", "The commands above the line as an MCP server on stdio."),
    ];
    for (name, what) in more {
        println!("  {name:<11} {what}");
    }
}

fn market(cli: &Cli, env: &Env) -> Result<String, String> {
    let budget = (cli.number("budget", 5.0)? * 10000.0) as i64;
    let top = cli.number("top", 12.0)? as usize;
    let cfg = Config { min_listed: cli.number("min-listed", 100.0)? as i64, ..Config::default() };
    let inputs = sources::market_inputs(env, &cfg, format::now_seconds())?;
    let analysis = strategies::analyze(&inputs, budget, &cfg);
    if cli.has("json") {
        return Ok(json::pretty(&fbl::dashboard::opps_json(&analysis)));
    }
    Ok(report::report(&analysis, top))
}

fn live(cli: &Cli, env: Env) -> Result<String, String> {
    let cfg = Config { min_listed: cli.number("min-listed", 100.0)? as i64, ..Config::default() };
    let opts = LiveOptions {
        port: cli.number("port", 4747.0)? as u16,
        budget: (cli.number("budget", 5.0)? * 10000.0) as i64,
        open_browser: !cli.has("no-open"),
        archive_dir: cli.flag("archive").map(PathBuf::from).unwrap_or_else(LiveOptions::default_archive_dir),
        web_dir: cli.flag("web").map(PathBuf::from).unwrap_or_else(LiveOptions::default_web_dir),
    };
    if !opts.web_dir.join("index.html").exists() {
        return Err(format!("No dashboard at {} (pass --web <folder with index.html>).", opts.web_dir.display()));
    }
    Arc::new(Live::new(env, cfg, opts)).run()?;
    Ok(String::new())
}

fn backtest(cli: &Cli, env: &Env) -> Result<String, String> {
    let opts = BacktestOptions {
        min_listed: cli.number("min-listed", 100.0)?,
        min_price: cli.number("min", 500.0)?,
        max_price: cli.number("max", 50_000.0)?,
    };
    let prices = auctionator::load(env)?;
    let trades = backtest::simulate(&prices, &opts, &Config::default());
    Ok(backtest::report(&trades, &opts))
}

fn snipes_command(cli: &Cli, env: &Env) -> Result<String, String> {
    let mut o = SnipeOptions {
        cap: cli.number("cap", 0.25)? * 10000.0,
        ratio: cli.number("ratio", 0.8)?,
        deposit: cli.number("deposit", 0.01)? * 10000.0,
        min_profit: cli.number("min-profit", 0.02)? * 10000.0,
        count: cli.number("count", 50.0)? as usize,
        min_listed: cli.number("min-listed", 300.0)?,
        ..SnipeOptions::default()
    };
    let list_name = format!("{} {}", cli.flag("name").unwrap_or("FBL Snipes"), snipes::random_tag());
    let addon = addon::load(env)?;
    let data: skillup::ProfessionData = serde_json::from_str(skillup::recipe_file("engineering").unwrap()).map_err(|e| e.to_string())?;
    let questie = questie::load_items(&env.questiedb_dir(), Config::default().vendor_cutoff);
    // Never list what you've hidden or what vendors sell without limit (as the dashboard does).
    o.exclude = questie.vendor_items.clone();
    o.exclude.extend(sources::hidden::load(&env.hidden_file()).keys().copied());
    let names = snipes::names(&addon, &data.items, &questie);
    let picks = snipes::snipes(&auctionator::load(env)?, &names, &o);
    let line = snipes::shopping_list(&list_name, &picks);
    let today = &format::iso_from_seconds(format::now_seconds())[..10];
    let out_file = cli
        .flag("out")
        .map(PathBuf::from)
        .or_else(|| cli.flag("dir").map(|d| PathBuf::from(d).join(today).join(format!("{}.txt", snipes::file_name(&list_name)))));
    if let Some(file) = &out_file {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(file, format!("{line}\n")).map_err(|e| e.to_string())?;
    }
    for s in &picks {
        eprintln!("{}", snipes::describe(s));
    }
    Ok(out_file.map_or(line, |f| f.display().to_string()))
}

/// The named character, or the most recently saved.
fn character<'a>(data: &'a addon::AddonData, name: Option<&str>) -> Result<&'a addon::Character, String> {
    tools::pick(data, name)
}

fn skillup_command(cli: &Cli, env: &Env) -> Result<String, String> {
    let addon = addon::load(env)?;
    let c = character(&addon, cli.arg(0))?;
    let profession = cli.arg(1).unwrap_or("engineering");
    let file = skillup::recipe_file(profession).ok_or_else(|| format!("No recipe data for {profession} (only engineering so far)."))?;
    let data: skillup::ProfessionData = serde_json::from_str(file).map_err(|e| e.to_string())?;
    let o = skillup::SkillupOptions {
        to: cli.number("to", 225.0)?,
        from: cli.flag("from").map(|f| f.parse().map_err(|_| "--from wants a number".to_string())).transpose()?,
        craft_time: cli.number("craft", 20.0)?,
    };
    skillup::plan(&addon, &auctionator::load(env)?, c, &data, &o)
}

fn questxp_command(cli: &Cli, env: &Env) -> Result<String, String> {
    let addon = addon::load(env)?;
    let c = character(&addon, cli.arg(0))?;
    let dir = env.questiedb_dir();
    let db = questie::QuestieDb::load(&dir).ok_or_else(|| format!("No QuestieDB at {} (set QUESTIEDB_DIR).", dir.display()))?;
    let xp = questxp::load_xp(&dir)?;
    let q = questxp::QuestXp::new(&db, &xp, c);
    if let Some(name) = cli.flag("chain") {
        // How much each reward beats what you wear, when /fbl items saved its stats.
        let mut gains = HashMap::new();
        let args: Args = [("character", c.key.as_str()), ("source", "items"), ("ahead", "15"), ("per_slot", "20")]
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        if let Ok(u) = tools::call("upgrades", &args, env) {
            for row in u["upgrades"].as_object().into_iter().flat_map(|o| o.values()).flat_map(|rows| rows.as_array().into_iter().flatten())
            {
                if let (Some(n), Some(g)) = (row["name"].as_str(), row["gain"].as_f64()) {
                    let best = gains.entry(n.to_string()).or_insert(0.0f64);
                    *best = best.max(g);
                }
            }
        }
        return q.chain(name, &gains);
    }
    let level = |i: usize, default: f64| cli.arg(i).map_or(Ok(default), |v| v.parse::<f64>().map_err(|_| format!("\"{v}\" isn't a level")));
    Ok(q.overview(&c.key, level(1, 33.0)?, level(2, 42.0)?, cli.arg(3)))
}

fn bis_command(cli: &Cli, env: &Env) -> Result<String, String> {
    let addon = addon::load(env)?;
    let c = character(&addon, cli.arg(0))?;
    let levels: Vec<i64> = cli.flag("levels").unwrap_or("40,45,50,60").split(',').filter_map(|l| l.trim().parse().ok()).collect();
    let path = bis::default_db_path();
    let text = std::fs::read_to_string(&path).map_err(|_| {
        format!("No WoWSims item database at {}; clone wowsims/forever next to this repo or set WOWSIMS_DB.", path.display())
    })?;
    let db: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(bis::report(c, &db, &levels, cli.has("all")))
}

/// `npc <query words> [--zone name] [--json]`: every positional word is part of the query.
fn npc_command(cli: &Cli, env: &Env) -> Result<String, String> {
    let query = cli.positional.join(" ");
    if query.trim().is_empty() {
        return Err("usage: fbl-rs npc <name or title> [--zone name] [--json]".into());
    }
    let dir = env.questiedb_dir();
    let db = questie::QuestieDb::load(&dir).ok_or_else(|| format!("No QuestieDB at {} (set QUESTIEDB_DIR).", dir.display()))?;
    let found = npcs::search(&db, query.trim(), cli.flag("zone"));
    if cli.has("json") {
        return serde_json::to_string_pretty(&found).map_err(|e| e.to_string());
    }
    Ok(npcs::report(&found, 25))
}

/// A character command: positionals fill its arguments in order (`ids` takes the rest).
fn tool(t: &tools::ToolInfo, cli: &Cli, env: &Env) -> Result<String, String> {
    let mut args: Args = cli.flags.clone();
    let mut positional = cli.positional.iter();
    for (name, _) in t.args.iter().filter(|(n, _)| !cli.flags.contains_key(*n)) {
        if *name == "ids" {
            let rest: Vec<&String> = positional.by_ref().collect();
            if !rest.is_empty() {
                args.insert("ids".into(), rest.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(","));
            }
        } else if let Some(v) = positional.next() {
            args.insert(name.to_string(), v.clone());
        }
    }
    tools::call(t.name, &args, env).map(|v| json::pretty(&v))
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let env = Env::from_process();
    let Some(cmd) = argv.first().map(String::as_str) else {
        usage();
        return ExitCode::SUCCESS;
    };
    let cli = Cli::parse(&argv[1..]);
    let result = match cmd {
        "help" | "--help" | "-h" => {
            usage();
            return ExitCode::SUCCESS;
        }
        "mcp" => mcp::serve(std::io::stdin().lock(), std::io::stdout(), &env).map(|_| String::new()).map_err(|e| e.to_string()),
        "market" => market(&cli, &env),
        "live" => live(&cli, env),
        "backtest" => backtest(&cli, &env),
        "snipes" => snipes_command(&cli, &env),
        "skillup" => skillup_command(&cli, &env),
        "questxp" => questxp_command(&cli, &env),
        "bis" => bis_command(&cli, &env),
        "npc" => npc_command(&cli, &env),
        _ => match TOOLS.iter().find(|t| t.name == cmd) {
            Some(t) => tool(t, &cli, &env),
            None => {
                eprintln!("Unknown command \"{cmd}\". Try fbl-rs help.");
                return ExitCode::from(2);
            }
        },
    };
    match result {
        Ok(text) if text.is_empty() => ExitCode::SUCCESS,
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
