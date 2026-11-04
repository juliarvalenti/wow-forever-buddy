//! The live preview (PV1): the player's screen as the game would draw it
//! with our addon and their settings, redrawn when a setting changes, in
//! the Preview tab.
//!
//! The WOWAPI emulator and the logged-in scene from `tools/` run in a
//! sandboxed Lua state (`sandbox`) on their own thread, compiled into the
//! app (`bundle`). WOWAPI's data folder is downloaded on the player's click
//! and checked against hashes the app carries (`data`). With the art switch
//! off (the default) nothing reads the game's files; Blizzard's own frames
//! are stand-ins and ours draw in full.
//!
//! Nothing runs until the Preview tab asks for a frame. After that, every
//! change that would send the game new settings asks for a redraw too, and
//! redraws that queue up are drawn once, with the newest settings.

pub mod bundle;
pub mod data;
pub mod sandbox;
pub mod ui;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use sandbox::{DrawOptions, Host, Session};

/// The moment the Your UI tab shows (ED1): the logged-in scene, or the same
/// scene in combat, in stealth or at low health (`tools/addon-shots/preview.lua`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Moment {
    #[default]
    Standing,
    Combat,
    Stealth,
    LowHealth,
}

impl Moment {
    /// The name `preview.lua` knows it by.
    pub fn as_str(self) -> &'static str {
        match self {
            Moment::Standing => "standing",
            Moment::Combat => "combat",
            Moment::Stealth => "stealth",
            Moment::LowHealth => "low_health",
        }
    }
}

/// A draw that takes longer than this is stopped (a full-art screen with
/// Blizzard's UI takes several seconds).
const DRAW_LIMIT: Duration = Duration::from_secs(90);

/// Where WOWAPI's data stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum PreviewData {
    /// Not downloaded: the tab offers the download.
    Missing,
    Downloading,
    Ready,
    /// The download failed (`data_error` says why); the tab offers a retry.
    Failed,
}

/// The Preview tab's state. The frame itself comes from `frame()`.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct PreviewStatus {
    pub data: PreviewData,
    pub data_error: Option<String>,
    /// For the button: "Download preview data (8 MB)".
    pub download_bytes: f64,
    pub downloaded_bytes: f64,
    /// A redraw is under way (the tab keeps the last frame meanwhile).
    pub drawing: bool,
    /// Bumps with each new frame.
    pub generation: u32,
    pub drew_ms: Option<u32>,
    /// The scene's Lua errors in the last frame (WOWAPI's gaps or ours).
    pub lua_errors: u32,
    /// The fixture character drawn ("rogue"), or `None` for the generic one.
    pub class: Option<String>,
    /// The last draw failed; the tab shows this under the last good frame.
    pub draw_error: Option<String>,
    /// The frame is only the backdrop: none of our frames are on at login.
    pub blank: bool,
    /// The moment the last frame shows.
    pub moment: Moment,
    /// The UI draft the last frame shows, or `None` for what's in game.
    pub draft: Option<u32>,
}

/// The frame as the tab shows it: a `data:` URL, which the app's CSP lets an
/// `<img>` load (BUG-PREVIEW-IMG: a `blob:` URL was refused). An SVG shown
/// as an image runs no script.
pub fn frame_url(svg: &str) -> String {
    use base64::Engine;
    format!(
        "data:image/svg+xml;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(svg)
    )
}

/// What a draw needs from the app.
#[derive(Debug, Clone, PartialEq)]
pub struct Inputs {
    /// The addon's slot files the app would send the game, by their path in
    /// the addon folder ("Data/Settings.lua").
    pub slots: BTreeMap<String, Vec<u8>>,
    /// The last-played character's class ("ROGUE").
    pub class: Option<String>,
    /// The player's extracted game files: only with the art switch on.
    pub extract: Option<PathBuf>,
    /// The moment to draw; `Preview` sets it from the tab's choice.
    pub moment: Moment,
    /// The UI draft (ED1) whose settings `slots` carry, instead of the
    /// approved ones; `None` for what's in game.
    pub draft: Option<u32>,
}

enum Job {
    Draw(Inputs),
    Download,
    Remove(Sender<std::io::Result<()>>),
}

type Notify = Box<dyn Fn() + Send + Sync>;

