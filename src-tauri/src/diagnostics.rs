use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

const LOG_FILE: &str = "diagnostics.jsonl";
const MAX_ENTRIES: usize = 300;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticEntry {
    pub timestamp_ms: u64,
    pub level: String,
    pub source: String,
    pub message: String,
    pub details: Option<String>,
}

fn log_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join(LOG_FILE))
        .map_err(|error| error.to_string())
}

pub fn entry(
    level: impl Into<String>,
    source: impl Into<String>,
    message: impl Into<String>,
    details: Option<String>,
) -> DiagnosticEntry {
    DiagnosticEntry {
        timestamp_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        level: level.into(),
        source: source.into(),
        message: message.into(),
        details,
    }
}

pub fn append(app: &AppHandle, item: DiagnosticEntry) -> Result<(), String> {
    let path = log_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut entries = read(app).unwrap_or_default();
    entries.push(item.clone());
    if entries.len() > MAX_ENTRIES {
        entries.drain(..entries.len() - MAX_ENTRIES);
    }
    let content = entries
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?
        .join("\n");
    std::fs::write(path, format!("{content}\n")).map_err(|error| error.to_string())?;
    let _ = app.emit("diagnostic-entry", item);
    Ok(())
}

pub fn read(app: &AppHandle) -> Result<Vec<DiagnosticEntry>, String> {
    let path = log_path(app)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    Ok(content
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect())
}

pub fn clear(app: &AppHandle) -> Result<(), String> {
    let path = log_path(app)?;
    if path.exists() {
        std::fs::remove_file(path).map_err(|error| error.to_string())?;
    }
    Ok(())
}
