//! ICON-SPIKE probe: reads 20 item icons out of a real WoW: Forever install
//! and reports how long each step took.
//!
//!     icon_probe.exe ["C:\...\World of Warcraft\_classic_beta_"] [fileDataID ...]
//!
//! With no folder it tries the usual install paths, then asks (dragging the
//! folder into the window types its path). Only reads the game's files; the
//! PNGs and `timings.txt` go to an `icon-probe` folder next to the exe.
//! Safe to run with the game open.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use wow_forever_buddy_lib::casc::{build_of, Casc, IconCache};

/// Item icons that exist in build 1.60.1.70205 (one is a 1024x1024 texture,
/// to time a big one), checked against the public CDN.
const ICONS: [u32; 20] = [
    132000, 132214, 132429, 132643, 132857, 133071, 133286, 133502, 133716, 133930, 134144, 134358,
    134572, 134786, 135001, 135216, 135430, 135644, 135858, 136072,
];

const GUESSES: &[&str] = &[
    r"C:\Program Files (x86)\World of Warcraft\_classic_beta_",
    r"C:\Program Files\World of Warcraft\_classic_beta_",
    r"D:\World of Warcraft\_classic_beta_",
    r"D:\Games\World of Warcraft\_classic_beta_",
];

fn main() {
    let mut args = std::env::args().skip(1);
    let flavor = args
        .next()
        .map(PathBuf::from)
        .or_else(|| GUESSES.iter().map(PathBuf::from).find(|p| p.is_dir()))
        .unwrap_or_else(ask_for_folder);
    let mut ids: Vec<u32> = args.filter_map(|a| a.parse().ok()).collect();
    if ids.is_empty() {
        ids = ICONS.to_vec();
    }
    let out = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|d| d.join("icon-probe")))
        .unwrap_or_else(|| PathBuf::from("icon-probe"));

    let report = run(&flavor, &ids, &out);
    println!("{report}");
    let _ = std::fs::create_dir_all(&out);
    let _ = std::fs::write(out.join("timings.txt"), &report);
    println!("\nSaved to {}", out.display());
    println!("Press Enter to close.");
    let _ = std::io::stdin().read_line(&mut String::new());
}

fn run(flavor: &Path, ids: &[u32], out: &Path) -> String {
    let mut r = format!(
        "icon_probe {}\nfolder: {}\n",
        env!("CARGO_PKG_VERSION"),
        flavor.display()
    );

    let t = Instant::now();
    let build = match build_of(flavor) {
        Ok(b) => b,
        Err(e) => return r + &format!("FAILED reading the build: {e}\n"),
    };
    r += &format!("build: {} ({})\n", build.version, ms(t.elapsed()));

    let t = Instant::now();
    let casc = match Casc::open(flavor) {
        Ok(c) => c,
        Err(e) => return r + &format!("FAILED opening storage: {e}\n"),
    };
    r += &format!(
        "open (config, index list, encoding, root): {}\n",
        ms(t.elapsed())
    );

    // A fresh cache each run, so the first pass really reads the game.
    let cache_dir = out.join("cache");
    let _ = std::fs::remove_dir_all(&cache_dir);
    let cache = IconCache::new(&cache_dir);

    let t = Instant::now();
    let cold = cache.fill(&casc, ids);
    let cold_time = t.elapsed();
    let ok = cold.iter().filter(|(_, res)| res.is_ok()).count();
    r += &format!(
        "cold: {ok} of {} icons in {} ({} each)\n",
        ids.len(),
        ms(cold_time),
        ms(cold_time / ids.len().max(1) as u32)
    );
    for (id, res) in &cold {
        match res {
            Ok(path) => {
                let _ = std::fs::copy(path, out.join(format!("{id}.png")));
            }
            Err(e) => r += &format!("  {id}: {e}\n"),
        }
    }

    let t = Instant::now();
    let warm = ids
        .iter()
        .filter(|&&id| cache.cached(&build, id).is_some())
        .count();
    r += &format!("warm (cache hits): {warm} in {}\n", ms(t.elapsed()));
    r
}

fn ms(d: Duration) -> String {
    format!("{:.1} ms", d.as_secs_f64() * 1000.0)
}

fn ask_for_folder() -> PathBuf {
    print!("Drag your _classic_beta_ folder into this window, then press Enter: ");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    PathBuf::from(line.trim().trim_matches('"'))
}
