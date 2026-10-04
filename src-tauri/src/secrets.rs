//! Integration API keys in the OS credential store (spec §7).
//!
//! Values live in Windows Credential Manager (Keychain on macOS dev machines),
//! never in settings, the DB, logs or backups. They never cross to the
//! frontend either: commands can set, delete and report status, and only
//! Rust-side integrations call [`SecretStore::get`].

use secrecy::SecretString;
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

/// Keyring service name; matches the app identifier in tauri.conf.json.
pub const SERVICE: &str = "com.juliarvalenti.wowforeverbuddy";

// Without a native store, keyring silently falls back to an in-memory mock and
// keys vanish on restart. Fail the build instead.
#[cfg(not(any(windows, target_os = "macos")))]
compile_error!("secrets need a native keyring store: build for Windows or macOS");

/// Longest value accepted, in UTF-16 code units. Windows Credential Manager
/// caps a credential blob at 2560 bytes and keyring stores UTF-16, so this is
/// the real limit there; we apply it on every OS so the error is ours and
/// the same everywhere. Real API keys are far shorter.
pub const MAX_VALUE_UTF16: usize = 1280;

/// The only keys the app stores. A closed enum, so the frontend can't write
/// arbitrary keychain entries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationId {
    Curseforge,
    Wago,
    WagoIo,
    Github,
    BattlenetClientId,
    BattlenetClientSecret,
}

impl IntegrationId {
    pub const ALL: [IntegrationId; 6] = [
        IntegrationId::Curseforge,
        IntegrationId::Wago,
        IntegrationId::WagoIo,
        IntegrationId::Github,
        IntegrationId::BattlenetClientId,
        IntegrationId::BattlenetClientSecret,
    ];

    /// The keyring entry's user name. Same spelling as the serde form.
    pub fn as_str(self) -> &'static str {
        match self {
            IntegrationId::Curseforge => "curseforge",
            IntegrationId::Wago => "wago",
            IntegrationId::WagoIo => "wago_io",
            IntegrationId::Github => "github",
            IntegrationId::BattlenetClientId => "battlenet_client_id",
            IntegrationId::BattlenetClientSecret => "battlenet_client_secret",
        }
    }
}

/// One row of the Integrations panel. Each id is checked on its own, so one
/// unreadable credential shows as that row's error instead of failing all.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, specta::Type)]
pub struct SecretStatus {
    pub id: IntegrationId,
    pub is_set: bool,
    /// Why the store couldn't be read for this id. Never contains the value.
    pub error: Option<String>,
}

/// Storage seam: the OS keyring in the app, in memory in tests.
pub trait SecretStore: Send + Sync + std::fmt::Debug {
    fn set(&self, id: IntegrationId, value: &str) -> AppResult<()>;
    /// Rust-only. There is deliberately no command that returns this.
    fn get(&self, id: IntegrationId) -> AppResult<Option<SecretString>>;
    /// Deleting a key that isn't set is not an error.
    fn delete(&self, id: IntegrationId) -> AppResult<()>;
    /// keyring 3 has no exists-check, so this reads the value and drops it.
    /// On macOS dev builds that can prompt for Keychain access once per item
    /// after a rebuild. Override in `KeyringStore` if a cheaper call appears.
    fn is_set(&self, id: IntegrationId) -> AppResult<bool> {
        Ok(self.get(id)?.is_some())
    }
}

/// Trims what the user pasted (keys often come with a trailing newline) and
/// rejects empty or oversized values. Errors never include the value.
pub fn normalize(value: &str) -> AppResult<&str> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::Secret(
            "value is empty; remove the key instead".into(),
        ));
    }
    if value.encode_utf16().count() > MAX_VALUE_UTF16 {
        return Err(AppError::Secret(format!(
            "value is longer than {MAX_VALUE_UTF16} characters"
        )));
    }
    Ok(value)
}

pub fn set(store: &dyn SecretStore, id: IntegrationId, value: &str) -> AppResult<()> {
    store.set(id, normalize(value)?)
}

pub fn status(store: &dyn SecretStore) -> Vec<SecretStatus> {
    IntegrationId::ALL
        .into_iter()
        .map(|id| match store.is_set(id) {
            Ok(is_set) => SecretStatus {
                id,
                is_set,
                error: None,
            },
            Err(e) => SecretStatus {
                id,
                is_set: false,
                error: Some(e.to_string()),
            },
        })
        .collect()
}