struct Shared {
    status: Mutex<PreviewStatus>,
    frame: Mutex<Option<String>>,
    notify: Mutex<Option<Notify>>,
}

impl Shared {
    fn update(&self, f: impl FnOnce(&mut PreviewStatus)) {
        f(&mut self.status.lock().expect("preview status lock"));
        if let Some(n) = self.notify.lock().expect("preview notify lock").as_ref() {
            n();
        }
    }
}

pub struct Preview {
    tx: Sender<Job>,
    shared: Arc<Shared>,
    started: Mutex<bool>,
    /// The tab's moment: kept here so a setting change redraws the same one.
    moment: Mutex<Moment>,
    /// The draft the tab picked (ED1), likewise; `None` for what's in game.
    draft: Mutex<Option<u32>>,
}

impl Preview {
    /// `dir` is the preview's own folder in the app's local data: `data/`
    /// for WOWAPI's data, `scratch/` for the Lua's writes. `log_dir` gets a
    /// line per draw with its time.
    pub fn start(dir: PathBuf, log_dir: PathBuf) -> Preview {
        let data_dir = dir.join("data");
        let ready = data::is_ready(&data_dir);
        let shared = Arc::new(Shared {
            status: Mutex::new(PreviewStatus {
                data: if ready {
                    PreviewData::Ready
                } else {
                    PreviewData::Missing
                },
                data_error: None,
                download_bytes: data::total_bytes() as f64,
                downloaded_bytes: 0.0,
                drawing: false,
                generation: 0,
                drew_ms: None,
                lua_errors: 0,
                class: None,
                draw_error: None,
                blank: false,
                moment: Moment::Standing,
                draft: None,
            }),
            frame: Mutex::new(None),
            notify: Mutex::new(None),
        });
        let (tx, rx) = mpsc::channel();
        let worker = Worker {
            dir,
            data_dir,
            log_dir,
            shared: shared.clone(),
            host: None,
            last: None,
        };
        std::thread::Builder::new()
            .name("preview".into())
            .spawn(move || worker.run(rx))
            .expect("spawn the preview thread");
        Preview {
            tx,
            shared,
            started: Mutex::new(false),
            moment: Mutex::new(Moment::Standing),
            draft: Mutex::new(None),
        }
    }

    /// The draft the tab shows, for `AppCore::preview_inputs`.
    pub fn draft(&self) -> Option<u32> {
        *self.draft.lock().expect("preview draft lock")
    }

    /// The tab picked a draft, or what's in game (`None`). The caller then
    /// opens the preview with inputs that carry it.
    pub fn set_draft(&self, draft: Option<u32>) {
        *self.draft.lock().expect("preview draft lock") = draft;
    }

    /// Called on each status change (the app emits `PreviewChanged`).
    pub fn on_change(&self, f: impl Fn() + Send + Sync + 'static) {
        *self.shared.notify.lock().expect("preview notify lock") = Some(Box::new(f));
    }

    pub fn status(&self) -> PreviewStatus {
        self.shared
            .status
            .lock()
            .expect("preview status lock")
            .clone()
    }

    pub fn frame(&self) -> Option<String> {
        self.shared
            .frame
            .lock()
            .expect("preview frame lock")
            .clone()
    }

    /// The tab asked for `moment` (or the one it last chose): from now on,
    /// setting changes redraw too.
    pub fn open(&self, mut inputs: Inputs, moment: Option<Moment>) {
        *self.started.lock().expect("preview started lock") = true;
        let mut m = self.moment.lock().expect("preview moment lock");
        if let Some(moment) = moment {
            *m = moment;
        }
        inputs.moment = *m;
        let _ = self.tx.send(Job::Draw(inputs));
    }

    /// A setting changed: redraw the tab's moment, if it has ever been opened.
    pub fn changed(&self, mut inputs: Inputs) {
        if *self.started.lock().expect("preview started lock") {
            inputs.moment = *self.moment.lock().expect("preview moment lock");
            let _ = self.tx.send(Job::Draw(inputs));
        }
    }

    pub fn download(&self) {
        let _ = self.tx.send(Job::Download);
    }

    /// Settings > "Remove preview data".
    pub fn remove_data(&self) -> std::io::Result<()> {
        let (done, wait) = mpsc::channel();
        let _ = self.tx.send(Job::Remove(done));
        wait.recv().unwrap_or(Ok(()))
    }
}

