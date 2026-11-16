//! fbl-rs: the command line.
//!
//! ```text
//! fbl-rs <command> [character] [query] [--name value]   a character command, printing JSON
//! fbl-rs market [--budget 5] [--min-listed 100] [--top 12] [--json]
//! fbl-rs backtest [--min-listed 100] [--min 500] [--max 50000]
//! fbl-rs mcp                                            the commands as an MCP server on stdio
//! ```
//!
//! Files come from FBL_FILE / FBL_WTF / FBL_AUCTIONATOR / QUESTIEDB_DIR / FBL_HIDDEN (see `sources`).

use fbl::backtest::{self, BacktestOptions};
use fbl::config::Config;
use fbl::sources::{self, Env};
use fbl::tools::{self, Args, TOOLS};
use fbl::{format, json, mcp, report, strategies};
use std::process::ExitCode;

/// `--name value` from the arguments, if given.
fn flag<'a>(argv: &'a [String], name: &str) -> Option<&'a str> {
    let i = argv.iter().position(|a| a == &format!("--{name}"))?;
    argv.get(i + 1).map(String::as_str)
}

fn flag_number(argv: &[String], name: &str, default: f64) -> Result<f64, String> {
    match flag(argv, name) {
        None => Ok(default),
        Some(v) => v.parse().map_err(|_| format!("--{name} wants a number, not \"{v}\"")),
    }
}

fn usage() {
    println!("usage: fbl-rs <command> [character] [query]\n       fbl-rs market | backtest | mcp\n\ncommands:");
    for t in TOOLS {
        println!("  {:<11} {}", t.name, t.description);
    }
    println!("  {:<11} Auction house picks from Auctionator's scans: buy, dips, sell, squeeze, flood, ladders, swingy.", "market");
    println!("  {:<11} Replays the market rules on past days against a control.", "backtest");
}

fn market(argv: &[String], env: &Env) -> Result<String, String> {
    let budget = (flag_number(argv, "budget", 5.0)? * 10000.0) as i64;
    let top = flag_number(argv, "top", 12.0)? as usize;
    let cfg = Config { min_listed: flag_number(argv, "min-listed", 100.0)? as i64, ..Config::default() };
    let inputs = sources::market_inputs(env, &cfg, format::now_seconds())?;
    let analysis = strategies::analyze(&inputs, budget, &cfg);
    if argv.iter().any(|a| a == "--json") {
        return Ok(json::pretty(&serde_json::to_value(&analysis).map_err(|e| e.to_string())?));
    }
    Ok(report::report(&analysis, top))
}

fn backtest(argv: &[String], env: &Env) -> Result<String, String> {
    let opts = BacktestOptions {
        min_listed: flag_number(argv, "min-listed", 100.0)?,
        min_price: flag_number(argv, "min", 500.0)?,
        max_price: flag_number(argv, "max", 50_000.0)?,
    };
    let prices = sources::auctionator::load(env)?;
    let trades = backtest::simulate(&prices, &opts, &Config::default());
    Ok(backtest::report(&trades, &opts))
}

/// Positional arguments fill the tool's arguments in order; `--name value` works too. `ids` takes
/// all the remaining positionals.
fn tool_args(tool: &tools::ToolInfo, rest: &[String]) -> Args {
    let mut args = Args::new();
    let mut positional: Vec<String> = Vec::new();
    let mut i = 0;
    while i < rest.len() {
        if let Some(name) = rest[i].strip_prefix("--") {
            args.insert(name.to_string(), rest.get(i + 1).cloned().unwrap_or_default());
            i += 2;
        } else {
            positional.push(rest[i].clone());
            i += 1;
        }
    }
    let mut positional = positional.into_iter();
    let unset: Vec<&str> = tool.args.iter().map(|(n, _)| *n).filter(|n| !args.contains_key(*n)).collect();
    for name in unset {
        if name == "ids" {
            let all: Vec<String> = positional.by_ref().collect();
            if !all.is_empty() {
                args.insert("ids".into(), all.join(","));
            }
        } else if let Some(v) = positional.next() {
            args.insert(name.to_string(), v);
        }
    }
    args
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let env = Env::from_process();
    let Some(cmd) = argv.first().map(String::as_str) else {
        usage();
        return ExitCode::SUCCESS;
    };
    let rest = &argv[1..];
    let result = match cmd {
        "help" | "--help" | "-h" => {
            usage();
            return ExitCode::SUCCESS;
        }
        "mcp" => {
            let stdin = std::io::stdin();
            return match mcp::serve(stdin.lock(), std::io::stdout(), &env) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            };
        }
        "market" => market(rest, &env),
        "backtest" => backtest(rest, &env),
        _ => match TOOLS.iter().find(|t| t.name == cmd) {
            None => {
                eprintln!("Unknown command \"{cmd}\".");
                return ExitCode::from(2);
            }
            Some(tool) => tools::call(cmd, &tool_args(tool, rest), &env).map(|v| json::pretty(&v)),
        },
    };
    match result {
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