/// The OS credential store via the `keyring` crate.
#[derive(Debug)]
pub struct KeyringStore {
    service: String,
}

impl KeyringStore {
    pub fn new() -> Self {
        Self::with_service(SERVICE)
    }

    /// A separate service name, so the real-keyring test can't touch the
    /// app's entries.
    pub fn with_service(service: &str) -> Self {
        Self {
            service: service.to_string(),
        }
    }

    fn entry(&self, id: IntegrationId) -> AppResult<keyring::Entry> {
        keyring::Entry::new(&self.service, id.as_str()).map_err(keyring_error)
    }
}

impl Default for KeyringStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretStore for KeyringStore {
    fn set(&self, id: IntegrationId, value: &str) -> AppResult<()> {
        self.entry(id)?.set_password(value).map_err(keyring_error)
    }

    fn get(&self, id: IntegrationId) -> AppResult<Option<SecretString>> {
        match self.entry(id)?.get_password() {
            Ok(value) => Ok(Some(SecretString::from(value))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(keyring_error(e)),
        }
    }

    fn delete(&self, id: IntegrationId) -> AppResult<()> {
        match self.entry(id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(keyring_error(e)),
        }
    }
}

fn keyring_error(e: keyring::Error) -> AppError {
    match e {
        // Its Debug form carries the stored bytes; never let them out.
        keyring::Error::BadEncoding(_) => {
            AppError::Secret("stored value is not valid UTF-8".into())
        }
        other => AppError::Secret(other.to_string()),
    }
}

/// In-memory store for tests.
#[cfg(test)]
#[derive(Default)]
pub struct MemoryStore(std::sync::Mutex<std::collections::HashMap<IntegrationId, String>>);

/// Never prints the values.
#[cfg(test)]
impl std::fmt::Debug for MemoryStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MemoryStore(..)")
    }
}

#[cfg(test)]
impl SecretStore for MemoryStore {
    fn set(&self, id: IntegrationId, value: &str) -> AppResult<()> {
        self.0.lock().unwrap().insert(id, value.to_string());
        Ok(())
    }

    fn get(&self, id: IntegrationId) -> AppResult<Option<SecretString>> {
        Ok(self
            .0
            .lock()
            .unwrap()
            .get(&id)
            .cloned()
            .map(SecretString::from))
    }

