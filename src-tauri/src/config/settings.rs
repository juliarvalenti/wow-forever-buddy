use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::config::migrate::{self, CURRENT_SCHEMA_VERSION};
use crate::error::{AppError, AppResult};
use crate::fsx::atomic::atomic_replace;
use crate::fsx::relpath::LinkedFolder;

/// settings.json (spec §6). Every field has a default, so a missing key never
/// fails a load. Keys this build doesn't know are kept on disk by
/// `SettingsStore`, at any depth, so a downgrade never wipes them.
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
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct InstallChoice {
    pub root: PathBuf,
    /// Flavor folder name, e.g. "_classic_beta_".
    pub flavor: String,
    /// Where the flavor's linked folders (WTF, Interface/AddOns) pointed when
    /// the user confirmed this install. Recorded once by `install::set`; game
    /// paths are checked against these, so a link re-pointed later is refused
    /// instead of silently followed.
    #[serde(default)]
    pub links: Vec<LinkedFolder>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct BackupSettings {
    /// None = the default, `<local_data_dir>/backups`.
    pub location: Option<PathBuf>,
    pub include_addons: bool,
    pub on_app_start: bool,
    pub on_game_exit: bool,
    /// Hours between scheduled backups while the app is open; 0 = off.
    /// Off is an explicit value so that a missing or null key always means
    /// "use the default", never "turned off".
    pub schedule_hours: u32,
}

impl Default for BackupSettings {
    fn default() -> Self {
        Self {
            location: None,
            include_addons: false,
            on_app_start: true,
            on_game_exit: true,
            schedule_hours: 24,
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
        let h = self.backup.schedule_hours;
        if h > MAX_SCHEDULE_HOURS {
            return Err(AppError::InvalidSettings(format!(
                "backup schedule must be 1–{MAX_SCHEDULE_HOURS} hours (or 0 for off), got {h}"
            )));
        }
        if let Some(loc) = &self.backup.location {
            if !loc.is_absolute() {
                return Err(AppError::InvalidSettings(format!(
                    "backup location must be an absolute path: {}",
                    loc.display()
                )));
            }
            // Spec §6: never inside the game folder, or every full snapshot
            // would capture the backup store itself.
            if let Some(install) = &self.install {
                let lower = |p: &Path| p.to_string_lossy().replace('\\', "/").to_lowercase();
                let root = lower(&install.root);
                let loc_str = lower(loc);
                if loc_str == root
                    || loc_str.starts_with(&format!("{}/", root.trim_end_matches('/')))
                {
                    return Err(AppError::InvalidSettings(format!(
                        "backup location can't be inside the game folder: {}",
                        loc.display()
                    )));
                }
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

/// A partial update from the UI (spec §8 `settings_update(patch)`): only the
/// fields present change, so a stale copy of the settings can't overwrite
/// newer values. There are no `install` or `schema_version` fields, and
/// unknown fields are rejected, so a patch can't touch backend-owned state.
#[derive(Debug, Clone, Default, Deserialize, specta::Type)]
#[serde(default, deny_unknown_fields)]
pub struct SettingsPatch {
    pub backup: Option<BackupPatch>,
    pub process_names_extra: Option<Vec<String>>,
    pub integrations: Option<IntegrationsPatch>,
    /// Set keys to a string to store them, or to null to remove them.
    pub ui: Option<BTreeMap<String, Option<String>>>,
}

#[derive(Debug, Clone, Default, Deserialize, specta::Type)]
#[serde(default, deny_unknown_fields)]
pub struct BackupPatch {
    /// A path to use it, or null to go back to the default location.
    #[serde(deserialize_with = "present")]
    #[specta(type = Option<PathBuf>)]
    pub location: Option<Option<PathBuf>>,
    pub include_addons: Option<bool>,
    pub on_app_start: Option<bool>,
    pub on_game_exit: Option<bool>,
    /// 0 turns scheduled backups off.
    pub schedule_hours: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize, specta::Type)]
#[serde(default, deny_unknown_fields)]
pub struct IntegrationsPatch {
    pub curseforge: Option<IntegrationSetting>,
    pub wago: Option<IntegrationSetting>,
    pub wago_io: Option<IntegrationSetting>,
    pub github: Option<IntegrationSetting>,
    pub battlenet: Option<IntegrationSetting>,
}

/// For `Option<Option<T>>` fields: a key that's present (even as null) is
/// `Some(..)`; only a missing key is `None` (via `#[serde(default)]`).
fn present<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

impl SettingsPatch {
    fn apply(self, s: &mut Settings) {
        if let Some(b) = self.backup {
            if let Some(location) = b.location {
                s.backup.location = location;
            }
            set(&mut s.backup.include_addons, b.include_addons);
            set(&mut s.backup.on_app_start, b.on_app_start);
            set(&mut s.backup.on_game_exit, b.on_game_exit);
            set(&mut s.backup.schedule_hours, b.schedule_hours);
        }
        set(&mut s.process_names_extra, self.process_names_extra);
        if let Some(i) = self.integrations {
            set(&mut s.integrations.curseforge, i.curseforge);
            set(&mut s.integrations.wago, i.wago);
            set(&mut s.integrations.wago_io, i.wago_io);
            set(&mut s.integrations.github, i.github);
            set(&mut s.integrations.battlenet, i.battlenet);
        }
        for (key, value) in self.ui.unwrap_or_default() {
            match value {
                Some(v) => s.ui.insert(key, v),
                None => s.ui.remove(&key),
            };
        }
    }
}

fn set<T>(field: &mut T, value: Option<T>) {
    if let Some(v) = value {
        *field = v;
    }
}

/// Owns settings.json.
///
/// It keeps the file's raw JSON next to the typed view, so that:
/// - loading never rewrites the file, except to create it, replace a file
///   that isn't JSON, or save a migration;
/// - a field that's missing, mistyped (say, from a newer build) or out of range
///   falls back to its own default without touching the others;
/// - saving writes only the values that actually changed, so unknown keys and
///   values this build couldn't read stay exactly as they were on disk.
pub struct SettingsStore {
    path: PathBuf,
    inner: RwLock<Inner>,
}

struct Inner {
    raw: Value,
    typed: Settings,
}

impl SettingsStore {
    pub fn load(path: PathBuf) -> AppResult<Self> {
        let (raw, write_back) = match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
                Ok(value) if value.is_object() => {
                    let migrated = migrate::run(value.clone(), &path)?;
                    let changed = migrated != value;
                    (migrated, changed)
                }
                _ => {
                    quarantine(&path)?;
                    (to_json(&Settings::default()), true)
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                (to_json(&Settings::default()), true)
            }
            Err(e) => return Err(e.into()),
        };

        let typed = lenient_parse(&raw);
        if write_back {
            write_json(&path, &raw)?;
        }
        Ok(Self {
            path,
            inner: RwLock::new(Inner { raw, typed }),
        })
    }

    pub fn get(&self) -> Settings {
        self.inner
            .read()
            .expect("settings lock poisoned")
            .typed
            .clone()
    }

    /// Applies a patch from the UI. Validated as a whole: if the result is
    /// invalid, nothing changes.
    pub fn apply_patch(&self, patch: SettingsPatch) -> AppResult<Settings> {
        self.update(|s| patch.apply(s))
    }

    /// Backend-side change (e.g. install commands). Validated and saved like any other.
    pub fn update(&self, change: impl FnOnce(&mut Settings)) -> AppResult<Settings> {
        let mut inner = self.inner.write().expect("settings lock poisoned");
        let mut next = inner.typed.clone();
        change(&mut next);
        next.validate()?;

        let mut raw = inner.raw.clone();
        apply_changes(&mut raw, &to_json(&inner.typed), &to_json(&next));
        if raw != inner.raw {
            write_json(&self.path, &raw)?;
        }
        inner.raw = raw;
        inner.typed = next.clone();
        Ok(next)
    }
}

fn to_json(settings: &Settings) -> Value {
    serde_json::to_value(settings).expect("Settings always serializes")
}

fn write_json(path: &Path, value: &Value) -> AppResult<()> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|e| AppError::Io(format!("serialize settings: {e}")))?;
    atomic_replace(path, &bytes)
}

/// Keeps a file that isn't JSON for inspection; the app starts from defaults
/// rather than refusing to launch.
fn quarantine(path: &Path) -> AppResult<()> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    std::fs::rename(
        path,
        path.with_file_name(format!("settings.corrupt-{stamp}.json")),
    )?;
    Ok(())
}

/// The typed view of `raw`. Any value that doesn't deserialize or validate
/// falls back to its default, independently of every other value.
fn lenient_parse(raw: &Value) -> Settings {
    if let Some(s) = parse_valid(raw) {
        return s;
    }
    let mut candidate = to_json(&Settings::default());
    if let Value::Object(fields) = raw {
        merge_accepted(&mut candidate, &mut Vec::new(), fields);
    }
    parse_valid(&candidate).expect("only accepted values were merged into the defaults")
}

fn parse_valid(value: &Value) -> Option<Settings> {
    let mut s = serde_json::from_value::<Settings>(value.clone()).ok()?;
    s.validate().ok()?;
    Some(s)
}

/// Copies each value from `fields` into `candidate` (under `path`) if the
/// result still parses and validates. Tries a whole subtree first and only
/// descends into it if that fails, so all-or-nothing objects like `install`
/// are kept or dropped as a unit.
fn merge_accepted(candidate: &mut Value, path: &mut Vec<String>, fields: &Map<String, Value>) {
    for (key, value) in fields {
        path.push(key.clone());
        let parent = value_at_mut(candidate, &path[..path.len() - 1]);
        let previous = parent.insert(key.clone(), value.clone());

        if parse_valid(candidate).is_none() {
            let parent = value_at_mut(candidate, &path[..path.len() - 1]);
            match &previous {
                Some(prev) => parent.insert(key.clone(), prev.clone()),
                None => parent.remove(key),
            };
            if let (Value::Object(children), Some(Value::Object(_))) = (value, &previous) {
                merge_accepted(candidate, path, children);
            }
        }
        path.pop();
    }
}

fn value_at_mut<'a>(root: &'a mut Value, path: &[String]) -> &'a mut Map<String, Value> {
    path.iter()
        .fold(root, |v, key| &mut v[key.as_str()])
        .as_object_mut()
        .expect("merge_accepted only descends into objects")
}

