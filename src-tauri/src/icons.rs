//! Item icons for the UI (F8): the webview asks for `icon://localhost/<id>`
//! (a FileDataID, nothing else) and gets the PNG from the cache, read from
//! the game's CASC storage on a miss (`casc`). Anything that goes wrong is
//! "no icon", and the UI keeps its letter tile.
//!
//! One worker thread does all of it, so requests that arrive together (a
//! sheet full of `<img>`s) share one root scan, and the storage (about
//! 160 MB once open) is opened on demand and dropped after a minute idle.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::casc::{self, BuildInfo, Casc, IconCache};

/// Close the storage after this long without a miss.
const IDLE: Duration = Duration::from_secs(60);
/// After the storage wouldn't open (no `Data` folder, the game patching),
/// don't try again for this long.
const RETRY_OPEN: Duration = Duration::from_secs(5 * 60);

/// Gets the PNG, or `None` for "no icon".
pub type Reply = Box<dyn FnOnce(Option<Vec<u8>>) + Send>;

/// Settings > Game data cache.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct IconCacheStatus {
    pub files: u32,
    pub bytes: f64,
    /// The game build icons come from ("1.60.1.70205"); `None` when the
    /// game's data can't be read (no install, or not a CASC install).
    pub build: Option<String>,
    /// This build's art files couldn't be read: the storage wouldn't open,
    /// or every icon tried failed and none is cached. Items show letters.
    pub unreadable: bool,
}

/// What a rebuild read.
#[derive(Debug, Clone, Default, PartialEq, Serialize, specta::Type)]
pub struct IconFill {
    pub read: u32,
    pub failed: u32,
}

enum Job {
    Get {
        flavor: PathBuf,
        id: u32,
        reply: Reply,
    },
    Fill {
        flavor: PathBuf,
        ids: Vec<u32>,
        done: Option<Sender<IconFill>>,
    },
    Clear {
        done: Sender<std::io::Result<()>>,
    },
}

pub struct Icons {
    dir: PathBuf,
    tx: Sender<Job>,
    health: Arc<Mutex<Health>>,
}

impl Icons {
    /// `dir` is the app's own cache folder; `log_dir` gets one line when the
    /// storage won't open.
    pub fn start(dir: PathBuf, log_dir: PathBuf) -> Icons {
        let (tx, rx) = mpsc::channel();
        let mut worker = Worker {
            dir: dir.clone(),
            log_dir,
            open: None,
            open_failed: None,
            failed: HashSet::new(),
            read: 0,
            build: None,
            health: Arc::default(),
        };
        let health = worker.health.clone();
        std::thread::Builder::new()
            .name("icons".into())
            .spawn(move || worker.run(rx))
            .expect("spawn the icons thread");
        Icons { dir, tx, health }
    }

    pub fn get(&self, flavor: PathBuf, id: u32, reply: Reply) {
        if let Err(mpsc::SendError(Job::Get { reply, .. })) =
            self.tx.send(Job::Get { flavor, id, reply })
        {
            reply(None);
        }
    }

    /// Reads whichever of `ids` aren't cached yet, in the background.
    pub fn prefetch(&self, flavor: PathBuf, ids: Vec<u32>) {
        let _ = self.tx.send(Job::Fill {
            flavor,
            ids,
            done: None,
        });
    }

    /// Empties the cache, then reads `ids` again. Waits for it.
    pub fn rebuild(&self, flavor: PathBuf, ids: Vec<u32>) -> std::io::Result<IconFill> {
        self.clear()?;
        let (done, wait) = mpsc::channel();
        let _ = self.tx.send(Job::Fill {
            flavor,
            ids,
            done: Some(done),
        });
        Ok(wait.recv().unwrap_or_default())
    }

    pub fn clear(&self) -> std::io::Result<()> {
        let (done, wait) = mpsc::channel();
        if self.tx.send(Job::Clear { done }).is_err() {
            return Err(std::io::Error::other("the icons thread stopped"));
        }
        wait.recv()
            .unwrap_or_else(|_| Err(std::io::Error::other("the icons thread stopped")))
    }

