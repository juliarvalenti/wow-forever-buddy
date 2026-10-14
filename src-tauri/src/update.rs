//! U2: the app's own updates (IMPLEMENTING §23).
//!
//! Once at start and once a day, while Settings › "Check for updates" is on,
//! the app asks GitHub for `latest.json` (the endpoint is fixed in
//! tauri.conf.json). That's its one network call the user didn't add
//! themselves, and it carries no ids: the app version is only in the user
//! agent. Nothing installs without a click.
//!
//! - The installer build downloads the new installer, and the updater plugin
//!   checks its signature against the public key in tauri.conf.json before
//!   anything runs. A bad signature means nothing changes.
//! - The portable exe can't replace itself: the banner links to the release
//!   page instead.

use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_updater::{Update, UpdaterExt};
use tauri_specta::Event;

use crate::state::AppState;

/// "Download from GitHub" and "What's new": always this page, never a URL
/// from latest.json.
pub const RELEASES_PAGE: &str =
    "https://github.com/juliarvalenti/wow-forever-buddy/releases/latest";

/// Where our release assets live. latest.json names the installer's URL; one
/// anywhere else is refused before downloading. (The signature check would
/// catch a forged file anyway; this keeps the download on GitHub too.)
const DOWNLOADS: &str = "https://github.com/juliarvalenti/wow-forever-buddy/releases/download/";

/// How often the background loop wakes to see whether a check is due.
pub const TICK: Duration = Duration::from_secs(3600);

/// A day between background checks.
const CHECK_EVERY: chrono::TimeDelta = chrono::TimeDelta::hours(24);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum FailReason {
    /// The download didn't pass its signature check.
    Signature,
    /// GitHub couldn't be reached.
    Offline,
    /// Anything else; the log has the details.
    Other,
}