/// Writes into `raw` only what differs between `prev` and `next`: keys this
/// build doesn't know, and values it never changed, are left as they were.
/// One edge: if `raw` has a non-object where we expect a section (say a newer
/// build made `backup` an array), the first edit to that section replaces it
/// with an object holding just the changed keys.
fn apply_changes(raw: &mut Value, prev: &Value, next: &Value) {
    let (Value::Object(prev), Value::Object(next)) = (prev, next) else {
        *raw = next.clone();
        return;
    };
    if !raw.is_object() {
        *raw = Value::Object(Map::new());
    }
    let raw = raw.as_object_mut().expect("just made it an object");
    for (key, next_value) in next {
        match prev.get(key) {
            Some(prev_value) if prev_value == next_value => {}
            Some(prev_value) if prev_value.is_object() && next_value.is_object() => {
                let slot = raw.entry(key.clone()).or_insert(Value::Null);
                apply_changes(slot, prev_value, next_value);
            }
            _ => {
                raw.insert(key.clone(), next_value.clone());
            }
        }
    }
    for key in prev.keys() {
        if !next.contains_key(key) {
            raw.remove(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn store_in(dir: &Path) -> SettingsStore {
        SettingsStore::load(dir.join("settings.json")).unwrap()
    }

    fn on_disk(dir: &Path) -> Value {
        serde_json::from_slice(&std::fs::read(dir.join("settings.json")).unwrap()).unwrap()
    }

    fn write_settings(dir: &Path, value: Value) {
        std::fs::write(dir.join("settings.json"), value.to_string()).unwrap();
    }

    fn install_json() -> Value {
        json!({ "root": "/games/wow", "flavor": "_classic_beta_" })
    }

    /// Builds a patch the way the frontend sends it: as JSON.
    fn patch(value: Value) -> SettingsPatch {
        serde_json::from_value(value).unwrap()
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
    fn loading_a_valid_file_never_rewrites_it() {
        let tmp = tempfile::tempdir().unwrap();
        let original = r#"{"schema_version":1,"backup":{"include_addons":true}}"#;
        std::fs::write(tmp.path().join("settings.json"), original).unwrap();

        let store = store_in(tmp.path());
        assert!(store.get().backup.include_addons);
        assert!(
            store.get().backup.on_game_exit,
            "missing key falls back to default"
        );
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("settings.json")).unwrap(),
            original
        );
    }

    #[test]
    fn non_json_file_is_quarantined_not_fatal() {
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

    /// Review case 1: unknown keys at any depth survive startup and saves.
    #[test]
    fn nested_unknown_keys_survive_load_and_save() {
        let tmp = tempfile::tempdir().unwrap();
        write_settings(
            tmp.path(),
            json!({
                "schema_version": 2,
                "backup": { "keep_days": 90 },
                "integrations": { "raiderio": { "enabled": true } },
                "from_the_future": [1, 2]
            }),
        );

        let store = store_in(tmp.path());
        store
            .apply_patch(patch(json!({ "backup": { "include_addons": true } })))
            .unwrap();

        let disk = on_disk(tmp.path());
        assert_eq!(
            disk["schema_version"], 2,
            "newer version is never downgraded"
        );
        assert_eq!(disk["backup"]["keep_days"], 90);
        assert_eq!(disk["backup"]["include_addons"], true);
        assert_eq!(disk["integrations"]["raiderio"]["enabled"], true);
        assert_eq!(disk["from_the_future"], json!([1, 2]));
    }

    /// Review case 2: a newer build changed a field's type. Only that field
    /// falls back; the file isn't quarantined and install is kept.
    #[test]
    fn mistyped_field_falls_back_alone() {
        let tmp = tempfile::tempdir().unwrap();
        write_settings(
            tmp.path(),
            json!({
                "schema_version": 2,
                "install": install_json(),
                "backup": { "schedule_hours": "daily", "include_addons": true }
            }),
        );

        let store = store_in(tmp.path());
        let s = store.get();
        assert_eq!(s.install.unwrap().flavor, "_classic_beta_");
        assert_eq!(s.backup.schedule_hours, 24);
        assert!(s.backup.include_addons, "sibling of the bad field is kept");
        assert_eq!(on_disk(tmp.path())["backup"]["schedule_hours"], "daily");

        // Saving an unrelated change still leaves the newer value alone.
        store
            .apply_patch(patch(
                json!({ "integrations": { "github": { "enabled": true } } }),
            ))
            .unwrap();
        assert_eq!(on_disk(tmp.path())["backup"]["schedule_hours"], "daily");
        assert_eq!(
            on_disk(tmp.path())["integrations"]["github"]["enabled"],
            true
        );
    }

    /// Review case 3: a hand-edited out-of-range value falls back alone.
    #[test]
    fn invalid_values_fall_back_alone() {
        let tmp = tempfile::tempdir().unwrap();
        write_settings(
            tmp.path(),
            json!({
                "schema_version": 1,
                "install": install_json(),
                "backup": { "schedule_hours": 200, "location": "relative/dir", "on_app_start": false }
            }),
        );

        let s = store_in(tmp.path()).get();
        assert!(s.install.is_some());
        assert_eq!(s.backup.schedule_hours, 24);
        assert_eq!(s.backup.location, None);
        assert!(!s.backup.on_app_start);
        assert!(
            !std::fs::read_dir(tmp.path()).unwrap().any(|e| e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("corrupt")),
            "nothing quarantined"
        );
    }

    #[test]
    fn fixing_a_bad_value_writes_just_that_value() {
        let tmp = tempfile::tempdir().unwrap();
        write_settings(
            tmp.path(),
            json!({ "schema_version": 1, "backup": { "schedule_hours": 200, "mystery": 1 } }),
        );
        let store = store_in(tmp.path());
        store
            .apply_patch(patch(json!({ "backup": { "schedule_hours": 12 } })))
            .unwrap();

        let disk = on_disk(tmp.path());
        assert_eq!(disk["backup"]["schedule_hours"], 12);
        assert_eq!(disk["backup"]["mystery"], 1);
    }

    #[test]
    fn patch_changes_only_what_it_names() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store_in(tmp.path());
        store
            .apply_patch(patch(json!({ "backup": { "include_addons": true } })))
            .unwrap();
        // A second patch from a stale UI copy that only knows about another field.
        let s = store
            .apply_patch(patch(json!({ "backup": { "on_app_start": false } })))
            .unwrap();
        assert!(s.backup.include_addons, "earlier change survives");
        assert!(!s.backup.on_app_start);
        assert!(s.backup.on_game_exit, "untouched field keeps its value");
        assert_eq!(store.apply_patch(patch(json!({}))).unwrap(), s);
    }

    #[test]
    fn scheduled_backups_off_is_explicit_zero() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store_in(tmp.path());
        store
            .apply_patch(patch(json!({ "backup": { "schedule_hours": 0 } })))
            .unwrap();
        assert_eq!(on_disk(tmp.path())["backup"]["schedule_hours"], 0);
        assert_eq!(store_in(tmp.path()).get().backup.schedule_hours, 0);

        // null or missing on disk means the default, never "off".
        write_settings(
            tmp.path(),
            json!({ "schema_version": 1, "backup": { "schedule_hours": null } }),
        );
        assert_eq!(store_in(tmp.path()).get().backup.schedule_hours, 24);
    }

    #[test]
    fn null_location_goes_back_to_default() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store_in(tmp.path());
        let custom = std::env::temp_dir().join("wfb-backups");
        let s = store
            .apply_patch(patch(json!({ "backup": { "location": custom } })))
            .unwrap();
        assert_eq!(s.backup.location, Some(custom));

        let s = store
            .apply_patch(patch(json!({ "backup": { "location": null } })))
            .unwrap();
        assert_eq!(s.backup.location, None);
    }

    #[test]
    fn ui_keys_are_set_and_removed() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store_in(tmp.path());
        store
            .apply_patch(patch(
                json!({ "ui": { "backups.filter": "auto", "nav": "backups" } }),
            ))
            .unwrap();
        assert_eq!(on_disk(tmp.path())["ui"]["backups.filter"], "auto");

        store
            .apply_patch(patch(json!({ "ui": { "backups.filter": null } })))
            .unwrap();
        assert_eq!(on_disk(tmp.path())["ui"], json!({ "nav": "backups" }));
    }

    #[test]
    fn patch_cannot_touch_backend_owned_fields() {
        for field in ["install", "schema_version"] {
            let attempt = json!({ field: install_json() });
            assert!(
                serde_json::from_value::<SettingsPatch>(attempt).is_err(),
                "{field} must be rejected"
            );
        }
        assert!(
            serde_json::from_value::<SettingsPatch>(json!({ "backup": { "bogus": 1 } })).is_err()
        );

        let tmp = tempfile::tempdir().unwrap();
        let store = store_in(tmp.path());
        let choice = InstallChoice {
            root: PathBuf::from("/games/wow"),
            flavor: "_classic_beta_".into(),
            links: Vec::new(),
        };
        store.update(|s| s.install = Some(choice.clone())).unwrap();
        let saved = store
            .apply_patch(patch(json!({ "backup": { "include_addons": true } })))
            .unwrap();
        assert_eq!(saved.install, Some(choice));
        assert_eq!(saved.schema_version, CURRENT_SCHEMA_VERSION);
        assert_eq!(on_disk(tmp.path())["install"]["flavor"], "_classic_beta_");
    }

    #[test]
    fn backup_location_cannot_be_inside_the_game_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store_in(tmp.path());
        let root = tmp.path().join("World of Warcraft");
        store
            .update(|s| {
                s.install = Some(InstallChoice {
                    root: root.clone(),
                    flavor: "_classic_beta_".into(),
                })
            })
            .unwrap();

        for inside in [
            root.clone(),
            root.join("_classic_beta_").join("WTF").join("bk"),
        ] {
            assert!(
                matches!(
                    store.apply_patch(patch(json!({ "backup": { "location": inside } }))),
                    Err(AppError::InvalidSettings(_))
                ),
                "{inside:?}"
            );
        }
        // A sibling whose name merely starts the same way is fine.
        let sibling = tmp.path().join("World of Warcraft Backups");
        assert!(store
            .apply_patch(patch(json!({ "backup": { "location": sibling } })))
            .is_ok());
    }

    #[test]
    fn invalid_patch_is_rejected_and_not_saved() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store_in(tmp.path());

        for bad in [
            json!({ "backup": { "schedule_hours": 200 } }),
            json!({ "backup": { "location": "relative/dir" } }),
            json!({ "process_names_extra": ["C:\\Games\\Wow.exe"] }),
        ] {
            assert!(
                matches!(
                    store.apply_patch(patch(bad.clone())),
                    Err(AppError::InvalidSettings(_))
                ),
                "{bad}"
            );
        }
        assert_eq!(store.get(), Settings::default());
        assert_eq!(on_disk(tmp.path())["backup"]["schedule_hours"], 24);
    }

    #[test]
    fn process_names_are_trimmed_and_deduped() {
        let tmp = tempfile::tempdir().unwrap();
        let store = store_in(tmp.path());
        let saved = store
            .apply_patch(patch(json!({
                "process_names_extra": [" WowForever.exe ", "", "wowforever.EXE"]
            })))
            .unwrap();
        assert_eq!(
            saved.process_names_extra,
            vec!["WowForever.exe".to_string()]
        );
    }
}