    pub fn status(&self, flavor: Option<&Path>) -> IconCacheStatus {
        let (mut files, mut bytes) = (0u32, 0u64);
        for entry in walkdir::WalkDir::new(&self.dir).into_iter().flatten() {
            if entry.file_type().is_file() && entry.path().extension().is_some_and(|e| e == "png") {
                files += 1;
                bytes += entry.metadata().map_or(0, |m| m.len());
            }
        }
        let build = flavor.and_then(|f| casc::build_of(f).ok());
        let unreadable = build.as_ref().is_some_and(|b| {
            self.health.lock().is_ok_and(|h| {
                h.build.as_ref() == Some(b)
                    && (h.open_failed || (h.read == 0 && h.failed > 0 && files == 0))
            })
        });
        IconCacheStatus {
            files,
            bytes: bytes as f64,
            build: build.map(|b| b.version),
            unreadable,
        }
    }
}

/// Every icon an item we know of uses: what a rebuild or a prefetch reads.
pub fn known_ids(db: &crate::db::Db) -> crate::error::AppResult<Vec<u32>> {
    db.with_conn(|c| {
        let mut stmt = c.prepare(
            "SELECT DISTINCT icon_file_id FROM items
             WHERE icon_file_id BETWEEN 1 AND 4294967295",
        )?;
        let ids = stmt
            .query_map([], |r| r.get::<_, i64>(0))?
            .filter_map(|r| r.ok().and_then(|i| u32::try_from(i).ok()))
            .collect();
        Ok(ids)
    })
}

struct Worker {
    dir: PathBuf,
    log_dir: PathBuf,
    open: Option<(Casc, Instant)>,
    /// The build whose storage wouldn't open, and when.
    open_failed: Option<(BuildInfo, Instant)>,
    /// Icons that failed for `build`: not tried again until the build
    /// changes or the cache is cleared.
    failed: HashSet<u32>,
    /// Icons read for `build` since it was first seen (or the last clear).
    read: u32,
    build: Option<BuildInfo>,
    health: Arc<Mutex<Health>>,
}

/// The worker's last word on how reading is going, for `status`.
#[derive(Default)]
struct Health {
    build: Option<BuildInfo>,
    open_failed: bool,
    read: u32,
    failed: u32,
}

