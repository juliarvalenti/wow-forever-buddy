use std::sync::Arc;

use secrecy::zeroize::Zeroize;
use tauri::State;

use crate::error::{AppError, AppResult};
use crate::secrets::{self, IntegrationId, SecretStatus, SecretStore};
use crate::state::AppState;

/// Runs a credential-store call off the main thread: the OS store can block,
/// e.g. on a Keychain unlock prompt.
async fn blocking<T, F>(store: Arc<dyn SecretStore>, f: F) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce(&dyn SecretStore) -> AppResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || f(&*store))
        .await
        .map_err(|e| AppError::Secret(e.to_string()))?
}

/// Which integration keys are set, with a per-row error if one can't be
/// read. Never returns the values.
#[tauri::command]
#[specta::specta]
pub async fn secrets_status(state: State<'_, AppState>) -> AppResult<Vec<SecretStatus>> {
    blocking(state.core.secrets.clone(), |store| {
        Ok(secrets::status(store))
    })
    .await
}

/// Stores a key (trimmed). There is no command to read it back.
#[tauri::command]
#[specta::specta]
pub async fn secrets_set(
    state: State<'_, AppState>,
    id: IntegrationId,
    value: String,
) -> AppResult<()> {
    blocking(state.core.secrets.clone(), move |store| {
        let mut value = value;
        let result = secrets::set(store, id, &value);
        value.zeroize();
        result
    })
    .await
}

#[tauri::command]
#[specta::specta]
pub async fn secrets_delete(state: State<'_, AppState>, id: IntegrationId) -> AppResult<()> {
    blocking(state.core.secrets.clone(), move |store| store.delete(id)).await
}
