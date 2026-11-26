use rusqlite::{Connection, OptionalExtension, params};

use crate::models::{AppSettings, ClipoError};

const SETTINGS_KEY: &str = "app_settings";

/// Loads persisted application settings, returning defaults when unset.
pub fn load_settings(conn: &Connection) -> Result<AppSettings, ClipoError> {
    let json: Option<String> = conn
        .query_row(
            "SELECT value_json FROM app_settings WHERE key = ?1",
            params![SETTINGS_KEY],
            |row| row.get(0),
        )
        .optional()?;

    match json {
        Some(raw) => Ok(serde_json::from_str(&raw)?),
        None => Ok(AppSettings::default()),
    }
}

/// Persists application settings atomically in the `app_settings` table.
pub fn save_settings(conn: &Connection, settings: &AppSettings) -> Result<(), ClipoError> {
    let raw = serde_json::to_string(settings)?;
    conn.execute(
        "INSERT INTO app_settings (key, value_json)
         VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json",
        params![SETTINGS_KEY, raw],
    )?;
    Ok(())
}