impl Worker {
    fn run(&mut self, rx: Receiver<Job>) {
        loop {
            let first = match rx.recv_timeout(IDLE / 3) {
                Ok(job) => job,
                Err(RecvTimeoutError::Timeout) => {
                    if self
                        .open
                        .as_ref()
                        .is_some_and(|(_, at)| at.elapsed() > IDLE)
                    {
                        self.open = None;
                    }
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => return,
            };
            // Everything queued behind it goes in the same batch.
            let mut batch = Vec::new();
            for job in std::iter::once(first).chain(rx.try_iter()) {
                if let Job::Clear { done } = job {
                    self.serve(std::mem::take(&mut batch));
                    let _ = done.send(self.clear());
                } else {
                    batch.push(job);
                }
            }
            self.serve(batch);
        }
    }

    fn clear(&mut self) -> std::io::Result<()> {
        self.failed.clear();
        self.open_failed = None;
        self.read = 0;
        self.publish();
        match std::fs::remove_dir_all(&self.dir) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    fn serve(&mut self, jobs: Vec<Job>) {
        // Nearly always one flavor; group in case the install just changed.
        let mut flavors: Vec<PathBuf> = Vec::new();
        for job in &jobs {
            if let Job::Get { flavor, .. } | Job::Fill { flavor, .. } = job {
                if !flavors.contains(flavor) {
                    flavors.push(flavor.clone());
                }
            }
        }
        let mut jobs: Vec<Option<Job>> = jobs.into_iter().map(Some).collect();
        for flavor in flavors {
            let mine: Vec<Job> = jobs
                .iter_mut()
                .filter(|j| {
                    matches!(j, Some(Job::Get { flavor: f, .. } | Job::Fill { flavor: f, .. }) if *f == flavor)
                })
                .filter_map(Option::take)
                .collect();
            self.serve_flavor(&flavor, mine);
        }
    }

    fn serve_flavor(&mut self, flavor: &Path, jobs: Vec<Job>) {
        let Ok(build) = casc::build_of(flavor) else {
            return finish(jobs, None, &IconFill::default());
        };
        self.use_build(&build);
        let cache = IconCache::new(&self.dir);

        let mut wanted: Vec<u32> = Vec::new();
        for job in &jobs {
            let ids = match job {
                Job::Get { id, .. } => std::slice::from_ref(id),
                Job::Fill { ids, .. } => ids.as_slice(),
                Job::Clear { .. } => &[],
            };
            for &id in ids {
                if !wanted.contains(&id)
                    && !self.failed.contains(&id)
                    && cache.cached(&build, id).is_none()
                {
                    wanted.push(id);
                }
            }
        }
        let mut fill = IconFill::default();
        if !wanted.is_empty() {
            match self.casc(flavor, &build) {
                Some(casc) => {
                    for (id, result) in cache.fill(casc, &wanted) {
                        match result {
                            Ok(_) => fill.read += 1,
                            Err(_) => {
                                fill.failed += 1;
                                self.failed.insert(id);
                            }
                        }
                    }
                }
                None => fill.failed = wanted.len() as u32,
            }
        }
        self.read += fill.read;
        self.publish();
        finish(jobs, Some((&cache, &build)), &fill);
    }

    /// What Settings needs to say "couldn't read the game's art files".
    fn publish(&self) {
        if let Ok(mut health) = self.health.lock() {
            *health = Health {
                build: self.build.clone(),
                open_failed: self
                    .open_failed
                    .as_ref()
                    .is_some_and(|(b, _)| Some(b) == self.build.as_ref()),
                read: self.read,
                failed: self.failed.len() as u32,
            };
        }
    }

    /// A new build: forget what failed for the old one and delete its icons.
    fn use_build(&mut self, build: &BuildInfo) {
        if self.build.as_ref() == Some(build) {
            return;
        }
        self.failed.clear();
        self.read = 0;
        self.open = None;
        let keep = casc::cache_path(&self.dir, build, 0)
            .parent()
            .map(Path::to_path_buf);
        for entry in std::fs::read_dir(&self.dir).into_iter().flatten().flatten() {
            let path = entry.path();
            let ours = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.len() == 32 && n.bytes().all(|b| b.is_ascii_hexdigit()));
            if ours && Some(&path) != keep.as_ref() && path.is_dir() {
                let _ = std::fs::remove_dir_all(&path);
            }
        }
        self.build = Some(build.clone());
    }

    fn casc(&mut self, flavor: &Path, build: &BuildInfo) -> Option<&Casc> {
        if self.open.as_ref().is_some_and(|(c, _)| c.build != *build) {
            self.open = None;
        }
        if self.open.is_none() {
            if let Some((b, at)) = &self.open_failed {
                if b == build && at.elapsed() < RETRY_OPEN {
                    return None;
                }
            }
            match Casc::open(flavor) {
                Ok(casc) => {
                    self.open_failed = None;
                    self.open = Some((casc, Instant::now()));
                }
                Err(e) => {
                    crate::applog::append(
                        &self.log_dir,
                        &format!("icons: couldn't read the game's data ({e})"),
                    );
                    self.open_failed = Some((build.clone(), Instant::now()));
                    return None;
                }
            }
        }
        let (casc, used) = self.open.as_mut()?;
        *used = Instant::now();
        Some(casc)
    }
}

/// Replies to every job: each `Get` with its PNG if it's cached now.
fn finish(jobs: Vec<Job>, cache: Option<(&IconCache, &BuildInfo)>, fill: &IconFill) {
    for job in jobs {
        match job {
            Job::Get { id, reply, .. } => reply(
                cache
                    .and_then(|(c, b)| c.cached(b, id))
                    .and_then(|p| std::fs::read(p).ok()),
            ),
            Job::Fill {
                done: Some(done), ..
            } => {
                let _ = done.send(fill.clone());
            }
            Job::Fill { done: None, .. } | Job::Clear { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::casc::tests::{install, ICON, LOCKED};

    fn get(icons: &Icons, flavor: &Path, id: u32) -> Option<Vec<u8>> {
        let (tx, rx) = mpsc::channel();
        icons.get(
            flavor.to_path_buf(),
            id,
            Box::new(move |png| {
                let _ = tx.send(png);
            }),
        );
        rx.recv().unwrap()
    }

    #[test]
    fn serves_icons_and_no_icon_for_the_rest() {
        let game = install();
        let tmp = tempfile::tempdir().unwrap();
        let icons = Icons::start(tmp.path().join("icons"), tmp.path().to_path_buf());

        let png = get(&icons, &game.flavor, ICON).unwrap();
        assert!(png.starts_with(b"\x89PNG"));
        assert_eq!(get(&icons, &game.flavor, LOCKED), None);
        assert_eq!(get(&icons, &game.flavor, 999), None);

        let status = icons.status(Some(&game.flavor));
        assert_eq!(status.files, 1);
        assert!(status.bytes > 0.0);
        assert_eq!(status.build.as_deref(), Some("1.60.1.70205"));
        // Some icons failing isn't "couldn't read the game's art files".
        assert!(!status.unreadable);

        // A cached icon is served without the archive.
        std::fs::remove_file(game.data.join("data").join("data.000")).unwrap();
        assert!(get(&icons, &game.flavor, ICON).is_some());
    }

    #[test]
    fn no_install_or_no_data_is_no_icon() {
        let tmp = tempfile::tempdir().unwrap();
        let icons = Icons::start(tmp.path().join("icons"), tmp.path().to_path_buf());
        assert_eq!(get(&icons, &tmp.path().join("_classic_beta_"), ICON), None);

        assert!(
            !icons.status(None).unreadable,
            "no install isn't a read failure"
        );

        let game = install();
        std::fs::remove_dir_all(game.data.join("data")).unwrap();
        assert_eq!(get(&icons, &game.flavor, ICON), None);
        assert!(icons.status(Some(&game.flavor)).unreadable);
        // Clear (or Rebuild) forgets it, so the next try can succeed.
        icons.clear().unwrap();
        assert!(!icons.status(Some(&game.flavor)).unreadable);
        assert!(std::fs::read_to_string(tmp.path().join("buddy.log"))
            .unwrap()
            .contains("icons: couldn't read the game's data"));
    }

    #[test]
    fn rebuild_and_clear() {
        let game = install();
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("icons");
        let icons = Icons::start(dir.clone(), tmp.path().to_path_buf());

        let fill = icons
            .rebuild(game.flavor.clone(), vec![ICON, LOCKED, ICON])
            .unwrap();
        assert_eq!(fill, IconFill { read: 1, failed: 1 });
        assert_eq!(icons.status(None).files, 1);

        icons.clear().unwrap();
        assert_eq!(icons.status(None).files, 0);
        assert!(!dir.exists());
        // And it fills again on the next request.
        assert!(get(&icons, &game.flavor, ICON).is_some());
    }

    #[test]
    fn another_builds_icons_are_pruned_and_nothing_else() {
        let game = install();
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("icons");
        let old = dir.join("ffffffffffffffffffffffffffffffff");
        let other = dir.join("not-a-build");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(old.join("1.png"), b"old").unwrap();

        let icons = Icons::start(dir.clone(), tmp.path().to_path_buf());
        assert!(get(&icons, &game.flavor, ICON).is_some());
        assert!(!old.exists());
        assert!(other.exists());
    }
}
