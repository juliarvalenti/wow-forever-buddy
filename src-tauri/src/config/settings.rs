use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::config::migrate::{self, CURRENT_SCHEMA_VERSION};
use crate::error::{AppError, AppResult};
use crate::fsx::atomic::atomic_replace;

/// settings.json (spec §6). Every field has a default, so a missing key never
/// fails a load, and unknown top-level keys survive in `extra` so running an
/// older build doesn't wipe settings a newer one wrote.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct Settings {
    /// Backend-owned: set by load/migrate, ignored on update.
    pub schema_version: u32,
    /// Backend-owned: changed only through install commands, which validate it.
    pub install: Option<InstallChoice>,
    pub backup: BackupSettings,
    /// Extra game process names to treat as WoW, e.g. if Forever's exe has an unusual name.
    pub process_names_extra: Vec<String>,
    pub integrations: Integrations,
    /// Free-form UI preferences (remembered tabs, filters). The backend never reads it.
    /// Strings only; the frontend JSON-encodes anything structured.
    pub ui: BTreeMap<String, String>,
    #[serde(flatten)]
    #[specta(skip)]
    pub extra: Map<String, Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: CURRENT_SCHEMA_VERSION,
            install: None,
            backup: BackupSettings::default(),
            process_names_extra: Vec::new(),
            integrations: Integrations::default(),
            ui: BTreeMap::new(),
            extra: Map::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct InstallChoice {
    pub root: PathBuf,
    /// Flavor folder name, e.g. "_classic_".
    pub flavor: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct BackupSettings {
    /// None = the default, `<local_data_dir>/backups`.
    pub location: Option<PathBuf>,
    pub include_addons: bool,
    pub on_app_start: bool,
    pub on_game_exit: bool,
    /// None = no scheduled backups.
    pub schedule_hours: Option<u32>,
}

impl Default for BackupSettings {
    fn default() -> Self {
        Self {
            location: None,
            include_addons: false,
            on_app_start: true,
            on_game_exit: true,
            schedule_hours: Some(24),
        }
    }
}

/// A closed set: each optional integration is a named field, never a free-form key.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct Integrations {
    pub curseforge: IntegrationSetting,
    pub wago: IntegrationSetting,
    pub wago_io: IntegrationSetting,
    pub github: IntegrationSetting,
    pub battlenet: IntegrationSetting,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct IntegrationSetting {
    pub enabled: bool,
}

const MAX_SCHEDULE_HOURS: u32 = 24 * 7;

impl Settings {
    /// Checks user-editable fields and normalizes them in place.
    fn validate(&mut self) -> AppResult<()> {
        if let Some(h) = self.backup.schedule_hours {
            if !(1..=MAX_SCHEDULE_HOURS).contains(&h) {
                return Err(AppError::InvalidSettings(format!(
                    "backup schedule must be 1–{MAX_SCHEDULE_HOURS} hours, got {h}"
                )));
            }
        }
        if let Some(loc) = &self.backup.location {
            if !loc.is_absolute() {
                return Err(AppError::InvalidSettings(format!(
                    "backup location must be an absolute path: {}",
                    loc.display()
                )));
            }
        }

        let mut names: Vec<String> = Vec::new();
        for name in &self.process_names_extra {
            let name = name.trim();
            if name.is_empty() {
                continue;
            }
            if name.contains(['/', '\\']) {
                return Err(AppError::InvalidSettings(format!(
                    "process name must be a file name, not a path: {name}"
                )));
            }
            if !names.iter().any(|n| n.eq_ignore_ascii_case(name)) {
                names.push(name.to_string());
            }
        }
        self.process_names_extra = names;
        Ok(())
    }
}

/// Owns settings.json: loads (with migration and corruption recovery), hands
/// out copies, and saves atomically on every change.
pub struct SettingsStore {
    path: PathBuf,
    current: RwLock<Settings>,
}

impl SettingsStore {
    pub fn load(path: PathBuf) -> AppResult<Self> {
        let settings = match std::fs::read(&path) {
            Ok(bytes) => Self::parse_or_recover(&path, &bytes)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Settings::default(),
            Err(e) => return Err(e.into()),
        };
        let store = Self {
            path,
            current: RwLock::new(settings),
        };
        store.save(&store.get())?;
        Ok(store)
    }

    fn parse_or_recover(path: &Path, bytes: &[u8]) -> AppResult<Settings> {
        let parsed = serde_json::from_slice::<Value>(bytes)
            .map_err(|e| e.to_string())
            .and_then(|value| migrate::run(value, path).map_err(|e| e.to_string()))
            .and_then(|value| serde_json::from_value::<Settings>(value).map_err(|e| e.to_string()))
            .and_then(|mut s| s.validate().map(|_| s).map_err(|e| e.to_string()));

        match parsed {
            Ok(s) => Ok(s),
            Err(_) => {
                // Keep the broken file for inspection and start from defaults
                // rather than refusing to launch.
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let quarantine = path.with_file_name(format!("settings.corrupt-{stamp}.json"));
                std::fs::rename(path, &quarantine)?;
                Ok(Settings::default())
            }
        }
    }

    pub fn get(&self) -> Settings {
        self.current.read().expect("settings lock poisoned").clone()
    }

    /// Applies a full settings object from the UI. Backend-owned fields
    /// (`schema_version`, `install`) and unknown keys keep their current values.
    pub fn update_from_user(&self, incoming: Settings) -> AppResult<Settings> {
        self.update(|current| {
            let mut next = incoming;
            next.schema_version = current.schema_version;
            next.install = current.install.clone();
            next.extra = current.extra.clone();
            *current = next;
        })
    }

    /// Backend-side change (e.g. install commands). Validated and saved like any other.
    pub fn update(&self, change: impl FnOnce(&mut Settings)) -> AppResult<Settings> {
        let mut guard = self.current.write().expect("settings lock poisoned");
        let mut next = guard.clone();
        change(&mut next);
        next.validate()?;
        self.save(&next)?;
        *guard = next.clone();
        Ok(next)
    }

    fn save(&self, settings: &Settings) -> AppResult<()> {
        let json = serde_json::to_vec_pretty(settings)
            .map_err(|e| AppError::Io(format!("serialize settings: {e}")))?;
        atomic_replace(&self.path, &json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_in(dir: &Path) -> SettingsStore {
        SettingsStore::load(dir.join("settings.json")).unwrap()
    }

    fn on_disk(dir: &Path) -> Value {
        serde_json::from_slice(&std::fs::read(dir.join("settings.json")).unwrap()).unwrap()
    }

    #[test]
    fn missing_file_gives_defaults_and_writes_them() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store_in(tmp.path());
        assert_eq!(store.get(), Settings::default());
        assert_eq!(
            on_disk(tmp.path())["schema_version"],
            CURRENT_SCHEMA_VERSION
        );
        assert_eq!(on_disk(tmp.path())["backup"]["schedule_hours"], 24);
    }

    #[test]
    fn missing_keys_get_defaults_and_unknown_keys_survive() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("settings.json"),
            r#"{ "schema_version": 1, "backup": { "include_addons": true }, "from_the_future": [1, 2] }"#,
        )
        .unwrap();
        let store = store_in(tmp.path());
        let s = store.get();
        assert!(s.backup.include_addons);
        assert!(
            s.backup.on_game_exit,
            "missing nested key falls back to default"
        );
        assert_eq!(
            on_disk(tmp.path())["from_the_future"],
            serde_json::json!([1, 2])
        );
    }

    #[test]
    fn corrupt_file_is_quarantined_not_fatal() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("settings.json"), b"{ not json").unwrap();
        let store = store_in(tmp.path());
        assert_eq!(store.get(), Settings::default());
        let quarantined = std::fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("settings.corrupt-")
            });
        assert!(quarantined);
    }

    #[test]
    fn user_update_cannot_touch_backend_owned_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store_in(tmp.path());
        let choice = InstallChoice {
            root: PathBuf::from("/games/wow"),
            flavor: "_classic_".into(),
        };
        store.update(|s| s.install = Some(choice.clone())).unwrap();

        let mut incoming = store.get();
        incoming.install = None;
        incoming.schema_version = 99;
        incoming.backup.include_addons = true;
        let saved = store.update_from_user(incoming).unwrap();

        assert_eq!(saved.install, Some(choice));
        assert_eq!(saved.schema_version, CURRENT_SCHEMA_VERSION);
        assert!(saved.backup.include_addons);
        assert_eq!(on_disk(tmp.path())["backup"]["include_addons"], true);
    }

    #[test]
    fn invalid_update_is_rejected_and_not_saved() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store_in(tmp.path());

        let mut bad = store.get();
        bad.backup.schedule_hours = Some(0);
        assert!(matches!(
            store.update_from_user(bad),
            Err(AppError::InvalidSettings(_))
        ));

        let mut bad = store.get();
        bad.backup.location = Some(PathBuf::from("relative/dir"));
        assert!(store.update_from_user(bad).is_err());

        let mut bad = store.get();
        bad.process_names_extra = vec!["C:\\Games\\Wow.exe".into()];
        assert!(store.update_from_user(bad).is_err());

        assert_eq!(store.get(), Settings::default());
        assert_eq!(on_disk(tmp.path())["backup"]["schedule_hours"], 24);
    }

    #[test]
    fn process_names_are_trimmed_and_deduped() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store_in(tmp.path());
        let mut s = store.get();
        s.process_names_extra = vec![
            " WowForever.exe ".into(),
            "".into(),
            "wowforever.EXE".into(),
        ];
        let saved = store.update_from_user(s).unwrap();
        assert_eq!(
            saved.process_names_extra,
            vec!["WowForever.exe".to_string()]
        );
    }
}
