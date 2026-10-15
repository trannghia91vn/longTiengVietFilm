use long_tieng_capture_host::{ocr, review};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

pub fn data(app: &AppHandle) -> Result<PathBuf, String> {
    #[cfg(debug_assertions)]
    if let Some(root) = std::env::var_os("LONG_TIENG_OCR_TEST_DATA") {
        return PathBuf::from(root)
            .canonicalize()
            .map_err(|e| e.to_string());
    }
    app.path().app_data_dir().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_ocr_sessions(app: AppHandle) -> Result<Vec<review::Summary>, String> {
    let root = data(&app)?;
    tauri::async_runtime::spawn_blocking(move || review::list(&root))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn load_ocr_session(
    app: AppHandle,
    session_id: String,
) -> Result<review::Document, String> {
    let root = data(&app)?;
    tauri::async_runtime::spawn_blocking(move || review::load(&root, &session_id))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn save_ocr_review(
    app: AppHandle,
    session_id: String,
    expected_revision: String,
    cues: Vec<review::EditCue>,
) -> Result<review::Document, String> {
    let root = data(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        review::save(&root, &session_id, &expected_revision, cues)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn get_ocr_evidence(
    app: AppHandle,
    session_id: String,
    sequence: u64,
) -> Result<review::Evidence, String> {
    let root = data(&app)?;
    tauri::async_runtime::spawn_blocking(move || review::evidence(&root, &session_id, sequence))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn process_ocr_session(
    app: AppHandle,
    session_id: String,
    reprocess: bool,
) -> Result<(), String> {
    let root = data(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        ocr::process(&root, &session_id, reprocess).map(|_| ())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn cancel_ocr_session(app: AppHandle, session_id: String) -> Result<(), String> {
    ocr::cancel(&data(&app)?, &session_id).map(|_| ())
}
#[tauri::command]
pub async fn recheck_ocr_evidence(
    app: AppHandle,
    session_id: String,
    sequence: u64,
) -> Result<Vec<ocr::engine::Candidate>, String> {
    let root = data(&app)?;
    tauri::async_runtime::spawn_blocking(move || review::recognize(&root, &session_id, sequence))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn export_ocr_revision(
    app: AppHandle,
    session_id: String,
    expected_revision: String,
    format: String,
    path: String,
) -> Result<(), String> {
    let root = data(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        review::export(
            &root,
            &session_id,
            &expected_revision,
            &format,
            std::path::Path::new(&path),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn open_ocr_folder(app: AppHandle, session_id: String) -> Result<(), String> {
    let folder = review::directory(&data(&app)?, &session_id)?;
    open_folder(&folder)
}
pub(crate) fn open_folder(folder: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("/usr/bin/open");
    #[cfg(target_os = "windows")]
    let mut command = std::process::Command::new("explorer.exe");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = std::process::Command::new("xdg-open");
    if !command
        .arg(folder)
        .status()
        .map_err(|e| e.to_string())?
        .success()
    {
        return Err("Không mở được thư mục bản thu.".into());
    }
    Ok(())
}
