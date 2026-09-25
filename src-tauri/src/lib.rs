mod diagnostics;
mod models;
mod pipeline;
mod project;
mod srt;

use srt::{Cue, SubtitleDocument};
use tauri::AppHandle;

#[tauri::command]
fn get_diagnostics(app: AppHandle) -> Result<Vec<diagnostics::DiagnosticEntry>, String> {
    diagnostics::read(&app)
}

#[tauri::command]
fn record_diagnostic(
    app: AppHandle,
    level: String,
    source: String,
    message: String,
    details: Option<String>,
) -> Result<(), String> {
    diagnostics::append(&app, diagnostics::entry(level, source, message, details))
}

#[tauri::command]
fn clear_diagnostics(app: AppHandle) -> Result<(), String> {
    diagnostics::clear(&app)
}

#[tauri::command]
fn write_text_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(path, content).map_err(|error| error.to_string())
}

#[tauri::command]
fn get_system_status(app: AppHandle) -> Result<pipeline::SystemStatus, String> {
    pipeline::status(&app)
}

#[tauri::command]
fn list_models(app: AppHandle) -> Result<Vec<models::ModelInfo>, String> {
    models::list(&app)
}

#[tauri::command]
async fn download_model(app: AppHandle, model_id: String) -> Result<(), String> {
    models::download(app, model_id).await
}

#[tauri::command]
async fn remove_model(app: AppHandle, model_id: String) -> Result<(), String> {
    models::remove(&app, &model_id).await
}

#[tauri::command]
async fn inspect_media(app: AppHandle, path: String) -> Result<pipeline::MediaInfo, String> {
    pipeline::inspect_media(&app, &path).await
}

#[tauri::command]
async fn prepare_video_preview(
    app: AppHandle,
    path: String,
    force_proxy: bool,
) -> Result<String, String> {
    pipeline::prepare_video_preview(&app, &path, force_proxy).await
}

#[tauri::command]
fn read_srt(path: String, language: String) -> Result<SubtitleDocument, String> {
    srt::read(std::path::Path::new(&path), &language)
}

#[tauri::command]
async fn transcribe_preview(
    app: AppHandle,
    video_path: String,
    seconds: u64,
    source_language: String,
) -> Result<SubtitleDocument, String> {
    pipeline::transcribe_preview(&app, &video_path, seconds, &source_language).await
}

#[tauri::command]
async fn translate_srt(
    app: AppHandle,
    input_path: String,
    source_language: String,
    quality: String,
    style: String,
) -> Result<pipeline::TranslationResult, String> {
    pipeline::translate_srt(&app, &input_path, &source_language, &quality, &style, None).await
}

#[tauri::command]
async fn render_dubbed_video(
    app: AppHandle,
    video_path: String,
    subtitle_path: String,
    output_path: Option<String>,
    max_seconds: Option<u64>,
) -> Result<pipeline::DubResult, String> {
    pipeline::render_dubbed_video(
        &app,
        &video_path,
        &subtitle_path,
        output_path.as_deref(),
        max_seconds,
    )
    .await
}

#[tauri::command]
fn sync_subtitles(
    app: AppHandle,
    reference_path: String,
    candidate_path: String,
) -> Result<pipeline::SyncResult, String> {
    pipeline::sync_subtitles(&app, &reference_path, &candidate_path)
}

#[tauri::command]
fn write_srt(path: String, cues: Vec<Cue>) -> Result<(), String> {
    srt::write(std::path::Path::new(&path), &cues)
}

#[tauri::command]
fn list_projects(app: AppHandle) -> Result<Vec<project::ProjectSummary>, String> {
    project::list(&app)
}

#[tauri::command]
fn create_project(
    app: AppHandle,
    video_path: String,
    source_language: String,
    profile: String,
) -> Result<project::DubbingProject, String> {
    project::create(&app, &video_path, &source_language, &profile)
}

#[tauri::command]
fn load_project(app: AppHandle, project_id: String) -> Result<project::DubbingProject, String> {
    project::load(&app, &project_id)
}

#[tauri::command]
fn update_project(
    app: AppHandle,
    project: project::DubbingProject,
) -> Result<project::DubbingProject, String> {
    project::update(&app, project)
}

#[tauri::command]
fn import_project_srt(
    app: AppHandle,
    project_id: String,
    path: String,
    language: String,
) -> Result<project::DubbingProject, String> {
    project::import_srt(&app, &project_id, &path, &language)
}

#[tauri::command]
async fn analyze_project(
    app: AppHandle,
    project_id: String,
    sample_seconds: Option<u64>,
) -> Result<project::DubbingProject, String> {
    project::analyze(&app, &project_id, sample_seconds).await
}

#[tauri::command]
async fn translate_project(
    app: AppHandle,
    project_id: String,
) -> Result<project::DubbingProject, String> {
    project::translate(&app, &project_id).await
}

#[tauri::command]
fn get_translation_memory_status(
    app: AppHandle,
) -> Result<project::TranslationMemoryStatus, String> {
    project::translation_memory_status(&app)
}

#[tauri::command]
fn set_translation_memory_enabled(
    app: AppHandle,
    enabled: bool,
) -> Result<project::TranslationMemoryStatus, String> {
    project::set_translation_memory_enabled(&app, enabled)
}

#[tauri::command]
fn save_translation_correction(
    app: AppHandle,
    project_id: String,
    cue_id: String,
    corrected_text: String,
) -> Result<project::TranslationMemoryStatus, String> {
    project::save_translation_correction(&app, &project_id, &cue_id, &corrected_text)
}

#[tauri::command]
fn clear_translation_memory(app: AppHandle) -> Result<project::TranslationMemoryStatus, String> {
    project::clear_translation_memory(&app)
}

#[tauri::command]
async fn synthesize_cues(
    app: AppHandle,
    request: project::SynthesizeRequest,
) -> Result<project::SynthesizeResult, String> {
    project::synthesize(&app, request).await
}

#[tauri::command]
async fn preview_voice(app: AppHandle, voice: String) -> Result<String, String> {
    project::preview_voice(&app, &voice).await
}

#[tauri::command]
async fn render_project(
    app: AppHandle,
    project_id: String,
    output_path: Option<String>,
    max_seconds: Option<u64>,
) -> Result<project::RenderResult, String> {
    project::render(&app, &project_id, output_path.as_deref(), max_seconds).await
}

#[tauri::command]
fn cancel_job() {
    project::cancel_job();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            get_diagnostics,
            record_diagnostic,
            clear_diagnostics,
            write_text_file,
            get_system_status,
            list_models,
            download_model,
            remove_model,
            inspect_media,
            prepare_video_preview,
            read_srt,
            transcribe_preview,
            translate_srt,
            render_dubbed_video,
            sync_subtitles,
            write_srt,
            list_projects,
            create_project,
            load_project,
            update_project,
            import_project_srt,
            analyze_project,
            translate_project,
            get_translation_memory_status,
            set_translation_memory_enabled,
            save_translation_correction,
            clear_translation_memory,
            preview_voice,
            synthesize_cues,
            render_project,
            cancel_job,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}