struct Worker {
    dir: PathBuf,
    data_dir: PathBuf,
    log_dir: PathBuf,
    shared: Arc<Shared>,
    host: Option<Arc<RwLock<Host>>>,
    last: Option<Inputs>,
}

thread_local! {
    /// The Lua state lives on the preview thread (it can't move between
    /// threads), kept between draws so the modules stay loaded.
    static SESSION: std::cell::RefCell<Option<Session>> = const { std::cell::RefCell::new(None) };
}

fn drop_session() {
    SESSION.with(|s| *s.borrow_mut() = None);
}

impl Worker {
    fn run(mut self, rx: Receiver<Job>) {
        while let Ok(job) = rx.recv() {
            match job {
                Job::Draw(mut inputs) => {
                    // Draw once with the newest of the queued requests.
                    let mut later = Vec::new();
                    while let Ok(next) = rx.try_recv() {
                        match next {
                            Job::Draw(newer) => inputs = newer,
                            other => later.push(other),
                        }
                    }
                    self.draw(inputs);
                    for job in later {
                        self.other(job);
                    }
                }
                other => self.other(other),
            }
        }
    }

    fn other(&mut self, job: Job) {
        match job {
            Job::Download => self.download(),
            Job::Remove(done) => {
                drop_session();
                self.host = None;
                let r = data::remove(&self.data_dir);
                *self.shared.frame.lock().expect("preview frame lock") = None;
                self.shared.update(|s| {
                    s.data = PreviewData::Missing;
                    s.downloaded_bytes = 0.0;
                    s.generation += 1;
                });
                let _ = done.send(r);
            }
            Job::Draw(inputs) => self.draw(inputs),
        }
    }

    fn download(&mut self) {
        self.shared.update(|s| {
            s.data = PreviewData::Downloading;
            s.data_error = None;
            s.downloaded_bytes = 0.0;
        });
        let shared = self.shared.clone();
        let result = data::download(&self.data_dir, |done| {
            shared.update(|s| s.downloaded_bytes = done as f64)
        });
        match result {
            Ok(()) => {
                self.shared.update(|s| s.data = PreviewData::Ready);
                if let Some(inputs) = self.last.clone() {
                    self.draw(inputs);
                }
            }
            Err(e) => {
                crate::applog::append(&self.log_dir, &format!("preview: {e}"));
                self.shared.update(|s| {
                    s.data = PreviewData::Failed;
                    s.data_error = Some(e.to_string());
                });
            }
        }
    }

    fn draw(&mut self, inputs: Inputs) {
        self.last = Some(inputs.clone());
        if self.shared.status.lock().expect("preview status lock").data != PreviewData::Ready {
            return;
        }
        self.shared.update(|s| s.drawing = true);
        let started = Instant::now();
        let result = self.draw_inner(&inputs);
        let ms = started.elapsed().as_millis().min(u32::MAX as u128) as u32;
        let first = self
            .shared
            .status
            .lock()
            .expect("preview status lock")
            .generation
            == 0;
        match result {
            Ok(drawn) => {
                crate::applog::append(
                    &self.log_dir,
                    &format!(
                        "preview: {} frame in {ms} ms ({} KB, {} Lua errors{})",
                        if first { "first" } else { "new" },
                        drawn.svg.len() / 1024,
                        drawn.lua_errors,
                        if inputs.extract.is_some() {
                            ", game art"
                        } else {
                            ""
                        }
                    ),
                );
                let blank = drawn.svg.len() < 1024;
                *self.shared.frame.lock().expect("preview frame lock") = Some(drawn.svg);
                self.shared.update(|s| {
                    s.drawing = false;
                    s.blank = blank;
                    s.generation += 1;
                    s.drew_ms = Some(ms);
                    s.lua_errors = drawn.lua_errors;
                    s.class = drawn.class;
                    s.moment = inputs.moment;
                    s.draft = inputs.draft;
                    s.draw_error = None;
                });
            }
            Err(e) => {
                crate::applog::append(
                    &self.log_dir,
                    &format!("preview: the draw failed after {ms} ms: {e}"),
                );
                // A failed state isn't reused.
                drop_session();
                self.host = None;
                self.shared.update(|s| {
                    s.drawing = false;
                    s.draw_error = Some(e);
                });
            }
        }
    }

