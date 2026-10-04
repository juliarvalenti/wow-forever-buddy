use std::path::Path;

use serde_json::Value;

use crate::error::{AppError, AppResult};

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// Migrates settings JSON from version N to N+1. `MIGRATIONS[i]` takes
/// version `i + 1` to `i + 2`, so adding v2 means appending one function and
/// bumping `CURRENT_SCHEMA_VERSION`.
pub type Migration = fn(Value) -> Value;

const MIGRATIONS: &[Migration] = &[];

const _: () = assert!(MIGRATIONS.len() as u32 == CURRENT_SCHEMA_VERSION - 1);

/// Brings raw settings JSON up to the current schema before typed
/// deserialization. The pre-migration file is copied to
/// `settings.v<N>.bak.json` first. A file from a newer build is left as is.
pub fn run(value: Value, settings_path: &Path) -> AppResult<Value> {
    run_with(value, settings_path, MIGRATIONS)
}

fn run_with(mut value: Value, settings_path: &Path, chain: &[Migration]) -> AppResult<Value> {
    let current = chain.len() as u32 + 1;
    let obj = value
        .as_object()
        .ok_or_else(|| AppError::InvalidSettings("settings.json is not an object".into()))?;
    // Files written before versioning existed count as v1.
    let from = obj
        .get("schema_version")
        .and_then(Value::as_u64)
        .map(|v| v as u32)
        .unwrap_or(1)
        .max(1);

    if from >= current {
        return Ok(value);
    }

    let backup = settings_path.with_file_name(format!("settings.v{from}.bak.json"));
    std::fs::copy(settings_path, backup)?;

    for step in &chain[(from - 1) as usize..] {
        value = step(value);
    }
    value["schema_version"] = Value::from(current);
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rename_schedule(mut v: Value) -> Value {
        if let Some(h) = v.as_object_mut().and_then(|o| o.remove("backup_every")) {
            v["backup"] = json!({ "schedule_hours": h });
        }
        v
    }

    fn add_flag(mut v: Value) -> Value {
        v["added_in_v3"] = json!(true);
        v
    }

    #[test]
    fn current_version_is_untouched() {
        let tmp = tempfile::tempdir().unwrap();
        let v = json!({ "schema_version": 1, "x": 1 });
        assert_eq!(
            run(v.clone(), &tmp.path().join("settings.json")).unwrap(),
            v
        );
    }

    #[test]
    fn newer_version_is_left_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let v = json!({ "schema_version": 7 });
        assert_eq!(
            run(v.clone(), &tmp.path().join("settings.json")).unwrap(),
            v
        );
    }

    #[test]
    fn chain_runs_from_file_version_and_backs_up_first() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        let original = json!({ "schema_version": 1, "backup_every": 12 });
        std::fs::write(&path, original.to_string()).unwrap();

        let out = run_with(original.clone(), &path, &[rename_schedule, add_flag]).unwrap();
        assert_eq!(
            out,
            json!({ "schema_version": 3, "backup": { "schedule_hours": 12 }, "added_in_v3": true })
        );

        let backup: Value = serde_json::from_slice(
            &std::fs::read(tmp.path().join("settings.v1.bak.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(backup, original);
    }

    #[test]
    fn partial_chain_skips_applied_steps() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        let v2 = json!({ "schema_version": 2, "backup_every": 12 });
        std::fs::write(&path, v2.to_string()).unwrap();

        let out = run_with(v2, &path, &[rename_schedule, add_flag]).unwrap();
        // rename_schedule (v1→v2) must not run again.
        assert_eq!(out["backup_every"], 12);
        assert_eq!(out["added_in_v3"], true);
        assert_eq!(out["schema_version"], 3);
    }

    #[test]
    fn non_object_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(run(json!([1, 2]), &tmp.path().join("settings.json")).is_err());
    }
}