/// Where the banner is (§23's states).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UpdatePhase {
    /// Up to date, or not checked yet.
    None,
    /// A newer version is out.
    Available {
        version: String,
    },
    Downloading {
        version: String,
        percent: Option<u8>,
    },
    /// Downloaded and verified. `waiting`: "Restart now" was clicked while a
    /// backup, restore or write runs; it restarts once that's done.
    Ready {
        version: String,
        waiting: bool,
    },
    Failed {
        version: String,
        reason: FailReason,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
pub struct UpdateStatus {
    pub phase: UpdatePhase,
    /// The installer build: "Update now" installs. False on the portable exe
    /// (and outside Windows), where the banner links to the release page.
    pub installable: bool,
    /// When GitHub last answered (RFC 3339 UTC).
    pub last_checked: Option<String>,
    /// The last check didn't get an answer, for Settings › About.
    pub check_failed: bool,
    pub checking: bool,
}

/// Held in Tauri's state next to `AppState`.
pub struct Updates {
    status: Mutex<UpdateStatus>,
    /// The update the last check found.
    found: Mutex<Option<Update>>,
    /// The verified installer, once downloaded.
    bytes: Mutex<Option<Vec<u8>>>,
}

impl Updates {
    pub fn new(installable: bool) -> Self {
        Self {
            status: Mutex::new(UpdateStatus {
                phase: UpdatePhase::None,
                installable,
                last_checked: None,
                check_failed: false,
                checking: false,
            }),
            found: Mutex::new(None),
            bytes: Mutex::new(None),
        }
    }

    pub fn status(&self) -> UpdateStatus {
        self.status.lock().expect("update lock poisoned").clone()
    }

    fn change(&self, app: &AppHandle, f: impl FnOnce(&mut UpdateStatus)) {
        let status = {
            let mut s = self.status.lock().expect("update lock poisoned");
            f(&mut s);
            s.clone()
        };
        let _ = status.emit(app);
    }

    /// Whether a downloaded update waits to be installed.
    pub fn ready(&self) -> bool {
        self.bytes.lock().expect("update lock poisoned").is_some()
    }
}

/// The installer build: NSIS puts `uninstall.exe` next to the app's exe. The
/// portable exe runs from wherever it was saved, without one. If this guesses
/// wrong it falls toward the release-page link, which is always safe.
pub fn is_installed(exe_dir: &Path) -> bool {
    cfg!(windows) && exe_dir.join("uninstall.exe").is_file()
}

/// Whether the installer's URL from latest.json is one of our release assets.
fn allowed_download(url: &str) -> bool {
    url.starts_with(DOWNLOADS) && !url[DOWNLOADS.len()..].contains("..")
}

fn reason(e: &tauri_plugin_updater::Error) -> FailReason {
    use tauri_plugin_updater::Error as E;
    match e {
        E::Minisign(_)
        | E::Base64(_)
        | E::SignatureUtf8(_)
        | E::SignedVersionMismatch { .. }
        | E::MissingSignedVersion => FailReason::Signature,
        E::Reqwest(_) | E::Network(_) => FailReason::Offline,
        _ => FailReason::Other,
    }
}

/// A background check is due: the switch is on, and GitHub hasn't answered
/// in the last day.
fn due(on: bool, last: Option<&str>, now: chrono::DateTime<chrono::Utc>) -> bool {
    on && last
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
        .is_none_or(|t| now - t.with_timezone(&chrono::Utc) >= CHECK_EVERY)
}

fn check_on(app: &AppHandle) -> bool {
    use tauri::Manager;
    app.try_state::<AppState>()
        .is_some_and(|s| s.core.settings.get().check_updates)
}

fn log(app: &AppHandle, what: &str) {
    use tauri::Manager;
    if let Some(state) = app.try_state::<AppState>() {
        crate::applog::append(&state.core.paths.log_dir, what);
    }
}

/// Asks GitHub for the newest version. Does nothing while the switch is off.
/// A version already downloaded or downloading is left alone.
pub async fn check(app: &AppHandle, updates: &Updates) -> UpdateStatus {
    if !check_on(app) {
        return updates.status();
    }
    updates.change(app, |s| s.checking = true);
    let result = match app.updater() {
        Ok(updater) => updater.check().await,
        Err(e) => Err(e),
    };
    let now = chrono::Utc::now().to_rfc3339();
    match result {
        Ok(found) => {
            let version = found.as_ref().map(|u| u.version.clone());
            // The installer being downloaded or waiting stays the one that
            // installs: never swap in a newer find under its bytes.
            let busy = matches!(
                updates.status().phase,
                UpdatePhase::Downloading { .. } | UpdatePhase::Ready { .. }
            );
            if !busy {
                *updates.found.lock().expect("update lock poisoned") = found;
            }
            updates.change(app, |s| {
                s.checking = false;
                s.check_failed = false;
                s.last_checked = Some(now);
                if !busy {
                    s.phase = match version {
                        Some(version) => UpdatePhase::Available { version },
                        None => UpdatePhase::None,
                    };
                }
            });
        }
        Err(e) => {
            log(app, &format!("update check failed: {e}"));
            updates.change(app, |s| {
                s.checking = false;
                s.check_failed = true;
            });
        }
    }
    updates.status()
}

/// "Update now" and "Try again" on the installer build: downloads the
/// installer and checks its signature. On success the banner says "ready";
/// installing waits for "Restart now", or for the app to close.
pub async fn download(app: &AppHandle, updates: &Updates) -> UpdateStatus {
    let found = updates.found.lock().expect("update lock poisoned").clone();
    let Some(update) = found else {
        return updates.status();
    };
    let version = update.version.clone();
    // Claimed under the lock, so a double click downloads once.
    let mut started = false;
    updates.change(app, |s| {
        let busy = matches!(
            s.phase,
            UpdatePhase::Downloading { .. } | UpdatePhase::Ready { .. }
        );
        if s.installable && !busy {
            s.phase = UpdatePhase::Downloading {
                version: version.clone(),
                percent: Some(0),
            };
            started = true;
        }
    });
    if !started {
        return updates.status();
    }
    let fail = |reason: FailReason| {
        let version = version.clone();
        updates.change(app, |s| s.phase = UpdatePhase::Failed { version, reason });
        updates.status()
    };
    if !allowed_download(update.download_url.as_str()) {
        log(
            app,
            &format!(
                "update refused: download not on our releases: {}",
                update.download_url
            ),
        );
        return fail(FailReason::Other);
    }
    let mut got = 0u64;
    let mut shown = 0u8;
    let result = update
        .download(
            |chunk, total| {
                got += chunk as u64;
                let Some(total) = total.filter(|&t| t > 0) else {
                    return;
                };
                let percent = (got * 100 / total).min(100) as u8;
                if percent != shown {
                    shown = percent;
                    updates.change(app, |s| {
                        if let UpdatePhase::Downloading { percent: p, .. } = &mut s.phase {
                            *p = Some(percent);
                        }
                    });
                }
            },
            || {},
        )
        .await;
    match result {
        Ok(bytes) => {
            *updates.bytes.lock().expect("update lock poisoned") = Some(bytes);
            updates.change(app, |s| {
                s.phase = UpdatePhase::Ready {
                    version: version.clone(),
                    waiting: false,
                }
            });
            updates.status()
        }
        Err(e) => {
            log(app, &format!("update download failed: {e}"));
            fail(reason(&e))
        }
    }
}

/// Runs the verified installer. With `restart`, it reopens the app after
/// ("Restart now"); without, it's the app closing after "Later", and the next
/// start is the new version. On Windows this exits the app.
///
/// Waits for any backup, restore or game-file write to finish first: they
/// all hold the jobs lock, and this takes it and never gives it back.
pub fn install(app: &AppHandle, updates: &Updates, restart: bool) {
    use tauri::Manager;
    let found = updates.found.lock().expect("update lock poisoned").clone();
    let bytes = updates.bytes.lock().expect("update lock poisoned").take();
    let (Some(update), Some(bytes)) = (found, bytes) else {
        return;
    };
    let state = app.state::<AppState>();
    let _job = match state.core.jobs.try_lock() {
        Ok(job) => job,
        Err(_) => {
            updates.change(app, |s| {
                if let UpdatePhase::Ready { waiting, .. } = &mut s.phase {
                    *waiting = true;
                }
            });
            state.core.jobs.lock().expect("job lock poisoned")
        }
    };
    let version = update.version.clone();
    if let Err(e) = update.restart_after_install(restart).install(&bytes) {
        log(app, &format!("update install failed: {e}"));
        updates.change(app, |s| {
            s.phase = UpdatePhase::Failed {
                version,
                reason: FailReason::Other,
            }
        });
    }
}

/// The background check: at start, then whenever a day has passed since
/// GitHub last answered. Turning the switch on is picked up within the hour.
pub fn spawn(app: &AppHandle) {
    use tauri::Manager;
    let app = app.clone();
    std::thread::spawn(move || loop {
        let updates = app.state::<Updates>();
        let last = updates.status().last_checked;
        if due(check_on(&app), last.as_deref(), chrono::Utc::now()) {
            tauri::async_runtime::block_on(check(&app, &updates));
        }
        std::thread::sleep(TICK);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_our_release_assets_are_downloaded() {
        assert!(allowed_download(
            "https://github.com/juliarvalenti/wow-forever-buddy/releases/download/v0.10.0/WoW.Forever.Buddy_0.10.0_x64-setup.exe"
        ));
        for bad in [
            "https://example.com/releases/download/v0.10.0/setup.exe",
            "http://github.com/juliarvalenti/wow-forever-buddy/releases/download/v1/x.exe",
            "https://github.com/someone-else/wow-forever-buddy/releases/download/v1/x.exe",
            "https://github.com/juliarvalenti/wow-forever-buddy/releases/download/../../../x/y.exe",
            "https://github.com/juliarvalenti/wow-forever-buddy.evil.com/releases/download/v1/x.exe",
        ] {
            assert!(!allowed_download(bad), "{bad}");
        }
    }

    #[test]
    fn checks_are_daily_and_never_while_off() {
        let now = chrono::Utc::now();
        let ago = |h: i64| (now - chrono::TimeDelta::hours(h)).to_rfc3339();
        assert!(due(true, None, now), "at start");
        assert!(!due(true, Some(&ago(3)), now));
        assert!(due(true, Some(&ago(25)), now));
        assert!(due(true, Some("garbage"), now));
        assert!(
            !due(false, None, now),
            "the switch stops even the start-up check"
        );
        assert!(!due(false, Some(&ago(48)), now));
    }

    #[test]
    fn the_portable_exe_is_not_installable() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(!is_installed(tmp.path()));
        std::fs::write(tmp.path().join("uninstall.exe"), b"").unwrap();
        assert_eq!(is_installed(tmp.path()), cfg!(windows));
    }
}