    fn draw_inner(&mut self, inputs: &Inputs) -> Result<sandbox::Drawn, String> {
        let scratch = self.dir.join("scratch");
        std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
        let host = self.host.get_or_insert_with(|| {
            Arc::new(RwLock::new(Host {
                data: None,
                scratch: scratch.clone(),
                extract: None,
                slots: BTreeMap::new(),
            }))
        });
        {
            let mut h = host.write().expect("preview host lock");
            h.data = Some(self.data_dir.clone());
            h.extract = inputs.extract.clone();
            h.slots = inputs.slots.clone();
        }
        let opts = DrawOptions {
            class: inputs.class.clone(),
            art: inputs.extract.is_some(),
            moment: inputs.moment.as_str(),
            ui: None,
        };
        let host = host.clone();
        SESSION.with(|s| {
            let mut s = s.borrow_mut();
            if s.is_none() {
                *s = Some(Session::new(host).map_err(|e| e.to_string())?);
            }
            s.as_ref().expect("just made").draw(&opts, DRAW_LIMIT)
        })
    }
}

/// The preview's folder in the app's local data.
pub fn dir(local_data: &Path) -> PathBuf {
    local_data.join("preview")
}

/// The addon with every slot the app would send the game now (so its
/// settings profile is the newest one approved), and the last-played
/// character's class. With a draft (ED1: its id and changes), the Settings
/// slot is the one it would make instead: drawn here, never written.
pub fn inputs(
    db: &crate::db::Db,
    flavor: &str,
    now: i64,
    draft: Option<(u32, Vec<crate::addon_settings::SettingBody>)>,
) -> crate::error::AppResult<Inputs> {
    let mut built = crate::bridge::build(db, flavor, now, &crate::bridge::SLOTS)?.slots;
    let draft = match draft {
        Some((id, changes)) => {
            let entries = crate::addon_settings::draft_slot_entries(db, flavor, &changes)?;
            let slot = crate::bridge::settings_slot(entries, now)?;
            built.retain(|(s, _)| *s != slot.0);
            built.push(slot);
            Some(id)
        }
        None => None,
    };
    let prefix = format!("{}/", crate::addon::FOLDER);
    let slots = built
        .into_iter()
        .filter_map(|(slot, bytes)| {
            let path = slot.path().as_string();
            path.strip_prefix(&prefix).map(|p| (p.to_string(), bytes))
        })
        .collect();
    let class = crate::characters::overview(db, flavor)?
        .characters
        .into_iter()
        .next()
        .and_then(|c| c.class);
    // Blizzard's art and UI need an in-app extract of the player's game
    // files, which comes with its own switch (PV1b); until then the preview
    // never reads them (F8c).
    Ok(Inputs {
        slots,
        class,
        extract: None,
        moment: Moment::Standing,
        draft,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    const FLAVOR: &str = "_classic_beta_";

    /// BUG-PREVIEW-IMG: the tab showed a broken image because its `blob:`
    /// URL isn't in the CSP's img-src. The URL the tab gets must be.
    #[test]
    fn the_frame_url_is_one_the_csp_lets_an_image_load() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap();
        let csp = conf["app"]["security"]["csp"].as_str().expect("a CSP");
        let img_src: Vec<&str> = csp
            .split(';')
            .map(str::trim)
            .find_map(|d| d.strip_prefix("img-src "))
            .expect("an img-src")
            .split_whitespace()
            .collect();
        let url = frame_url("<svg xmlns='http://www.w3.org/2000/svg'/>");
        let scheme = format!("{}:", url.split(':').next().unwrap());
        assert!(
            img_src.contains(&scheme.as_str()),
            "the tab's frame is a {scheme} URL, which img-src ({img_src:?}) refuses"
        );
    }

    /// A preview whose data folder already holds WOWAPI's data (as a player's
    /// would after the download), and a way to wait for its next frame.
    fn started(tmp: &Path) -> (Preview, mpsc::Receiver<()>) {
        let dir = tmp.join("preview");
        let data = dir.join("data");
        std::fs::create_dir_all(&data).unwrap();
        let repo = sandbox::tests::repo_data();
        for f in &data::FILES {
            std::fs::copy(repo.join(f.name), data.join(f.name)).unwrap();
        }
        let preview = Preview::start(dir, tmp.join("logs"));
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        preview.on_change(move || {
            let _ = tx.lock().unwrap().send(());
        });
        (preview, rx)
    }

    fn next_frame(
        preview: &Preview,
        rx: &mpsc::Receiver<()>,
        after: u32,
    ) -> (PreviewStatus, String) {
        let until = Instant::now() + Duration::from_secs(120);
        loop {
            let s = preview.status();
            if s.generation > after && !s.drawing {
                return (s, preview.frame().expect("a frame"));
            }
            assert!(s.draw_error.is_none(), "{:?}", s.draw_error);
            let left = until.saturating_duration_since(Instant::now());
            assert!(!left.is_zero(), "no new frame: {s:?}");
            let _ = rx.recv_timeout(left);
        }
    }

    /// The ticket's check: set a value, get a changed frame back.
    #[test]
    fn an_approved_setting_redraws_the_preview() {
        let tmp = tempfile::tempdir().unwrap();
        let db = crate::db::Db::open(&tmp.path().join("app.db")).unwrap();
        let (preview, rx) = started(tmp.path());
        assert_eq!(preview.status().data, PreviewData::Ready);
        let rogue = |mut i: Inputs| {
            i.class = Some("ROGUE".into());
            i
        };

        // Nothing changed yet: the settings slot is the shipped stub, and our
        // rogue HUD is off.
        preview.open(rogue(inputs(&db, FLAVOR, 1, None).unwrap()), None);
        let (first, before) = next_frame(&preview, &rx, 0);
        assert_eq!(first.class.as_deref(), Some("rogue"));
        assert!(!before.contains("Syndicate Highwayman"));

        // ED1: a draft that turns the HUD on is drawn with it, and nothing
        // is written: the slot the game would get still has no profile.
        let draft = vec![crate::addon_settings::SettingBody {
            option: "hud.enabled".into(),
            value: serde_json::Value::Bool(true),
        }];
        let with = inputs(&db, FLAVOR, 1, Some((7, draft))).unwrap();
        assert_eq!(with.draft, Some(7));
        preview.open(rogue(with), None);
        let (drafted, frame) = next_frame(&preview, &rx, first.generation);
        assert_eq!(drafted.draft, Some(7));
        assert!(
            frame.contains("Syndicate Highwayman"),
            "the draft's HUD is drawn"
        );
        assert!(crate::addon_settings::slot_entries(&db, FLAVOR)
            .unwrap()
            .is_empty());
        preview.open(rogue(inputs(&db, FLAVOR, 1, None).unwrap()), None);
        let (first, _) = next_frame(&preview, &rx, drafted.generation);
        assert_eq!(first.draft, None);

        // Approving "hud.enabled" makes a profile the Settings slot carries;
        // the redraw has the HUD.
        crate::addon_settings::approve(
            &db,
            FLAVOR,
            &crate::addon_settings::SettingBody {
                option: "hud.enabled".into(),
                value: serde_json::Value::Bool(true),
            },
            2,
        )
        .unwrap();
        preview.changed(rogue(inputs(&db, FLAVOR, 2, None).unwrap()));
        let (second, after) = next_frame(&preview, &rx, first.generation);
        assert!(
            after.contains("Syndicate Highwayman"),
            "the HUD should show the target"
        );
        assert!(
            after.contains("data:font/ttf;base64,"),
            "the HUD's own font (Barlow) should be embedded"
        );
        assert_ne!(before, after);
        eprintln!(
            "preview: first frame {:?} ms, redraw {:?} ms",
            first.drew_ms, second.drew_ms
        );
    }

    #[test]
    fn without_data_nothing_draws_and_the_tab_offers_the_download() {
        let tmp = tempfile::tempdir().unwrap();
        let preview = Preview::start(tmp.path().join("preview"), tmp.path().join("logs"));
        let s = preview.status();
        assert_eq!(s.data, PreviewData::Missing);
        assert_eq!(s.download_bytes as u64, data::total_bytes());
        preview.open(
            Inputs {
                slots: BTreeMap::new(),
                class: None,
                extract: None,
                moment: Moment::Standing,
                draft: None,
            },
            None,
        );
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(preview.status().generation, 0);
        assert!(preview.frame().is_none());
    }
}
