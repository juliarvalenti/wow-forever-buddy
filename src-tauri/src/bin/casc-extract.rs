//! Extracts Blizzard's own Interface code and UI art out of the installed
//! game into a dev-local cache (ticket CASC2): the community listfile maps
//! paths to FileDataIDs, and the CASC reader (the one behind item icons)
//! gives the bytes. The cache is keyed by build, like icons, and is never
//! committed: it's Blizzard's files, refetched per build.
//!
//!     casc-extract --flavor "/Applications/World of Warcraft/_forever_" \
//!         --listfile tools/casc-listfile/files.csv
//!
//! Text and XML are written raw; `.blp` textures are decoded to PNG. Files
//! the game hasn't downloaded yet are reported, not fatal: one login in
//! game streams them in, and the next run takes them.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use wow_forever_buddy_lib::casc;

fn main() -> ExitCode {
    let mut flavor: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut listfile: Option<PathBuf> = None;
    let mut filters: Vec<String> = Vec::new();
    let mut dry_run = false;
    let args = std::env::args().skip(1);
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        let mut value = || {
            args.next()
                .unwrap_or_else(|| usage_then_exit(format!("{arg} needs a value")))
        };
        match arg.as_str() {
            "--flavor" => flavor = Some(PathBuf::from(value())),
            "--out" => out = Some(PathBuf::from(value())),
            "--listfile" => listfile = Some(PathBuf::from(value())),
            "--filter" => filters.push(value()),
            "--dry-run" => dry_run = true,
            other => usage_then_exit(format!("unknown argument {other}")),
        }
    }

    let Some(flavor) = flavor else {
        usage_then_exit("--flavor is required (the game's flavor folder, e.g. \"/Applications/World of Warcraft/_forever_\")".to_string())
    };
    let listfile = listfile.unwrap_or_else(default_listfile);
    let text = match std::fs::read_to_string(&listfile) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("can't read the listfile at {}: {e}", listfile.display());
            eprintln!("fetch it first: tools/casc-listfile/fetch-listfile.sh");
            return ExitCode::FAILURE;
        }
    };
    let entries = casc::parse_listfile(&text);
    if entries.is_empty() {
        eprintln!("the listfile at {} has no usable rows", listfile.display());
        return ExitCode::FAILURE;
    }
    let filters = (!filters.is_empty()).then_some(filters);
    let filters: Vec<String> = filters
        .unwrap_or_else(|| casc::DEFAULT_FILTERS.iter().map(|f| f.to_string()).collect());

    let build = match casc::build_of(&flavor) {
        Ok(build) => build,
        Err(e) => {
            eprintln!("can't read the installed build at {}: {e}", flavor.display());
            return ExitCode::FAILURE;
        }
    };
    println!(
        "build {} ({}), {} listfile rows, {} filters{}",
        build.version,
        casc::hex(&build.build_key),
        entries.len(),
        filters.len(),
        if dry_run { ", dry run" } else { "" }
    );
    let storage = match casc::Casc::open(&flavor) {
        Ok(storage) => storage,
        Err(e) => {
            eprintln!("can't open the game's CASC storage: {e}");
            return ExitCode::FAILURE;
        }
    };
    let out = out.unwrap_or_else(default_cache);
    let summary = casc::run(&storage, &out, &entries, &filters, dry_run);

    println!("cache: {}", out.display());
    println!(
        "extracted {}, skipped {} (already cached), {} MB",
        summary.extracted,
        summary.skipped,
        summary.bytes / (1024 * 1024)
    );
    println!(
        "not downloaded yet {}, not in this build {}, failed {}",
        summary.not_local, summary.missing, summary.failed
    );
    if summary.not_local > 0 {
        println!("log in once in game, so the client downloads them, then run this again");
    }
    ExitCode::SUCCESS
}

/// Next to the icon cache, so one app data folder holds everything read
/// from the game.
fn default_cache() -> PathBuf {
    app_cache().join("casc-extract")
}

fn default_listfile() -> PathBuf {
    Path::new("tools").join("casc-listfile").join("files.csv")
}

fn app_cache() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.juliarvalenti.wowforeverbuddy")
        .join("cache")
}

fn usage_then_exit(problem: String) -> ! {
    eprintln!("{problem}");
    eprintln!();
    eprintln!(
        "usage: casc-extract --flavor <flavor dir> [--out <cache dir>] \\\n\
         \x20        [--listfile <csv>] [--filter <path prefix>]... [--dry-run]"
    );
    std::process::exit(2);
}