    fn delete(&self, id: IntegrationId) -> AppResult<()> {
        self.0.lock().unwrap().remove(&id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use secrecy::ExposeSecret;

    #[test]
    fn ids_serialize_as_their_keyring_names() {
        for id in IntegrationId::ALL {
            let json = serde_json::to_value(id).unwrap();
            assert_eq!(json, serde_json::json!(id.as_str()));
            let back: IntegrationId = serde_json::from_value(json).unwrap();
            assert_eq!(back, id);
        }
        let names: std::collections::HashSet<_> =
            IntegrationId::ALL.iter().map(|id| id.as_str()).collect();
        assert_eq!(names.len(), IntegrationId::ALL.len());
    }

    #[test]
    fn unknown_ids_are_rejected() {
        for bad in ["foo", "Curseforge", "", "../github"] {
            assert!(serde_json::from_value::<IntegrationId>(serde_json::json!(bad)).is_err());
        }
    }

    #[test]
    fn normalize_trims_and_validates() {
        assert_eq!(normalize("  abc123\r\n").unwrap(), "abc123");
        assert!(matches!(normalize(" \n\t"), Err(AppError::Secret(_))));
        let long = "k".repeat(MAX_VALUE_UTF16 + 1);
        let err = normalize(&long).unwrap_err().to_string();
        assert!(!err.contains("kkkk"), "error leaks the value: {err}");
        assert!(normalize(&"k".repeat(MAX_VALUE_UTF16)).is_ok());
    }

    #[test]
    fn length_cap_counts_utf16_units() {
        // "é" is 2 UTF-8 bytes but 1 UTF-16 unit; "😀" is 4 bytes, 2 units.
        assert!(normalize(&"é".repeat(MAX_VALUE_UTF16)).is_ok());
        assert!(normalize(&"😀".repeat(MAX_VALUE_UTF16 / 2)).is_ok());
        assert!(normalize(&format!("{}k", "😀".repeat(MAX_VALUE_UTF16 / 2))).is_err());
    }

    /// Fails reads for one id, like a credential with a bad encoding.
    #[derive(Debug, Default)]
    struct OneBadStore(MemoryStore);

    impl SecretStore for OneBadStore {
        fn set(&self, id: IntegrationId, value: &str) -> AppResult<()> {
            self.0.set(id, value)
        }
        fn get(&self, id: IntegrationId) -> AppResult<Option<SecretString>> {
            if id == IntegrationId::Wago {
                return Err(AppError::Secret("stored value is not valid UTF-8".into()));
            }
            self.0.get(id)
        }
        fn delete(&self, id: IntegrationId) -> AppResult<()> {
            self.0.delete(id)
        }
    }

    #[test]
    fn one_unreadable_entry_does_not_hide_the_others() {
        let store = OneBadStore::default();
        set(&store, IntegrationId::Github, "ghp_x").unwrap();
        let list = status(&store);
        assert_eq!(list.len(), IntegrationId::ALL.len());
        let row = |id| list.iter().find(|s| s.id == id).unwrap();
        assert!(row(IntegrationId::Github).is_set);
        assert_eq!(row(IntegrationId::Github).error, None);
        assert!(!row(IntegrationId::Wago).is_set);
        assert!(row(IntegrationId::Wago)
            .error
            .as_deref()
            .unwrap()
            .contains("UTF-8"));
    }

    #[test]
    fn set_get_delete() {
        let store = MemoryStore::default();
        assert!(!store.is_set(IntegrationId::Github).unwrap());
        set(&store, IntegrationId::Github, " ghp_secret\n").unwrap();
        let got = store.get(IntegrationId::Github).unwrap().unwrap();
        assert_eq!(got.expose_secret(), "ghp_secret");
        assert!(store.is_set(IntegrationId::Github).unwrap());
        assert!(!store.is_set(IntegrationId::Wago).unwrap());

        store.delete(IntegrationId::Github).unwrap();
        assert!(!store.is_set(IntegrationId::Github).unwrap());
        // Deleting again is fine.
        store.delete(IntegrationId::Github).unwrap();
    }

    #[test]
    fn empty_set_does_not_overwrite() {
        let store = MemoryStore::default();
        set(&store, IntegrationId::Wago, "old").unwrap();
        assert!(set(&store, IntegrationId::Wago, "   ").is_err());
        let got = store.get(IntegrationId::Wago).unwrap().unwrap();
        assert_eq!(got.expose_secret(), "old");
    }

    #[test]
    fn status_lists_every_id_without_values() {
        let store = MemoryStore::default();
        set(&store, IntegrationId::Curseforge, "cf-key").unwrap();
        let list = status(&store);
        assert_eq!(list.len(), IntegrationId::ALL.len());
        assert!(list.contains(&SecretStatus {
            id: IntegrationId::Curseforge,
            is_set: true,
            error: None,
        }));
        assert!(list.iter().filter(|s| s.is_set).count() == 1);
        let json = serde_json::to_string(&list).unwrap();
        assert!(!json.contains("cf-key"));
    }

    #[test]
    fn secrets_are_redacted_in_debug() {
        let store = MemoryStore::default();
        set(&store, IntegrationId::BattlenetClientSecret, "hunter2").unwrap();
        let got = store.get(IntegrationId::BattlenetClientSecret).unwrap();
        assert!(!format!("{got:?}").contains("hunter2"));
    }

    /// Talks to the real OS store under a test-only service name. Ignored by
    /// default since CI keychains may be locked or prompt; run with
    /// `cargo test secrets::tests::real_keyring -- --ignored`.
    #[test]
    #[ignore]
    fn real_keyring_round_trip() {
        let store = KeyringStore::with_service(&format!("{SERVICE}.test"));
        let id = IntegrationId::Wago;
        store.delete(id).unwrap();
        assert!(!store.is_set(id).unwrap());
        store.set(id, "test-value-é").unwrap();
        assert_eq!(
            store.get(id).unwrap().unwrap().expose_secret(),
            "test-value-é"
        );
        store.set(id, "replaced").unwrap();
        assert_eq!(store.get(id).unwrap().unwrap().expose_secret(), "replaced");
        store.delete(id).unwrap();
        assert!(!store.is_set(id).unwrap());
        store.delete(id).unwrap();
    }
}
