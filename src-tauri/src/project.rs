use crate::pipeline;
use crate::srt::{self, Cue};
use crate::{diagnostics, models};
use futures_util::stream::{FuturesUnordered, StreamExt};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::time::sleep;
use uuid::Uuid;

static CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);
const CURRENT_TTS_CACHE_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlossaryEntry {
    pub source: String,
    pub target: String,
    pub note: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerProfile {
    pub id: String,
    pub display_name: String,
    pub voice_preset: String,
    pub color: String,
    pub notes: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DialogueCue {
    pub id: String,
    pub index: usize,
    pub start_ms: i64,
    pub end_ms: i64,
    pub source_text: String,
    pub translated_text: String,
    #[serde(default)]
    pub machine_translated_text: String,
    #[serde(default)]
    pub scene_id: String,
    #[serde(default)]
    pub utterance_group_id: String,
    pub speaker_id: String,
    pub confidence: f64,
    pub audio_path: Option<String>,
    pub audio_duration_ms: Option<i64>,
    pub speed: f64,
    pub volume: f64,
    pub status: String,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioSettings {
    pub separate_background: bool,
    pub background_volume: f64,
    pub dialogue_volume: f64,
    pub min_tempo: f64,
    pub max_tempo: f64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectAssets {
    pub source_srt: Option<String>,
    pub translated_srt: Option<String>,
    pub background_stem: Option<String>,
    pub vocal_stem: Option<String>,
    pub preview_output: Option<String>,
    pub final_output: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterBible {
    pub summary: String,
    pub relationships: Vec<String>,
    pub address_rules: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneContext {
    pub id: String,
    pub start_index: usize,
    pub end_index: usize,
    pub summary: String,
    pub tone: String,
    pub status: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationGuide {
    pub revision: u32,
    pub synopsis: String,
    pub tone: String,
    pub user_notes: String,
    pub address_rules: Vec<String>,
    pub scenes: Vec<SceneContext>,
    pub prepared_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationMemoryEntry {
    pub id: String,
    pub source_language: String,
    pub profile: String,
    pub source_text: String,
    pub machine_text: String,
    pub corrected_text: String,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationMemoryStatus {
    pub enabled: bool,
    pub count: usize,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobError {
    pub code: String,
    pub stage: String,
    pub message: String,
    pub cue_id: Option<String>,
    pub cue_index: Option<usize>,
    pub voice: Option<String>,
    pub batch: Option<usize>,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub elapsed_ms: u64,
    pub details: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LongJobState {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub stage: String,
    pub total: usize,
    pub processed: usize,
    pub generated: usize,
    pub reused: usize,
    pub percent: f64,
    pub message: String,
    pub current_cue_index: Option<usize>,
    pub started_at_ms: u64,
    pub updated_at_ms: u64,
    pub elapsed_seconds: u64,
    pub eta_seconds: Option<u64>,
    pub error: Option<JobError>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DubbingProject {
    pub id: String,
    pub title: String,
    pub video_path: String,
    pub preview_path: Option<String>,
    pub source_language: String,
    pub target_language: String,
    pub profile: String,
    pub status: String,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub glossary: Vec<GlossaryEntry>,
    pub speakers: Vec<SpeakerProfile>,
    pub cues: Vec<DialogueCue>,
    pub audio_settings: AudioSettings,
    pub assets: ProjectAssets,
    pub character_bible: CharacterBible,
    #[serde(default)]
    pub translation_guide: TranslationGuide,
    pub warnings: Vec<String>,
    #[serde(default)]
    pub tts_cache_version: u32,
    #[serde(default)]
    pub last_job: Option<LongJobState>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub id: String,
    pub title: String,
    pub video_path: String,
    pub source_language: String,
    pub profile: String,
    pub cue_count: usize,
    pub updated_at_ms: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectProgress {
    pub job_id: Option<String>,
    pub kind: Option<String>,
    pub status: String,
    pub stage: String,
    pub current: usize,
    pub total: usize,
    pub processed: usize,
    pub generated: usize,
    pub reused: usize,
    pub percent: f64,
    pub message: String,
    pub current_cue_index: Option<usize>,
    pub elapsed_seconds: u64,
    pub eta_seconds: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SynthesizeRequest {
    pub project_id: String,
    pub cue_ids: Vec<String>,
    pub max_seconds: Option<u64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SynthesizeResult {
    pub project: DubbingProject,
    pub generated: usize,
    pub reused: usize,
    pub skipped: usize,
    pub warning_count: usize,
    pub elapsed_seconds: u64,
    pub job: LongJobState,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RenderResult {
    pub project: DubbingProject,
    pub path: String,
    pub rendered_cues: usize,
    pub used_separation: bool,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn project_root(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join("projects"))
        .map_err(|error| error.to_string())
}

fn project_dir(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    if id.is_empty() || !id.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-') {
        return Err("ID dự án không hợp lệ.".to_string());
    }
    Ok(project_root(app)?.join(id))
}

fn project_path(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    Ok(project_dir(app, id)?.join("project.json"))
}

fn write_project_translation_srt(
    app: &AppHandle,
    project: &mut DubbingProject,
) -> Result<(), String> {
    let destination = project_dir(app, &project.id)?.join("vietnamese.srt");
    let cues = project
        .cues
        .iter()
        .map(|cue| Cue {
            index: cue.index,
            start_ms: cue.start_ms,
            end_ms: cue.end_ms,
            text: cue.translated_text.clone(),
        })
        .collect::<Vec<_>>();
    srt::write(&destination, &cues)?;
    project.assets.translated_srt = Some(destination.to_string_lossy().into_owned());
    Ok(())
}

fn translation_memory_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join("translation-memory.json"))
        .map_err(|error| error.to_string())
}

fn translation_memory_settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join("translation-memory-settings.json"))
        .map_err(|error| error.to_string())
}

fn translation_memory_enabled(app: &AppHandle) -> Result<bool, String> {
    let path = translation_memory_settings_path(app)?;
    if !path.exists() {
        return Ok(true);
    }
    let value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?;
    Ok(value
        .get("enabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true))
}

fn read_translation_memory(app: &AppHandle) -> Result<Vec<TranslationMemoryEntry>, String> {
    let path = translation_memory_path(app)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    serde_json::from_slice(&std::fs::read(path).map_err(|error| error.to_string())?)
        .map_err(|error| format!("Bộ nhớ dịch không hợp lệ: {error}"))
}

fn write_translation_memory(
    app: &AppHandle,
    entries: &[TranslationMemoryEntry],
) -> Result<(), String> {
    let destination = translation_memory_path(app)?;
    let parent = destination
        .parent()
        .ok_or_else(|| "Không xác định được thư mục app data.".to_string())?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let temporary = destination.with_extension("json.tmp");
    std::fs::write(
        &temporary,
        serde_json::to_vec_pretty(entries).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    std::fs::rename(temporary, destination).map_err(|error| error.to_string())
}

pub fn translation_memory_status(app: &AppHandle) -> Result<TranslationMemoryStatus, String> {
    Ok(TranslationMemoryStatus {
        enabled: translation_memory_enabled(app)?,
        count: read_translation_memory(app)?.len(),
    })
}

pub fn set_translation_memory_enabled(
    app: &AppHandle,
    enabled: bool,
) -> Result<TranslationMemoryStatus, String> {
    let destination = translation_memory_settings_path(app)?;
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(
        destination,
        serde_json::to_vec_pretty(&serde_json::json!({ "enabled": enabled }))
            .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    translation_memory_status(app)
}

pub fn clear_translation_memory(app: &AppHandle) -> Result<TranslationMemoryStatus, String> {
    let path = translation_memory_path(app)?;
    if path.exists() {
        std::fs::remove_file(path).map_err(|error| error.to_string())?;
    }
    translation_memory_status(app)
}

pub fn save_translation_correction(
    app: &AppHandle,
    project_id: &str,
    cue_id: &str,
    corrected_text: &str,
) -> Result<TranslationMemoryStatus, String> {
    if !translation_memory_enabled(app)? {
        return translation_memory_status(app);
    }
    let project = load_inner(app, project_id)?;
    let cue = project
        .cues
        .iter()
        .find(|cue| cue.id == cue_id)
        .ok_or_else(|| "Không tìm thấy cue cần ghi nhớ.".to_string())?;
    let corrected = corrected_text.trim();
    if cue.source_text.trim().is_empty()
        || cue.machine_translated_text.trim().is_empty()
        || corrected.is_empty()
        || corrected == cue.machine_translated_text.trim()
    {
        return translation_memory_status(app);
    }
    let mut digest = Sha256::new();
    digest.update(project.source_language.as_bytes());
    digest.update([0]);
    digest.update(project.profile.as_bytes());
    digest.update([0]);
    digest.update(cue.source_text.trim().as_bytes());
    digest.update([0]);
    digest.update(corrected.as_bytes());
    let id = hex::encode(digest.finalize());
    let mut entries = read_translation_memory(app)?;
    entries.retain(|entry| {
        entry.id != id
            && !(entry.source_language == project.source_language
                && entry.profile == project.profile
                && entry.source_text.trim() == cue.source_text.trim())
    });
    entries.push(TranslationMemoryEntry {
        id,
        source_language: project.source_language,
        profile: project.profile,
        source_text: cue.source_text.trim().to_string(),
        machine_text: cue.machine_translated_text.trim().to_string(),
        corrected_text: corrected.to_string(),
        created_at_ms: now_ms(),
    });
    if entries.len() > 2_000 {
        let remove = entries.len() - 2_000;
        entries.drain(0..remove);
    }
    write_translation_memory(app, &entries)?;
    translation_memory_status(app)
}

fn save(app: &AppHandle, project: &DubbingProject) -> Result<(), String> {
    let directory = project_dir(app, &project.id)?;
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let destination = directory.join("project.json");
    let temporary = directory.join("project.json.tmp");
    let json = serde_json::to_vec_pretty(project).map_err(|error| error.to_string())?;
    std::fs::write(&temporary, json).map_err(|error| error.to_string())?;
    std::fs::rename(&temporary, &destination).map_err(|error| error.to_string())
}

fn load_inner(app: &AppHandle, id: &str) -> Result<DubbingProject, String> {
    let bytes = std::fs::read(project_path(app, id)?).map_err(|error| error.to_string())?;
    let mut project: DubbingProject = serde_json::from_slice(&bytes)
        .map_err(|error| format!("project.json không hợp lệ: {error}"))?;
    if let Some(job) = project.last_job.as_mut() {
        if job.status == "running" {
            job.status = "interrupted".into();
            job.message = "Tác vụ trước bị gián đoạn. Có thể tiếp tục từ phần đã lưu.".into();
            job.updated_at_ms = now_ms();
            project.updated_at_ms = now_ms();
            save(app, &project)?;
        }
    }
    if project.tts_cache_version < CURRENT_TTS_CACHE_VERSION {
        for cue in &mut project.cues {
            cue.audio_path = None;
            cue.audio_duration_ms = None;
            if !cue.translated_text.trim().is_empty() {
                cue.status = "translated".into();
            }
        }
        project.assets.preview_output = None;
        project.assets.final_output = None;
        project.tts_cache_version = CURRENT_TTS_CACHE_VERSION;
        project.updated_at_ms = now_ms();
        project.warnings.push(
            "Audio TTS cũ đã được vô hiệu hóa để tạo lại bằng bộ phát âm tiếng Việt SEA-G2P."
                .into(),
        );
        save(app, &project)?;
    }
    Ok(project)
}

fn default_speakers() -> Vec<SpeakerProfile> {
    vec![SpeakerProfile {
        id: "speaker-1".into(),
        display_name: "Nhân vật 1".into(),
        voice_preset: "thuy_dung".into(),
        color: "#5b78b2".into(),
        notes: String::new(),
    }]
}

fn emit_progress(
    app: &AppHandle,
    stage: &str,
    current: usize,
    total: usize,
    percent: f64,
    message: impl Into<String>,
) {
    let _ = app.emit(
        "project-progress",
        ProjectProgress {
            job_id: None,
            kind: None,
            status: "running".into(),
            stage: stage.into(),
            current,
            total,
            processed: current,
            generated: 0,
            reused: 0,
            percent: percent.clamp(0.0, 100.0),
            message: message.into(),
            current_cue_index: None,
            elapsed_seconds: 0,
            eta_seconds: None,
        },
    );
}

fn refresh_job_timing(job: &mut LongJobState) {
    let now = now_ms();
    job.updated_at_ms = now;
    job.elapsed_seconds = now.saturating_sub(job.started_at_ms) / 1_000;
    job.percent = if job.total == 0 {
        0.0
    } else {
        (job.processed as f64 / job.total as f64 * 100.0).clamp(0.0, 100.0)
    };
    job.eta_seconds = if job.processed > 0 && job.processed < job.total {
        Some(
            (job.elapsed_seconds as f64 / job.processed as f64 * (job.total - job.processed) as f64)
                .round() as u64,
        )
    } else {
        None
    };
}

fn emit_long_job(app: &AppHandle, job: &LongJobState) {
    let _ = app.emit(
        "project-progress",
        ProjectProgress {
            job_id: Some(job.id.clone()),
            kind: Some(job.kind.clone()),
            status: job.status.clone(),
            stage: job.stage.clone(),
            current: job.processed,
            total: job.total,
            processed: job.processed,
            generated: job.generated,
            reused: job.reused,
            percent: job.percent,
            message: job.message.clone(),
            current_cue_index: job.current_cue_index,
            elapsed_seconds: job.elapsed_seconds,
            eta_seconds: job.eta_seconds,
        },
    );
}

fn start_long_job(kind: &str, total: usize, message: &str) -> LongJobState {
    let now = now_ms();
    LongJobState {
        id: Uuid::new_v4().to_string(),
        kind: kind.into(),
        status: "running".into(),
        stage: "preflight".into(),
        total,
        processed: 0,
        generated: 0,
        reused: 0,
        percent: 0.0,
        message: message.into(),
        current_cue_index: None,
        started_at_ms: now,
        updated_at_ms: now,
        elapsed_seconds: 0,
        eta_seconds: None,
        error: None,
    }
}

fn checkpoint_job(
    app: &AppHandle,
    project: &mut DubbingProject,
    job: &mut LongJobState,
) -> Result<(), String> {
    refresh_job_timing(job);
    project.last_job = Some(job.clone());
    project.updated_at_ms = now_ms();
    save(app, project)?;
    emit_long_job(app, job);
    Ok(())
}

fn record_job_event(app: &AppHandle, level: &str, source: &str, job: &LongJobState) {
    let _ = diagnostics::append(
        app,
        diagnostics::entry(
            level,
            source,
            job.message.clone(),
            serde_json::to_string_pretty(job).ok(),
        ),
    );
}

fn ensure_not_cancelled() -> Result<(), String> {
    if CANCEL_REQUESTED.load(Ordering::Relaxed) {
        Err("Đã hủy tác vụ.".to_string())
    } else {
        Ok(())
    }
}

fn begin_job() {
    CANCEL_REQUESTED.store(false, Ordering::Relaxed);
}

pub fn cancel_job() {
    CANCEL_REQUESTED.store(true, Ordering::Relaxed);
}

pub fn create(
    app: &AppHandle,
    video_path: &str,
    source_language: &str,
    profile: &str,
) -> Result<DubbingProject, String> {
    if !matches!(source_language, "ja" | "en") {
        return Err("Ngôn ngữ nguồn chỉ hỗ trợ tiếng Nhật hoặc tiếng Anh.".to_string());
    }
    if !matches!(
        profile,
        "auto" | "modern" | "anime" | "historical" | "documentary"
    ) {
        return Err("Profile dịch không hợp lệ.".to_string());
    }
    let path = Path::new(video_path);
    if !path.is_file() {
        return Err("Không tìm thấy video nguồn.".to_string());
    }
    let timestamp = now_ms();
    let project = DubbingProject {
        id: Uuid::new_v4().to_string(),
        title: path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("Dự án lồng tiếng")
            .to_string(),
        video_path: video_path.to_string(),
        preview_path: None,
        source_language: source_language.to_string(),
        target_language: "vi".into(),
        profile: profile.to_string(),
        status: "draft".into(),
        created_at_ms: timestamp,
        updated_at_ms: timestamp,
        glossary: Vec::new(),
        speakers: default_speakers(),
        cues: Vec::new(),
        audio_settings: AudioSettings {
            separate_background: false,
            background_volume: 1.0,
            dialogue_volume: 1.0,
            min_tempo: 0.92,
            max_tempo: 1.12,
        },
        assets: ProjectAssets::default(),
        character_bible: CharacterBible::default(),
        translation_guide: TranslationGuide::default(),
        warnings: Vec::new(),
        tts_cache_version: CURRENT_TTS_CACHE_VERSION,
        last_job: None,
    };
    save(app, &project)?;
    Ok(project)
}

pub fn list(app: &AppHandle) -> Result<Vec<ProjectSummary>, String> {
    let root = project_root(app)?;
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut projects = std::fs::read_dir(root)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let bytes = std::fs::read(entry.path().join("project.json")).ok()?;
            let project: DubbingProject = serde_json::from_slice(&bytes).ok()?;
            Some(ProjectSummary {
                id: project.id,
                title: project.title,
                video_path: project.video_path,
                source_language: project.source_language,
                profile: project.profile,
                cue_count: project.cues.len(),
                updated_at_ms: project.updated_at_ms,
            })
        })
        .collect::<Vec<_>>();
    projects.sort_by_key(|project| std::cmp::Reverse(project.updated_at_ms));
    Ok(projects)
}

pub fn load(app: &AppHandle, id: &str) -> Result<DubbingProject, String> {
    load_inner(app, id)
}

pub fn update(app: &AppHandle, mut project: DubbingProject) -> Result<DubbingProject, String> {
    let existing = load_inner(app, &project.id)?;
    if existing.video_path != project.video_path {
        return Err("Không thể đổi video nguồn của dự án đã tạo.".to_string());
    }
    project.updated_at_ms = now_ms();
    save(app, &project)?;
    Ok(project)
}

fn dialogue_from_cues(cues: &[Cue], language: &str) -> Vec<DialogueCue> {
    cues.iter()
        .enumerate()
        .map(|(position, cue)| DialogueCue {
            id: format!("cue-{:05}", position + 1),
            index: position + 1,
            start_ms: cue.start_ms,
            end_ms: cue.end_ms,
            source_text: if language == "vi" {
                String::new()
            } else {
                cue.text.clone()
            },
            translated_text: if language == "vi" {
                cue.text.clone()
            } else {
                String::new()
            },
            machine_translated_text: String::new(),
            scene_id: String::new(),
            utterance_group_id: String::new(),
            speaker_id: "speaker-1".into(),
            confidence: 1.0,
            audio_path: None,
            audio_duration_ms: None,
            speed: 1.0,
            volume: 1.0,
            status: if language == "vi" {
                "translated"
            } else {
                "source"
            }
            .into(),
            warnings: Vec::new(),
        })
        .collect()
}

pub fn import_srt(
    app: &AppHandle,
    project_id: &str,
    path: &str,
    language: &str,
) -> Result<DubbingProject, String> {
    if !matches!(language, "ja" | "en" | "vi") {
        return Err("SRT chỉ hỗ trợ Nhật, Anh hoặc Việt.".to_string());
    }
    let document = srt::read(Path::new(path), language)?;
    let mut project = load_inner(app, project_id)?;
    project.cues = dialogue_from_cues(&document.cues, language);
    project.speakers = default_speakers();
    project.translation_guide = TranslationGuide::default();
    project.character_bible = CharacterBible::default();
    if language == "vi" {
        project.assets.translated_srt = Some(path.into());
        project.status = "translated".into();
    } else {
        project.source_language = language.into();
        project.assets.source_srt = Some(path.into());
        project.status = "subtitles-imported".into();
    }
    project.updated_at_ms = now_ms();
    project.warnings.push(
        "Mọi cue đang dùng Nhân vật 1. Có thể thêm nhân vật và gán lại từng cue trong panel Nhân vật."
            .into(),
    );
    save(app, &project)?;
    Ok(project)
}

pub async fn analyze(
    app: &AppHandle,
    project_id: &str,
    sample_seconds: Option<u64>,
) -> Result<DubbingProject, String> {
    begin_job();
    let mut project = load_inner(app, project_id)?;
    let media = pipeline::inspect_media(app, &project.video_path).await?;
    let seconds =
        sample_seconds.unwrap_or_else(|| (media.duration_ms.max(1) as u64).div_ceil(1_000));
    emit_progress(app, "extracting", 0, 1, 4.0, "Đang trích âm thanh phim...");
    ensure_not_cancelled()?;
    let transcript =
        pipeline::transcribe_preview(app, &project.video_path, seconds, &project.source_language)
            .await?;
    ensure_not_cancelled()?;
    project.cues = dialogue_from_cues(&transcript.cues, &project.source_language);
    project.translation_guide = TranslationGuide::default();
    project.character_bible = CharacterBible::default();
    let destination = project_dir(app, project_id)?.join("source.srt");
    srt::write(&destination, &transcript.cues)?;
    project.assets.source_srt = Some(destination.to_string_lossy().into_owned());
    project.status = "analyzed".into();
    project.updated_at_ms = now_ms();

    project.speakers = default_speakers();
    project.warnings.push(
        "Mọi cue đang dùng Nhân vật 1. Có thể thêm nhân vật và gán lại từng cue trong panel Nhân vật."
            .into(),
    );
    save(app, &project)?;
    emit_progress(app, "complete", 1, 1, 100.0, "Phân tích phim hoàn tất");
    Ok(project)
}

fn project_style(profile: &str) -> &str {
    match profile {
        "modern" => "modern",
        "anime" => "anime",
        "historical" => "historical",
        "documentary" => "documentary",
        _ => "auto",
    }
}

#[cfg(test)]
fn build_bible(project: &DubbingProject) -> CharacterBible {
    let relationships = project
        .speakers
        .iter()
        .filter(|speaker| !speaker.notes.trim().is_empty())
        .map(|speaker| format!("{}: {}", speaker.display_name, speaker.notes.trim()))
        .collect::<Vec<_>>();
    let address_rules = project
        .glossary
        .iter()
        .map(|entry| format!("{} → {}", entry.source.trim(), entry.target.trim()))
        .collect::<Vec<_>>();
    CharacterBible {
        summary: format!(
            "Dự án {} gồm {} nhân vật và {} cue, phong cách {}.",
            project.title,
            project.speakers.len(),
            project.cues.len(),
            project.profile
        ),
        relationships,
        address_rules,
    }
}

fn assign_translation_metadata(project: &mut DubbingProject) {
    let source = project
        .cues
        .iter()
        .map(|cue| Cue {
            index: cue.index,
            start_ms: cue.start_ms,
            end_ms: cue.end_ms,
            text: if cue.source_text.trim().is_empty() {
                cue.translated_text.clone()
            } else {
                cue.source_text.clone()
            },
        })
        .collect::<Vec<_>>();
    let groups = pipeline::utterance_groups(&source);
    for (cue, group) in project.cues.iter_mut().zip(groups) {
        cue.utterance_group_id = group;
        cue.scene_id = project
            .translation_guide
            .scenes
            .iter()
            .find(|scene| cue.index >= scene.start_index && cue.index <= scene.end_index)
            .map(|scene| scene.id.clone())
            .unwrap_or_default();
    }
}

fn apply_prepared_guide(
    project: &mut DubbingProject,
    prepared: pipeline::PreparedTranslationGuide,
) {
    let previous_notes = project.translation_guide.user_notes.clone();
    let mut address_rules = prepared.address_rules.clone();
    for rule in &project.translation_guide.address_rules {
        if !address_rules.contains(rule) {
            address_rules.push(rule.clone());
        }
    }
    project.translation_guide = TranslationGuide {
        revision: project.translation_guide.revision.saturating_add(1).max(1),
        synopsis: prepared.synopsis.clone(),
        tone: prepared.tone,
        user_notes: previous_notes,
        address_rules: address_rules.clone(),
        scenes: prepared
            .scenes
            .into_iter()
            .map(|scene| SceneContext {
                id: scene.id,
                start_index: scene.start_index,
                end_index: scene.end_index,
                summary: scene.summary,
                tone: scene.tone,
                status: "ready".into(),
            })
            .collect(),
        prepared_at_ms: now_ms(),
    };
    project.character_bible = CharacterBible {
        summary: prepared.synopsis,
        relationships: prepared.relationships,
        address_rules,
    };
    assign_translation_metadata(project);
}

pub async fn prepare_translation_context(
    app: &AppHandle,
    project_id: &str,
) -> Result<DubbingProject, String> {
    begin_job();
    let mut project = load_inner(app, project_id)?;
    let source = project
        .cues
        .iter()
        .filter(|cue| !cue.source_text.trim().is_empty())
        .map(|cue| Cue {
            index: cue.index,
            start_ms: cue.start_ms,
            end_ms: cue.end_ms,
            text: cue.source_text.clone(),
        })
        .collect::<Vec<_>>();
    if source.is_empty() {
        return Err(
            "SRT hiện tại đã là tiếng Việt nên không cần chuẩn bị ngữ cảnh. Hãy chuyển sang bước 3 để tạo giọng."
                .into(),
        );
    }
    let manual_context = serde_json::to_string(&serde_json::json!({
        "speakers": project.speakers.iter().map(|speaker| serde_json::json!({
            "name": speaker.display_name,
            "notes": speaker.notes
        })).collect::<Vec<_>>(),
        "glossary": project.glossary,
        "userNotes": project.translation_guide.user_notes,
    }))
    .map_err(|error| error.to_string())?;
    let prepared = pipeline::prepare_translation_guide(
        app,
        &source,
        &project.source_language,
        project_style(&project.profile),
        &manual_context,
    )
    .await?;
    ensure_not_cancelled()?;
    apply_prepared_guide(&mut project, prepared);
    project.status = "translation-context-ready".into();
    project.updated_at_ms = now_ms();
    save(app, &project)?;
    Ok(project)
}

fn memory_tokens(text: &str) -> std::collections::HashSet<String> {
    let lowered = text.to_lowercase();
    let mut tokens = lowered
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| token.chars().count() > 2)
        .map(ToString::to_string)
        .collect::<std::collections::HashSet<_>>();
    let compact = lowered
        .chars()
        .filter(|character| character.is_alphanumeric())
        .collect::<Vec<_>>();
    for window in compact.windows(3) {
        tokens.insert(window.iter().collect());
    }
    tokens
}

fn relevant_translation_memory(
    app: &AppHandle,
    project: &DubbingProject,
    source: &[Cue],
) -> Result<Vec<TranslationMemoryEntry>, String> {
    if !translation_memory_enabled(app)? {
        return Ok(Vec::new());
    }
    let source_tokens = memory_tokens(
        &source
            .iter()
            .take(160)
            .map(|cue| cue.text.as_str())
            .collect::<Vec<_>>()
            .join(" "),
    );
    let mut scored = read_translation_memory(app)?
        .into_iter()
        .filter(|entry| {
            entry.source_language == project.source_language && entry.profile == project.profile
        })
        .map(|entry| {
            let tokens = memory_tokens(&entry.source_text);
            let overlap = tokens.intersection(&source_tokens).count();
            let score = overlap as f64 / tokens.len().max(1) as f64;
            (score, entry)
        })
        .filter(|(score, _)| *score > 0.0)
        .collect::<Vec<_>>();
    scored.sort_by(|left, right| {
        right
            .0
            .partial_cmp(&left.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| right.1.created_at_ms.cmp(&left.1.created_at_ms))
    });
    Ok(scored.into_iter().take(5).map(|(_, entry)| entry).collect())
}

async fn translate_inner(
    app: &AppHandle,
    project_id: &str,
    only_scene_id: Option<&str>,
) -> Result<DubbingProject, String> {
    begin_job();
    let mut project = load_inner(app, project_id)?;
    if project.cues.is_empty() {
        return Err("Dự án chưa có transcript hoặc SRT nguồn.".to_string());
    }
    let source = project
        .cues
        .iter()
        .map(|cue| Cue {
            index: cue.index,
            start_ms: cue.start_ms,
            end_ms: cue.end_ms,
            text: cue.source_text.clone(),
        })
        .collect::<Vec<_>>();
    if source.iter().all(|cue| cue.text.trim().is_empty()) {
        return Err("SRT hiện tại đã là tiếng Việt, không cần dịch.".to_string());
    }
    if project.translation_guide.scenes.is_empty() {
        return Err("Chưa có ngữ cảnh toàn phim. Hãy bấm Chuẩn bị ngữ cảnh trước khi dịch.".into());
    }
    if let Some(scene_id) = only_scene_id {
        if !project
            .translation_guide
            .scenes
            .iter()
            .any(|scene| scene.id == scene_id)
        {
            return Err("Không tìm thấy cảnh cần dịch lại.".into());
        }
    }
    assign_translation_metadata(&mut project);
    let memory = relevant_translation_memory(app, &project, &source)?;
    let project_context = serde_json::to_string(&serde_json::json!({
        "profile": project.profile,
        "characterBible": project.character_bible,
        "translationGuide": project.translation_guide,
        "onlySceneId": only_scene_id,
        "acceptedTranslations": if only_scene_id.is_some() {
            project.cues.iter().filter(|cue| !cue.translated_text.trim().is_empty()).map(|cue| serde_json::json!({
                "id": cue.index,
                "text": cue.translated_text,
            })).collect::<Vec<_>>()
        } else {
            Vec::new()
        },
        "memoryExamples": memory.iter().map(|entry| serde_json::json!({
            "source": entry.source_text,
            "machine": entry.machine_text,
            "preferred": entry.corrected_text,
        })).collect::<Vec<_>>(),
        "speakers": project.speakers.iter().map(|speaker| serde_json::json!({
            "id": speaker.id,
            "name": speaker.display_name,
            "notes": speaker.notes,
        })).collect::<Vec<_>>(),
        "glossary": project.glossary,
    }))
    .map_err(|error| error.to_string())?;
    let source_path = project_dir(app, project_id)?.join("source-for-translation.srt");
    srt::write(&source_path, &source)?;
    emit_progress(
        app,
        "project-profile",
        0,
        1,
        2.0,
        "Đang chuẩn bị cảnh, glossary và bộ nhớ văn phong...",
    );
    ensure_not_cancelled()?;
    let result = pipeline::translate_srt(
        app,
        &source_path.to_string_lossy(),
        &project.source_language,
        "natural",
        project_style(&project.profile),
        Some(&project_context),
    )
    .await?;
    ensure_not_cancelled()?;
    let translated: HashMap<usize, String> = result
        .output
        .cues
        .iter()
        .filter(|cue| !cue.text.trim().is_empty())
        .map(|cue| (cue.index, cue.text.clone()))
        .collect();
    for cue in &mut project.cues {
        if only_scene_id.is_some_and(|scene_id| cue.scene_id != scene_id) {
            continue;
        }
        if let Some(text) = translated.get(&cue.index) {
            cue.translated_text.clone_from(text);
            cue.machine_translated_text.clone_from(text);
            cue.status = "translated".into();
            cue.warnings.retain(|warning| {
                !warning.contains("biên tập") && !warning.contains("kiểm tra mạch thoại")
            });
            cue.audio_path = None;
            cue.audio_duration_ms = None;
        }
    }
    let mut fallback_scenes = Vec::new();
    for warning in result.warnings {
        if let Some(index) = warning.cue_index {
            if let Some(cue) = project.cues.iter_mut().find(|cue| cue.index == index) {
                cue.warnings.push(warning.message.clone());
                cue.status = "translation-warning".into();
                if !cue.scene_id.is_empty() && !fallback_scenes.contains(&cue.scene_id) {
                    fallback_scenes.push(cue.scene_id.clone());
                }
            }
        }
        project.warnings.push(warning.message);
    }
    if let Some(scene_id) = only_scene_id {
        if let Some(scene) = project
            .translation_guide
            .scenes
            .iter_mut()
            .find(|scene| scene.id == scene_id)
        {
            scene.status = if fallback_scenes.contains(&scene.id) {
                "fallback"
            } else {
                "translated"
            }
            .into();
        }
    } else {
        for scene in &mut project.translation_guide.scenes {
            scene.status = if fallback_scenes.contains(&scene.id) {
                "fallback"
            } else {
                "translated"
            }
            .into();
        }
    }
    write_project_translation_srt(app, &mut project)?;
    project.status = "translated".into();
    project.updated_at_ms = now_ms();
    save(app, &project)?;
    emit_progress(
        app,
        "complete",
        1,
        1,
        100.0,
        "Dịch và biên tập lời thoại hoàn tất",
    );
    Ok(project)
}

pub async fn translate(app: &AppHandle, project_id: &str) -> Result<DubbingProject, String> {
    let project = load_inner(app, project_id)?;
    if project.translation_guide.scenes.is_empty() {
        prepare_translation_context(app, project_id).await?;
    }
    translate_inner(app, project_id, None).await
}

fn raw_audio_cache_key(text: &str, voice: &str) -> String {
    let spoken_text = pipeline::speech_text(text);
    let mut digest = Sha256::new();
    digest.update(b"vieneu-v3-turbo-q8-sea-g2p-raw-v2");
    digest.update([0]);
    digest.update(spoken_text.as_bytes());
    digest.update([0]);
    digest.update(voice.as_bytes());
    digest.update([0]);
    digest.update(b"temperature=.8;top-k=25;top-p=.95;repeat=1.2");
    hex::encode(digest.finalize())
}

fn processed_audio_cache_key(raw_key: &str, cue: &DialogueCue, settings: &AudioSettings) -> String {
    let mut digest = Sha256::new();
    digest.update(b"vieneu-final-v2");
    digest.update([0]);
    digest.update(raw_key.as_bytes());
    digest.update((cue.end_ms - cue.start_ms).to_le_bytes());
    digest.update(cue.speed.to_le_bytes());
    digest.update(cue.volume.to_le_bytes());
    digest.update(settings.min_tempo.to_le_bytes());
    digest.update(settings.max_tempo.to_le_bytes());
    hex::encode(digest.finalize())
}

#[derive(Debug)]
struct ManagedCommandOutput {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[derive(Debug)]
struct ManagedCommandError {
    code: String,
    message: String,
    exit_code: Option<i32>,
    timed_out: bool,
    elapsed_ms: u64,
    details: String,
}

impl ManagedCommandError {
    fn display(&self) -> String {
        if self.details.trim().is_empty() {
            self.message.clone()
        } else {
            format!("{}: {}", self.message, self.details.trim())
        }
    }
}

enum ManagedEvent {
    Line(String),
    Heartbeat,
}

fn push_tail(target: &mut String, line: &str) {
    target.push_str(line);
    target.push('\n');
    if target.len() > 8_192 {
        let split = target.len() - 8_192;
        let boundary = target
            .char_indices()
            .find(|(index, _)| *index >= split)
            .map(|(index, _)| index)
            .unwrap_or(split);
        target.drain(..boundary);
    }
}

async fn read_process_lines<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    is_stderr: bool,
    sender: mpsc::UnboundedSender<(bool, String)>,
) {
    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if sender.send((is_stderr, line)).is_err() {
            break;
        }
    }
}

async fn run_managed_command<F>(
    command: &mut Command,
    label: &str,
    stall_timeout: Duration,
    hard_timeout: Duration,
    mut on_event: F,
) -> Result<ManagedCommandOutput, ManagedCommandError>
where
    F: FnMut(ManagedEvent),
{
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(|error| ManagedCommandError {
        code: "spawn-failed".into(),
        message: format!("Không chạy được {label}"),
        exit_code: None,
        timed_out: false,
        elapsed_ms: 0,
        details: error.to_string(),
    })?;
    let (sender, mut receiver) = mpsc::unbounded_channel();
    if let Some(stdout) = child.stdout.take() {
        tokio::spawn(read_process_lines(stdout, false, sender.clone()));
    }
    if let Some(stderr) = child.stderr.take() {
        tokio::spawn(read_process_lines(stderr, true, sender.clone()));
    }
    drop(sender);
    let started = Instant::now();
    let mut last_output = Instant::now();
    let mut last_heartbeat = Instant::now();
    let mut stdout_tail = String::new();
    let mut stderr_tail = String::new();

    loop {
        while let Ok((is_stderr, line)) = receiver.try_recv() {
            last_output = Instant::now();
            if is_stderr {
                push_tail(&mut stderr_tail, &line);
            } else {
                push_tail(&mut stdout_tail, &line);
            }
            on_event(ManagedEvent::Line(line));
        }
        if let Some(status) = child.try_wait().map_err(|error| ManagedCommandError {
            code: "wait-failed".into(),
            message: format!("Không đọc được trạng thái {label}"),
            exit_code: None,
            timed_out: false,
            elapsed_ms: started.elapsed().as_millis() as u64,
            details: error.to_string(),
        })? {
            while let Some((is_stderr, line)) = receiver.recv().await {
                if is_stderr {
                    push_tail(&mut stderr_tail, &line);
                } else {
                    push_tail(&mut stdout_tail, &line);
                }
                on_event(ManagedEvent::Line(line));
            }
            let elapsed_ms = started.elapsed().as_millis() as u64;
            if status.success() {
                return Ok(ManagedCommandOutput {
                    stdout: stdout_tail.into_bytes(),
                    stderr: stderr_tail.into_bytes(),
                });
            }
            return Err(ManagedCommandError {
                code: "process-failed".into(),
                message: format!("{label} thất bại"),
                exit_code: status.code(),
                timed_out: false,
                elapsed_ms,
                details: if stderr_tail.trim().is_empty() {
                    stdout_tail
                } else {
                    stderr_tail
                },
            });
        }

        let elapsed = started.elapsed();
        let cancelled = CANCEL_REQUESTED.load(Ordering::Relaxed);
        let hard_expired = elapsed >= hard_timeout;
        let stalled = last_output.elapsed() >= stall_timeout;
        if cancelled || hard_expired || stalled {
            let _ = child.kill().await;
            let _ = child.wait().await;
            return Err(ManagedCommandError {
                code: if cancelled {
                    "cancelled"
                } else if stalled {
                    "stalled"
                } else {
                    "hard-timeout"
                }
                .into(),
                message: if cancelled {
                    format!("Đã hủy {label}")
                } else if stalled {
                    format!("{label} không có tiến triển quá lâu")
                } else {
                    format!("{label} vượt quá thời gian tối đa")
                },
                exit_code: None,
                timed_out: !cancelled,
                elapsed_ms: elapsed.as_millis() as u64,
                details: if stderr_tail.trim().is_empty() {
                    stdout_tail
                } else {
                    stderr_tail
                },
            });
        }
        if last_heartbeat.elapsed() >= Duration::from_secs(2) {
            on_event(ManagedEvent::Heartbeat);
            last_heartbeat = Instant::now();
        }
        sleep(Duration::from_millis(250)).await;
    }
}

fn wav_duration_ms(path: &Path) -> Result<i64, String> {
    let reader = hound::WavReader::open(path)
        .map_err(|error| format!("WAV không hợp lệ {}: {error}", path.display()))?;
    let rate = reader.spec().sample_rate.max(1) as f64;
    Ok((reader.duration() as f64 / rate * 1_000.0).round() as i64)
}

fn valid_wav(path: &Path) -> bool {
    path.is_file() && wav_duration_ms(path).is_ok_and(|duration| duration > 0)
}

#[derive(Clone, Debug)]
struct TtsTask {
    cue_id: String,
    cue_index: usize,
    text: String,
    voice: String,
    raw_key: String,
    raw_path: PathBuf,
    processed_path: PathBuf,
    target_duration_ms: i64,
    speed: f64,
    volume: f64,
    had_markup: bool,
}

#[derive(Deserialize)]
struct BatchManifest {
    requests: Vec<BatchManifestRequest>,
}

#[derive(Deserialize)]
struct BatchManifestRequest {
    id: String,
}

fn job_error(stage: &str, message: impl Into<String>) -> JobError {
    JobError {
        code: "job-failed".into(),
        stage: stage.into(),
        message: message.into(),
        ..JobError::default()
    }
}

fn process_job_error(
    stage: &str,
    error: ManagedCommandError,
    cue: Option<&TtsTask>,
    voice: Option<&str>,
    batch: Option<usize>,
) -> JobError {
    JobError {
        code: error.code,
        stage: stage.into(),
        message: error.message,
        cue_id: cue.map(|item| item.cue_id.clone()),
        cue_index: cue.map(|item| item.cue_index),
        voice: voice.map(str::to_string),
        batch,
        exit_code: error.exit_code,
        timed_out: error.timed_out,
        elapsed_ms: error.elapsed_ms,
        details: (!error.details.trim().is_empty()).then_some(error.details),
    }
}

fn fail_job(
    app: &AppHandle,
    project: &mut DubbingProject,
    job: &mut LongJobState,
    error: JobError,
) -> String {
    job.status = if error.code == "cancelled" {
        "cancelled"
    } else {
        "failed"
    }
    .into();
    job.stage = error.stage.clone();
    job.message = error.message.clone();
    job.error = Some(error.clone());
    let _ = checkpoint_job(app, project, job);
    record_job_event(app, "error", "project.job", job);
    if let Some(details) = error.details {
        format!("{}\n{}", error.message, details.trim())
    } else {
        error.message
    }
}

async fn detect_audio_backend(executable: &Path) -> Result<String, ManagedCommandError> {
    let output = run_managed_command(
        Command::new(executable).arg("--version"),
        "kiểm tra audio.cpp",
        Duration::from_secs(10),
        Duration::from_secs(10),
        |_| {},
    )
    .await?;
    let version = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .to_ascii_lowercase();
    Ok(
        if version.contains("metal") && !version.contains("cpu only") {
            "metal"
        } else {
            "cpu"
        }
        .into(),
    )
}

fn ensure_writable(directory: &Path) -> Result<(), String> {
    std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let probe = directory.join(format!(".write-probe-{}", Uuid::new_v4()));
    std::fs::write(&probe, b"ok")
        .map_err(|error| format!("Không thể ghi vào {}: {error}", directory.display()))?;
    std::fs::remove_file(probe).map_err(|error| error.to_string())
}

pub async fn preview_voice(app: &AppHandle, voice: &str) -> Result<String, String> {
    const PREVIEW_TEXT: &str = "Sushi đẹp trai, ham ăn, ham chơi, thích anime!";
    if !matches!(
        voice,
        "thuy_dung"
            | "thuc_doan"
            | "my_duyen"
            | "kim_thanh"
            | "thai_son"
            | "minh_triet"
            | "duc_tri"
    ) {
        return Err("Voice preset không hợp lệ.".into());
    }
    begin_job();
    let cache = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("voice-previews");
    ensure_writable(&cache)?;
    let key = raw_audio_cache_key(PREVIEW_TEXT, voice);
    let destination = cache.join(format!("{key}.wav"));
    if valid_wav(&destination) {
        return Ok(destination.to_string_lossy().into_owned());
    }

    let executable = pipeline::find_executable(app, &["audiocpp_cli", "audiocpp-cli"])
        .ok_or_else(|| "Không tìm thấy audio.cpp.".to_string())?;
    let g2p_library = pipeline::find_executable(app, &["libsea_g2p_rs.dylib"])
        .ok_or_else(|| "Thiếu thư viện SEA-G2P.".to_string())?;
    let g2p_dictionary = pipeline::find_executable(app, &["sea_g2p.bin"])
        .ok_or_else(|| "Thiếu từ điển SEA-G2P.".to_string())?;
    let model = models::model_path(app, "vieneu-v3-turbo-q8", "vieneu-v3-turbo-q8_0.gguf")?;
    if !model.is_file() {
        return Err("Chưa tải VieNeu-TTS v3 Turbo trong Model Manager.".into());
    }
    let reference_codes = models::model_path(
        app,
        "vieneu-v3-turbo-q8",
        &format!("voices/{voice}/ref_codes.txt"),
    )?;
    let speaker_embedding = models::model_path(
        app,
        "vieneu-v3-turbo-q8",
        &format!("voices/{voice}/speaker.emb.txt"),
    )?;
    if !reference_codes.is_file() || !speaker_embedding.is_file() {
        return Err(format!("Thiếu dữ liệu voice preset {voice}."));
    }
    let backend = detect_audio_backend(&executable)
        .await
        .map_err(|error| error.display())?;
    let g2p_library_option = format!("vieneu_v3_turbo.g2p_library={}", g2p_library.display());
    let g2p_dictionary_option = format!("vieneu_v3_turbo.g2p_dict={}", g2p_dictionary.display());
    let reference_option = format!("reference_codes_file={}", reference_codes.display());
    let embedding_option = format!("speaker_embedding_file={}", speaker_embedding.display());
    let temporary = destination.with_extension("part.wav");
    let _ = std::fs::remove_file(&temporary);
    let mut command = Command::new(executable);
    command
        .args([
            "--task",
            "tts",
            "--family",
            "vieneu_v3_turbo",
            "--backend",
            &backend,
        ])
        .arg("--model")
        .arg(model)
        .args(["--session-option", &g2p_library_option])
        .args(["--session-option", &g2p_dictionary_option])
        .args([
            "--text",
            PREVIEW_TEXT,
            "--temperature",
            "0.8",
            "--top-k",
            "25",
            "--top-p",
            "0.95",
            "--repetition-penalty",
            "1.2",
            "--request-option",
            &reference_option,
        ])
        .args(["--request-option", &embedding_option, "--out"])
        .arg(&temporary)
        .args(["--log", "--metrics"]);
    let result = run_managed_command(
        &mut command,
        "VieNeu-TTS nghe thử preset",
        Duration::from_secs(90),
        Duration::from_secs(5 * 60),
        |_| {},
    )
    .await;
    if let Err(error) = result {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.display());
    }
    if !valid_wav(&temporary) {
        let _ = std::fs::remove_file(&temporary);
        return Err("VieNeu-TTS không tạo được WAV nghe thử hợp lệ.".into());
    }
    std::fs::rename(&temporary, &destination).map_err(|error| {
        let _ = std::fs::remove_file(&temporary);
        format!("Không lưu được audio nghe thử: {error}")
    })?;
    Ok(destination.to_string_lossy().into_owned())
}

fn make_tts_tasks(
    project: &DubbingProject,
    selected: &HashSet<String>,
    raw_cache: &Path,
    processed_cache: &Path,
) -> Result<Vec<TtsTask>, String> {
    let speaker_voice = project
        .speakers
        .iter()
        .map(|speaker| (speaker.id.as_str(), speaker.voice_preset.as_str()))
        .collect::<HashMap<_, _>>();
    project
        .cues
        .iter()
        .filter(|cue| selected.contains(&cue.id))
        .map(|cue| {
            if cue.translated_text.trim().is_empty() {
                return Err(format!("Cue {} không có lời tiếng Việt.", cue.index));
            }
            if cue.start_ms < 0 || cue.end_ms <= cue.start_ms {
                return Err(format!("Cue {} có timestamp không hợp lệ.", cue.index));
            }
            let speech_text = pipeline::speech_text(&cue.translated_text);
            if speech_text.is_empty() {
                return Err(format!(
                    "Cue {} không còn nội dung đọc sau khi bỏ tag phụ đề.",
                    cue.index
                ));
            }
            let had_markup = speech_text
                != cue
                    .translated_text
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
            let voice = speaker_voice
                .get(cue.speaker_id.as_str())
                .ok_or_else(|| format!("Cue {} tham chiếu nhân vật không tồn tại.", cue.index))?
                .to_string();
            if voice.trim().is_empty() {
                return Err(format!("Cue {} chưa được gán voice preset.", cue.index));
            }
            let raw_key = raw_audio_cache_key(&speech_text, &voice);
            let processed_key = processed_audio_cache_key(&raw_key, cue, &project.audio_settings);
            Ok(TtsTask {
                cue_id: cue.id.clone(),
                cue_index: cue.index,
                text: speech_text,
                voice,
                raw_path: raw_cache.join(format!("{raw_key}.wav")),
                processed_path: processed_cache.join(format!("{processed_key}.wav")),
                raw_key,
                target_duration_ms: cue.end_ms - cue.start_ms,
                speed: cue.speed,
                volume: cue.volume,
                had_markup,
            })
        })
        .collect()
}

async fn generate_tts_batch(
    app: &AppHandle,
    job: &mut LongJobState,
    executable: &Path,
    model: &Path,
    g2p_library: &Path,
    g2p_dictionary: &Path,
    backend: &str,
    voice: &str,
    batch_number: usize,
    tasks: &[TtsTask],
    workspace: &Path,
) -> Result<(), JobError> {
    let input = workspace.join("input");
    let output = workspace.join("output");
    std::fs::create_dir_all(&input).map_err(|error| job_error("tts-batch", error.to_string()))?;
    std::fs::create_dir_all(&output).map_err(|error| job_error("tts-batch", error.to_string()))?;
    let manifest = workspace.join("manifest.json");
    let mut ids = HashMap::new();
    for task in tasks {
        let id = format!("cue-{:05}-{}", task.cue_index, &task.raw_key[..10]);
        std::fs::write(input.join(format!("{id}.txt")), &task.text)
            .map_err(|error| job_error("tts-batch", error.to_string()))?;
        ids.insert(id, task);
    }
    let reference_codes = models::model_path(
        app,
        "vieneu-v3-turbo-q8",
        &format!("voices/{voice}/ref_codes.txt"),
    )
    .map_err(|error| job_error("preflight", error))?;
    let speaker_embedding = models::model_path(
        app,
        "vieneu-v3-turbo-q8",
        &format!("voices/{voice}/speaker.emb.txt"),
    )
    .map_err(|error| job_error("preflight", error))?;
    let g2p_library_option = format!("vieneu_v3_turbo.g2p_library={}", g2p_library.display());
    let g2p_dictionary_option = format!("vieneu_v3_turbo.g2p_dict={}", g2p_dictionary.display());
    let reference_option = format!("reference_codes_file={}", reference_codes.display());
    let embedding_option = format!("speaker_embedding_file={}", speaker_embedding.display());
    let mut command = Command::new(executable);
    command
        .args([
            "--task",
            "tts",
            "--family",
            "vieneu_v3_turbo",
            "--backend",
            backend,
        ])
        .arg("--model")
        .arg(model)
        .args(["--session-option", &g2p_library_option])
        .args(["--session-option", &g2p_dictionary_option])
        .args(["--temperature", "0.8", "--top-k", "25", "--top-p", "0.95"])
        .args([
            "--repetition-penalty",
            "1.2",
            "--request-option",
            &reference_option,
        ])
        .args(["--request-option", &embedding_option, "--batch-text-dir"])
        .arg(&input)
        .arg("--out-dir")
        .arg(&output)
        .arg("--batch-manifest-out")
        .arg(&manifest)
        .args(["--log", "--metrics"]);
    job.stage = "tts-batch".into();
    job.message = format!(
        "Đang tạo batch {batch_number} · {} · {} cue",
        voice,
        tasks.len()
    );
    refresh_job_timing(job);
    emit_long_job(app, job);
    let app_for_events = app.clone();
    let job_id = job.id.clone();
    let kind = job.kind.clone();
    let status = job.status.clone();
    let stage = job.stage.clone();
    let total = job.total;
    let processed = job.processed;
    let generated = job.generated;
    let reused = job.reused;
    let message = job.message.clone();
    let job_started_at = job.started_at_ms;
    run_managed_command(
        &mut command,
        "VieNeu-TTS batch",
        Duration::from_secs(90),
        Duration::from_secs(15 * 60),
        move |event| {
            if let ManagedEvent::Heartbeat = event {
                let percent = if total == 0 {
                    0.0
                } else {
                    processed as f64 / total as f64 * 100.0
                };
                let _ = app_for_events.emit(
                    "project-progress",
                    ProjectProgress {
                        job_id: Some(job_id.clone()),
                        kind: Some(kind.clone()),
                        status: status.clone(),
                        stage: stage.clone(),
                        current: processed,
                        total,
                        processed,
                        generated,
                        reused,
                        percent,
                        message: message.clone(),
                        current_cue_index: None,
                        elapsed_seconds: now_ms().saturating_sub(job_started_at) / 1_000,
                        eta_seconds: None,
                    },
                );
            }
        },
    )
    .await
    .map_err(|error| {
        process_job_error(
            "tts-batch",
            error,
            tasks.first(),
            Some(voice),
            Some(batch_number),
        )
    })?;
    let parsed: BatchManifest = serde_json::from_slice(
        &std::fs::read(&manifest).map_err(|error| job_error("tts-manifest", error.to_string()))?,
    )
    .map_err(|error| {
        job_error(
            "tts-manifest",
            format!("Manifest batch không hợp lệ: {error}"),
        )
    })?;
    let returned = parsed
        .requests
        .into_iter()
        .map(|request| request.id)
        .collect::<HashSet<_>>();
    for (id, task) in ids {
        if !returned.contains(&id) {
            return Err(job_error(
                "tts-manifest",
                format!("Batch thiếu output cho cue {}.", task.cue_index),
            ));
        }
        let generated = output.join(format!("{id}.wav"));
        if !valid_wav(&generated) {
            return Err(job_error(
                "tts-manifest",
                format!("WAV của cue {} bị thiếu hoặc hỏng.", task.cue_index),
            ));
        }
        if !valid_wav(&task.raw_path) {
            std::fs::rename(&generated, &task.raw_path)
                .map_err(|error| job_error("tts-cache", error.to_string()))?;
        }
    }
    Ok(())
}

async fn normalize_task(
    ffmpeg: PathBuf,
    task: TtsTask,
    settings: AudioSettings,
) -> Result<(TtsTask, i64, bool), JobError> {
    let source_duration_ms =
        wav_duration_ms(&task.raw_path).map_err(|error| job_error("normalizing", error))?;
    let source_duration = source_duration_ms as f64 / 1_000.0;
    let target_duration = task.target_duration_ms.max(100) as f64 / 1_000.0;
    let tempo = (source_duration / target_duration).clamp(settings.min_tempo, settings.max_tempo)
        * task.speed.clamp(0.8, 1.25);
    let filter = format!(
        "aresample=48000,atempo={:.5},volume={:.4},alimiter=limit=0.95",
        tempo.clamp(0.5, 2.0),
        task.volume.clamp(0.1, 2.0)
    );
    let temporary = task.processed_path.with_extension("part.wav");
    let mut command = Command::new(ffmpeg);
    command
        .args(["-y", "-loglevel", "error", "-i"])
        .arg(&task.raw_path)
        .args([
            "-af",
            &filter,
            "-ac",
            "1",
            "-ar",
            "48000",
            "-c:a",
            "pcm_s16le",
        ])
        .arg(&temporary);
    run_managed_command(
        &mut command,
        "FFmpeg chuẩn hóa cue",
        Duration::from_secs(30),
        Duration::from_secs(2 * 60),
        |_| {},
    )
    .await
    .map_err(|error| {
        process_job_error("normalizing", error, Some(&task), Some(&task.voice), None)
    })?;
    if !valid_wav(&temporary) {
        let _ = std::fs::remove_file(&temporary);
        return Err(job_error(
            "normalizing",
            format!("FFmpeg tạo WAV hỏng cho cue {}.", task.cue_index),
        ));
    }
    std::fs::rename(&temporary, &task.processed_path)
        .map_err(|error| job_error("normalizing", error.to_string()))?;
    let final_duration =
        wav_duration_ms(&task.processed_path).map_err(|error| job_error("normalizing", error))?;
    let overflow = source_duration > target_duration * settings.max_tempo;
    Ok((task, final_duration, overflow))
}

pub async fn synthesize(
    app: &AppHandle,
    request: SynthesizeRequest,
) -> Result<SynthesizeResult, String> {
    begin_job();
    let mut project = load_inner(app, &request.project_id)?;
    let eligible = project
        .cues
        .iter()
        .filter(|cue| {
            !cue.translated_text.trim().is_empty()
                && (request.cue_ids.is_empty() || request.cue_ids.contains(&cue.id))
        })
        .count();
    let selected = project
        .cues
        .iter()
        .filter(|cue| {
            !cue.translated_text.trim().is_empty()
                && request
                    .max_seconds
                    .map_or(true, |seconds| cue.start_ms < seconds as i64 * 1_000)
                && (request.cue_ids.is_empty() || request.cue_ids.contains(&cue.id))
        })
        .map(|cue| cue.id.clone())
        .collect::<HashSet<_>>();
    let skipped = eligible.saturating_sub(selected.len());
    if selected.is_empty() {
        return Err("Không có cue tiếng Việt hợp lệ trong phạm vi đã chọn.".into());
    }
    let mut job = start_long_job(
        "synthesize",
        selected.len(),
        "Đang kiểm tra engine, model, voice và dung lượng...",
    );
    checkpoint_job(app, &mut project, &mut job)?;
    record_job_event(app, "info", "project.job.start", &job);

    let result: Result<(usize, usize), JobError> = async {
        let directory = project_dir(app, &request.project_id).map_err(|error| job_error("preflight", error))?;
        let raw_cache = directory.join("audio/raw");
        let processed_cache = directory.join("audio/processed");
        ensure_writable(&raw_cache).map_err(|error| job_error("preflight", error))?;
        ensure_writable(&processed_cache).map_err(|error| job_error("preflight", error))?;
        let executable = pipeline::find_executable(app, &["audiocpp_cli", "audiocpp-cli"])
            .ok_or_else(|| job_error("preflight", "Không tìm thấy audio.cpp."))?;
        let ffmpeg = pipeline::find_executable(app, &["ffmpeg"])
            .ok_or_else(|| job_error("preflight", "Không tìm thấy FFmpeg."))?;
        let g2p_library = pipeline::find_executable(app, &["libsea_g2p_rs.dylib"])
            .ok_or_else(|| job_error("preflight", "Thiếu thư viện SEA-G2P."))?;
        let g2p_dictionary = pipeline::find_executable(app, &["sea_g2p.bin"])
            .ok_or_else(|| job_error("preflight", "Thiếu từ điển SEA-G2P."))?;
        let model = models::model_path(app, "vieneu-v3-turbo-q8", "vieneu-v3-turbo-q8_0.gguf")
            .map_err(|error| job_error("preflight", error))?;
        if !model.is_file() {
            return Err(job_error("preflight", "Chưa tải VieNeu-TTS v3 Turbo trong Model Manager."));
        }
        let backend = detect_audio_backend(&executable).await
            .map_err(|error| process_job_error("preflight", error, None, None, None))?;
        let tasks = make_tts_tasks(&project, &selected, &raw_cache, &processed_cache)
            .map_err(|error| job_error("preflight", error))?;
        for voice in tasks.iter().map(|task| task.voice.as_str()).collect::<HashSet<_>>() {
            let reference = models::model_path(app, "vieneu-v3-turbo-q8", &format!("voices/{voice}/ref_codes.txt"))
                .map_err(|error| job_error("preflight", error))?;
            let embedding = models::model_path(app, "vieneu-v3-turbo-q8", &format!("voices/{voice}/speaker.emb.txt"))
                .map_err(|error| job_error("preflight", error))?;
            if !reference.is_file() || !embedding.is_file() {
                return Err(job_error("preflight", format!("Thiếu dữ liệu voice preset {voice}.")));
            }
        }

        let mut generated = 0usize;
        let mut reused = 0usize;
        let mut pending = Vec::new();
        for task in tasks {
            let existing = project.cues.iter().find(|cue| cue.id == task.cue_id)
                .and_then(|cue| cue.audio_path.as_deref()).map(PathBuf::from)
                .filter(|path| (!task.had_markup || path == &task.processed_path) && valid_wav(path));
            let cached = valid_wav(&task.processed_path).then_some(task.processed_path.clone());
            if let Some(path) = existing.or(cached) {
                let duration = wav_duration_ms(&path).map_err(|error| job_error("preflight", error))?;
                if let Some(cue) = project.cues.iter_mut().find(|cue| cue.id == task.cue_id) {
                    cue.audio_path = Some(path.to_string_lossy().into_owned());
                    cue.audio_duration_ms = Some(duration);
                    cue.status = "voiced".into();
                }
                reused += 1;
                job.reused = reused;
                job.processed += 1;
            } else {
                pending.push(task);
            }
        }
        let required = pending.len() as u64 * 2_500_000 + 1_073_741_824;
        let available = fs2::available_space(&directory)
            .map_err(|error| job_error("preflight", format!("Không đo được dung lượng trống: {error}")))?;
        if available < required {
            return Err(job_error("preflight", format!("Không đủ dung lượng: cần khoảng {:.1} GB, hiện còn {:.1} GB.", required as f64 / 1e9, available as f64 / 1e9)));
        }
        job.message = format!("Preflight xong · {} · {} cue cache · {} cue cần tạo", backend.to_uppercase(), reused, pending.len());
        checkpoint_job(app, &mut project, &mut job).map_err(|error| job_error("checkpoint", error))?;

        let mut grouped = BTreeMap::<String, Vec<TtsTask>>::new();
        for task in pending { grouped.entry(task.voice.clone()).or_default().push(task); }
        let mut batch_number = 0usize;
        for (voice, voice_tasks) in grouped {
            for batch in voice_tasks.chunks(32) {
                ensure_not_cancelled().map_err(|_| {
                    let mut error = job_error("tts-batch", "Đã hủy tạo giọng."); error.code = "cancelled".into(); error
                })?;
                batch_number += 1;
                let workspace = directory.join("jobs").join(&job.id).join(format!("batch-{batch_number:04}"));
                let missing_raw = batch.iter().filter(|task| !valid_wav(&task.raw_path)).cloned().collect::<Vec<_>>();
                if !missing_raw.is_empty() {
                    generate_tts_batch(app, &mut job, &executable, &model, &g2p_library, &g2p_dictionary, &backend, &voice, batch_number, &missing_raw, &workspace).await?;
                }
                job.stage = "normalizing".into();
                job.message = format!("Chuẩn hóa batch {batch_number} · tối đa 4 tiến trình FFmpeg");
                emit_long_job(app, &job);
                let mut normalized = Vec::new();
                for group in batch.chunks(4) {
                    let mut futures = FuturesUnordered::new();
                    for task in group.iter().cloned() {
                        futures.push(normalize_task(
                            ffmpeg.clone(),
                            task,
                            project.audio_settings.clone(),
                        ));
                    }
                    let mut first_error = None;
                    while let Some(outcome) = futures.next().await {
                        match outcome {
                            Ok(value) => normalized.push(value),
                            Err(error) if first_error.is_none() => {
                                first_error = Some(error);
                                CANCEL_REQUESTED.store(true, Ordering::Relaxed);
                            }
                            Err(_) => {}
                        }
                    }
                    if let Some(error) = first_error {
                        if error.code != "cancelled" {
                            CANCEL_REQUESTED.store(false, Ordering::Relaxed);
                        }
                        return Err(error);
                    }
                }
                for (task, duration, overflow) in normalized {
                    if let Some(cue) = project.cues.iter_mut().find(|cue| cue.id == task.cue_id) {
                        cue.warnings.retain(|warning| !warning.starts_with("Audio dài hơn khoảng thoại"));
                        if overflow {
                            cue.warnings.push("Audio dài hơn khoảng thoại ở tốc độ tối đa; nội dung SRT được giữ nguyên.".into());
                        }
                        cue.audio_path = Some(task.processed_path.to_string_lossy().into_owned());
                        cue.audio_duration_ms = Some(duration);
                        cue.status = if overflow { "timing-warning" } else { "voiced" }.into();
                    }
                    generated += 1;
                    job.generated = generated;
                    job.processed += 1;
                    job.current_cue_index = Some(task.cue_index);
                }
                job.message = format!("Đã lưu checkpoint batch {batch_number} · {}/{} cue", job.processed, job.total);
                checkpoint_job(app, &mut project, &mut job).map_err(|error| job_error("checkpoint", error))?;
            }
        }
        Ok((generated, reused))
    }.await;

    match result {
        Ok((generated, reused)) => {
            project.status = "voiced".into();
            job.status = "completed".into();
            job.stage = "complete".into();
            job.processed = job.total;
            job.current_cue_index = None;
            job.message = format!("Tạo giọng hoàn tất · {generated} mới · {reused} dùng lại");
            checkpoint_job(app, &mut project, &mut job)?;
            record_job_event(app, "info", "project.job.complete", &job);
            let warning_count = project
                .cues
                .iter()
                .filter(|cue| cue.status == "timing-warning")
                .count();
            Ok(SynthesizeResult {
                project,
                generated,
                reused,
                skipped,
                warning_count,
                elapsed_seconds: job.elapsed_seconds,
                job,
            })
        }
        Err(error) => Err(fail_job(app, &mut project, &mut job, error)),
    }
}

fn write_zero_samples(
    output: &mut BufWriter<std::fs::File>,
    sample_count: u64,
) -> Result<(), String> {
    static ZERO: [u8; 65_536] = [0; 65_536];
    let mut remaining = sample_count.saturating_mul(2);
    while remaining > 0 {
        let count = remaining.min(ZERO.len() as u64) as usize;
        output
            .write_all(&ZERO[..count])
            .map_err(|error| error.to_string())?;
        remaining -= count as u64;
    }
    Ok(())
}

fn build_narration(
    app: &AppHandle,
    project: &DubbingProject,
    duration_ms: i64,
    destination: &Path,
    job: &mut LongJobState,
) -> Result<usize, String> {
    const SAMPLE_RATE: u64 = 48_000;
    let file = std::fs::File::create(destination).map_err(|error| error.to_string())?;
    let mut output = BufWriter::new(file);
    let mut written = 0_u64;
    let mut rendered = 0;
    let cues = project
        .cues
        .iter()
        .filter(|cue| cue.start_ms < duration_ms && cue.audio_path.is_some())
        .collect::<Vec<_>>();
    let mut last_heartbeat = Instant::now();
    for (position, cue) in cues.iter().enumerate() {
        ensure_not_cancelled()?;
        let Some(path) = cue.audio_path.as_deref() else {
            continue;
        };
        let start = cue.start_ms.max(0) as u64 * SAMPLE_RATE / 1_000;
        if start > written {
            write_zero_samples(&mut output, start - written)?;
            written = start;
        }
        let end = cue.end_ms.min(duration_ms).max(cue.start_ms + 1) as u64 * SAMPLE_RATE / 1_000;
        let available = end.saturating_sub(written);
        let mut reader = hound::WavReader::open(path).map_err(|error| error.to_string())?;
        let samples = reader
            .samples::<i16>()
            .take(available as usize)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        for sample in samples {
            output
                .write_all(&sample.to_le_bytes())
                .map_err(|error| error.to_string())?;
            written += 1;
        }
        rendered += 1;
        if last_heartbeat.elapsed() >= Duration::from_secs(2) || position + 1 == cues.len() {
            job.processed = 35 + ((position + 1) * 20 / cues.len().max(1));
            job.current_cue_index = Some(cue.index);
            job.message = format!("Đang xây narration · {}/{} cue", position + 1, cues.len());
            refresh_job_timing(job);
            emit_long_job(app, job);
            last_heartbeat = Instant::now();
        }
    }
    let total = duration_ms.max(1) as u64 * SAMPLE_RATE / 1_000;
    if written < total {
        write_zero_samples(&mut output, total - written)?;
    }
    output.flush().map_err(|error| error.to_string())?;
    Ok(rendered)
}

async fn try_separate_background(
    app: &AppHandle,
    project: &mut DubbingProject,
    job: &mut LongJobState,
) -> Result<bool, String> {
    if !project.audio_settings.separate_background {
        return Ok(false);
    }
    if let Some(path) = project.assets.background_stem.as_deref() {
        if Path::new(path).exists() {
            return Ok(true);
        }
    }
    let executable = pipeline::find_executable(app, &["audiocpp_cli", "audiocpp-cli"])
        .ok_or_else(|| "Đã bật tách nền nhưng không tìm thấy audio.cpp.".to_string())?;
    let model = models::model_path(app, "mel-band-roformer-q8", "mel-band-roformer-q8_0.gguf")?;
    if !model.exists() {
        return Err("Đã bật tách nền nhưng chưa tải Mel-Band RoFormer trong Model Manager.".into());
    }
    let stems = project_dir(app, &project.id)?.join("stems");
    let output_dir = stems.join(format!("separation-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&output_dir).map_err(|error| error.to_string())?;
    let source_audio = stems.join("source-44k-stereo.wav");
    let ffmpeg = pipeline::find_executable(app, &["ffmpeg"])
        .ok_or_else(|| "Không tìm thấy FFmpeg để chuẩn bị audio tách nền.".to_string())?;
    let mut extract = Command::new(ffmpeg);
    extract
        .args(["-y", "-i"])
        .arg(&project.video_path)
        .args(["-vn", "-ar", "44100", "-ac", "2", "-c:a", "pcm_s16le"])
        .arg(&source_audio);
    run_managed_command(
        &mut extract,
        "trích audio để tách nền",
        Duration::from_secs(120),
        Duration::from_secs(60 * 60),
        |_| {},
    )
    .await
    .map_err(|error| error.display())?;
    let backend = detect_audio_backend(&executable)
        .await
        .map_err(|error| error.display())?;
    let mut command = Command::new(&executable);
    command
        .args([
            "--task",
            "sep",
            "--family",
            "mel_band_roformer",
            "--backend",
            &backend,
        ])
        .arg("--model")
        .arg(&model)
        .arg("--audio")
        .arg(&source_audio)
        .arg("--out-dir")
        .arg(&output_dir)
        .arg("--log");
    job.stage = "separating".into();
    job.message = format!(
        "Đang tách nhạc nền và hiệu ứng · {}",
        backend.to_uppercase()
    );
    emit_long_job(app, job);
    let app_for_events = app.clone();
    let snapshot = job.clone();
    run_managed_command(
        &mut command,
        "tách nhạc nền và hiệu ứng",
        Duration::from_secs(5 * 60),
        Duration::from_secs(4 * 60 * 60),
        move |event| {
            if let ManagedEvent::Heartbeat = event {
                let mut heartbeat = snapshot.clone();
                heartbeat.elapsed_seconds =
                    now_ms().saturating_sub(heartbeat.started_at_ms) / 1_000;
                heartbeat.message = "Đang tách nhạc nền và hiệu ứng...".into();
                emit_long_job(&app_for_events, &heartbeat);
            }
        },
    )
    .await
    .map_err(|error| error.display())?;
    let generated_wavs = std::fs::read_dir(&output_dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.extension()
                        .and_then(|value| value.to_str())
                        .is_some_and(|value| value.eq_ignore_ascii_case("wav"))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let background = generated_wavs
        .iter()
        .find(|path| {
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            name.contains("instrumental")
                || name.contains("accompaniment")
                || name.contains("no_vocal")
                || name.contains("music")
        })
        .cloned();
    let vocal = generated_wavs
        .iter()
        .find(|path| {
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            name.contains("vocal") && !name.contains("no_vocal")
        })
        .cloned();
    let background = background.ok_or_else(|| {
        "Tách nền hoàn tất nhưng không tạo được track nhạc/hiệu ứng. Đã dừng xuất video."
            .to_string()
    })?;
    project.assets.background_stem = Some(background.to_string_lossy().into_owned());
    project.assets.vocal_stem = vocal.map(|path| path.to_string_lossy().into_owned());
    Ok(true)
}

fn partial_output_path(destination: &Path) -> PathBuf {
    let stem = destination
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("output");
    let extension = destination
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("mp4");
    destination.with_file_name(format!("{stem}.part.{extension}"))
}

pub async fn render(
    app: &AppHandle,
    project_id: &str,
    output_path: Option<&str>,
    max_seconds: Option<u64>,
) -> Result<RenderResult, String> {
    begin_job();
    let mut project = load_inner(app, project_id)?;
    let mut job = start_long_job(
        "render",
        100,
        "Đang kiểm tra video, audio cue và nơi lưu...",
    );
    checkpoint_job(app, &mut project, &mut job)?;
    record_job_event(app, "info", "project.job.start", &job);

    let result: Result<(PathBuf, usize, bool, Option<String>), JobError> = async {
        let media = pipeline::inspect_media(app, &project.video_path).await
            .map_err(|error| job_error("preflight", error))?;
        let duration_ms = max_seconds
            .map(|seconds| media.duration_ms.min(seconds as i64 * 1_000))
            .unwrap_or(media.duration_ms)
            .max(1);
        let required_cues = project.cues.iter()
            .filter(|cue| cue.start_ms < duration_ms && !cue.translated_text.trim().is_empty())
            .collect::<Vec<_>>();
        if required_cues.is_empty() {
            return Err(job_error("preflight", "Không có cue tiếng Việt trong phạm vi xuất."));
        }
        for cue in &required_cues {
            let path = cue.audio_path.as_deref().map(Path::new)
                .ok_or_else(|| job_error("preflight", format!("Cue {} chưa có audio. Hãy tiếp tục Tạo toàn bộ.", cue.index)))?;
            if !valid_wav(path) {
                return Err(job_error("preflight", format!("Audio cue {} bị thiếu hoặc hỏng. Hãy tiếp tục Tạo toàn bộ.", cue.index)));
            }
        }
        let directory = project_dir(app, project_id).map_err(|error| job_error("preflight", error))?;
        let destination = output_path.map(PathBuf::from).unwrap_or_else(|| {
            if max_seconds.is_some() { directory.join(format!("preview-5-min-{}.mp4", now_ms())) }
            else { directory.join("dubbed.mp4") }
        });
        let output_dir = destination.parent().unwrap_or(directory.as_path());
        ensure_writable(output_dir).map_err(|error| job_error("preflight", error))?;
        let partial = partial_output_path(&destination);
        let _ = std::fs::remove_file(&partial);
        let ffmpeg = pipeline::find_executable(app, &["ffmpeg"])
            .ok_or_else(|| job_error("preflight", "Không tìm thấy FFmpeg."))?;
        job.processed = 5;
        job.message = format!("Preflight xong · {} cue có audio hợp lệ", required_cues.len());
        checkpoint_job(app, &mut project, &mut job).map_err(|error| job_error("checkpoint", error))?;

        let used_separation = if max_seconds.is_some() || !project.audio_settings.separate_background {
            false
        } else {
            job.processed = 8;
            checkpoint_job(app, &mut project, &mut job).map_err(|error| job_error("checkpoint", error))?;
            try_separate_background(app, &mut project, &mut job).await
                .map_err(|error| job_error("separating", error))?
        };
        ensure_not_cancelled().map_err(|_| { let mut error = job_error("narration", "Đã hủy xuất video."); error.code = "cancelled".into(); error })?;
        job.stage = "narration".into();
        job.processed = 35;
        job.message = "Đang xây narration theo timeline...".into();
        emit_long_job(app, &job);
        let narration = directory.join(format!("narration-{}.pcm", &job.id));
        let rendered_cues = build_narration(app, &project, duration_ms, &narration, &mut job)
            .map_err(|error| {
                let mut value = job_error("narration", error);
                if CANCEL_REQUESTED.load(Ordering::Relaxed) { value.code = "cancelled".into(); }
                value
            })?;
        let subtitle_path = directory.join("vietnamese.srt");
        let subtitles = project.cues.iter().map(|cue| Cue {
            index: cue.index, start_ms: cue.start_ms, end_ms: cue.end_ms, text: cue.translated_text.clone(),
        }).collect::<Vec<_>>();
        srt::write(&subtitle_path, &subtitles).map_err(|error| job_error("narration", error))?;
        let previous_preview = project.assets.preview_output.clone();
        let is_mkv = destination.extension().and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("mkv"));
        let include_subtitle = is_mkv && max_seconds.is_none();
        let has_original_audio = media.audio_codec.is_some();
        let mut command = Command::new(ffmpeg);
        command.args(["-y", "-i"]).arg(&project.video_path);
        if used_separation {
            command.arg("-i").arg(project.assets.background_stem.as_deref().unwrap());
        }
        command.args(["-f", "s16le", "-ar", "48000", "-ac", "1", "-i"]).arg(&narration);
        let voice_input = if used_separation { 2 } else { 1 };
        let subtitle_input = if include_subtitle {
            let index = voice_input + 1;
            command.arg("-i").arg(&subtitle_path);
            Some(index)
        } else { None };
        let filter = if used_separation || has_original_audio {
            let background_input = if used_separation { 1 } else { 0 };
            format!(
                "[{voice_input}:a]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,asetpts=PTS-STARTPTS,volume={:.3},alimiter=limit=0.95,asplit=2[voice_duck][voice_mix];[{background_input}:a]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,asetpts=PTS-STARTPTS,volume={:.3}[bed];[bed][voice_duck]sidechaincompress=threshold=0.018:ratio=8:attack=18:release=260[ducked];[ducked][voice_mix]amix=inputs=2:duration=first:normalize=0,loudnorm=I=-16:TP=-1.5:LRA=11,aformat=sample_rates=48000:channel_layouts=stereo[mix]",
                project.audio_settings.dialogue_volume, project.audio_settings.background_volume,
            )
        } else {
            format!(
                "[{voice_input}:a]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,asetpts=PTS-STARTPTS,volume={:.3},alimiter=limit=0.95,loudnorm=I=-16:TP=-1.5:LRA=11,aformat=sample_rates=48000:channel_layouts=stereo[mix]",
                project.audio_settings.dialogue_volume,
            )
        };
        command.args(["-filter_complex", &filter, "-map", "0:v:0", "-map", "[mix]"]);
        if max_seconds.is_some() {
            command.args(["-t", &(duration_ms as f64 / 1_000.0).to_string(), "-vf", "scale='min(1280,iw)':-2", "-c:v", "h264_videotoolbox", "-b:v", "4M"]);
        } else {
            command.args(["-c:v", "copy"]);
        }
        command.args(["-c:a", "aac", "-b:a", "192k", "-ar", "48000", "-ac", "2"]);
        if let Some(subtitle_input) = subtitle_input {
            if has_original_audio { command.args(["-map", "0:a:0?", "-metadata:s:a:1", "language=und"]); }
            command.args(["-map", &format!("{subtitle_input}:s:0?"), "-metadata:s:a:0", "language=vie", "-metadata:s:s:0", "language=vie", "-c:s", "srt"]);
        } else if !is_mkv {
            command.args(["-movflags", "+faststart"]);
        }
        command.args(["-progress", "pipe:1", "-nostats"]).arg(&partial);
        job.stage = "mixing".into();
        job.processed = 55;
        job.message = "Đang mix và xuất video...".into();
        checkpoint_job(app, &mut project, &mut job).map_err(|error| job_error("checkpoint", error))?;
        let app_for_progress = app.clone();
        let snapshot = job.clone();
        let mut last_out_ms = 0i64;
        let mut latest_processed = 55usize;
        let command_result = run_managed_command(
            &mut command,
            "FFmpeg xuất video",
            Duration::from_secs(120),
            Duration::from_secs(60 * 60),
            move |event| {
                match event {
                    ManagedEvent::Line(line) => {
                        let value = line.strip_prefix("out_time_us=").or_else(|| line.strip_prefix("out_time_ms="))
                            .and_then(|raw| raw.parse::<i64>().ok()).map(|micros| micros / 1_000);
                        if let Some(out_ms) = value {
                            last_out_ms = last_out_ms.max(out_ms);
                            let ratio = (last_out_ms as f64 / duration_ms as f64).clamp(0.0, 1.0);
                            let mut update = snapshot.clone();
                            latest_processed = latest_processed.max(55 + (ratio * 44.0).round() as usize);
                            update.processed = latest_processed;
                            update.percent = update.processed as f64;
                            update.message = format!("Đang xuất video · {:.1}%", ratio * 100.0);
                            refresh_job_timing(&mut update);
                            emit_long_job(&app_for_progress, &update);
                        }
                    }
                    ManagedEvent::Heartbeat => {
                        let mut update = snapshot.clone();
                        update.processed = latest_processed;
                        update.percent = latest_processed as f64;
                        update.elapsed_seconds = now_ms().saturating_sub(update.started_at_ms) / 1_000;
                        update.message = "FFmpeg đang xử lý, chưa có mốc thời gian mới...".into();
                        emit_long_job(&app_for_progress, &update);
                    }
                }
            },
        ).await;
        if let Err(error) = command_result {
            let _ = std::fs::remove_file(&partial);
            let _ = std::fs::remove_file(&narration);
            return Err(process_job_error("mixing", error, None, None, None));
        }
        if !partial.is_file() || std::fs::metadata(&partial).map(|value| value.len()).unwrap_or(0) == 0 {
            let _ = std::fs::remove_file(&partial);
            return Err(job_error("mixing", "FFmpeg kết thúc nhưng file video tạm bị thiếu hoặc rỗng."));
        }
        if let Err(error) = std::fs::rename(&partial, &destination) {
            let _ = std::fs::remove_file(&partial);
            return Err(job_error(
                "mixing",
                format!("Không thể hoàn tất file xuất: {error}"),
            ));
        }
        let _ = std::fs::remove_file(narration);
        Ok((destination, rendered_cues, used_separation, previous_preview))
    }.await;

    match result {
        Ok((destination, rendered_cues, used_separation, previous_preview)) => {
            if max_seconds.is_some() {
                project.assets.preview_output = Some(destination.to_string_lossy().into_owned());
                if let Some(previous) = previous_preview {
                    let previous = PathBuf::from(previous);
                    if previous != destination
                        && previous.parent() == project_dir(app, project_id).ok().as_deref()
                    {
                        let _ = std::fs::remove_file(previous);
                    }
                }
            } else {
                project.assets.final_output = Some(destination.to_string_lossy().into_owned());
                project.status = "rendered".into();
            }
            job.status = "completed".into();
            job.stage = "complete".into();
            job.processed = job.total;
            job.current_cue_index = None;
            job.message = format!("Xuất video hoàn tất · {rendered_cues} cue");
            checkpoint_job(app, &mut project, &mut job)?;
            record_job_event(app, "info", "project.job.complete", &job);
            Ok(RenderResult {
                project,
                path: destination.to_string_lossy().into_owned(),
                rendered_cues,
                used_separation,
            })
        }
        Err(error) => Err(fail_job(app, &mut project, &mut job, error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_changes_with_text_voice_and_parameters() {
        let mut cue = DialogueCue {
            id: "cue-1".into(),
            index: 1,
            start_ms: 0,
            end_ms: 1_000,
            source_text: "hello".into(),
            translated_text: "xin chào".into(),
            machine_translated_text: "xin chào".into(),
            scene_id: String::new(),
            utterance_group_id: String::new(),
            speaker_id: "speaker-1".into(),
            confidence: 1.0,
            audio_path: None,
            audio_duration_ms: None,
            speed: 1.0,
            volume: 1.0,
            status: "translated".into(),
            warnings: vec![],
        };
        let settings = AudioSettings {
            separate_background: false,
            background_volume: 1.0,
            dialogue_volume: 1.0,
            min_tempo: 0.92,
            max_tempo: 1.12,
        };
        let first_raw = raw_audio_cache_key(&cue.translated_text, "thuy_dung");
        assert_eq!(
            first_raw,
            raw_audio_cache_key("<i>xin chào</i>", "thuy_dung")
        );
        let first_processed = processed_audio_cache_key(&first_raw, &cue, &settings);
        cue.translated_text = "chào bạn".into();
        assert_ne!(
            first_raw,
            raw_audio_cache_key(&cue.translated_text, "thuy_dung")
        );
        assert_ne!(first_raw, raw_audio_cache_key("xin chào", "thai_son"));
        cue.translated_text = "xin chào".into();
        cue.speed = 1.08;
        assert_ne!(
            first_processed,
            processed_audio_cache_key(&first_raw, &cue, &settings)
        );
        cue.speed = 1.0;
        cue.volume = 0.8;
        assert_ne!(
            first_processed,
            processed_audio_cache_key(&first_raw, &cue, &settings)
        );
        cue.volume = 1.0;
        cue.end_ms = 1_200;
        assert_ne!(
            first_processed,
            processed_audio_cache_key(&first_raw, &cue, &settings)
        );
    }

    #[test]
    fn partial_output_keeps_media_extension() {
        assert_eq!(
            partial_output_path(Path::new("/tmp/movie.mp4")),
            PathBuf::from("/tmp/movie.part.mp4")
        );
        assert_eq!(
            partial_output_path(Path::new("/tmp/movie.mkv")),
            PathBuf::from("/tmp/movie.part.mkv")
        );
    }

    #[tokio::test]
    async fn managed_process_fails_fast_on_exit_timeout_and_cancel() {
        begin_job();
        let exit_error = run_managed_command(
            Command::new("sh").args(["-c", "exit 7"]),
            "process giả",
            Duration::from_secs(2),
            Duration::from_secs(2),
            |_| {},
        )
        .await
        .unwrap_err();
        assert_eq!(exit_error.exit_code, Some(7));

        let timeout_started = Instant::now();
        let timeout_error = run_managed_command(
            Command::new("sh").args(["-c", "sleep 2"]),
            "process im lặng",
            Duration::from_millis(100),
            Duration::from_secs(3),
            |_| {},
        )
        .await
        .unwrap_err();
        assert!(timeout_error.timed_out);
        assert!(timeout_started.elapsed() < Duration::from_secs(1));

        begin_job();
        tokio::spawn(async {
            sleep(Duration::from_millis(100)).await;
            CANCEL_REQUESTED.store(true, Ordering::Relaxed);
        });
        let cancel_started = Instant::now();
        let cancel_error = run_managed_command(
            Command::new("sh").args(["-c", "sleep 10"]),
            "process bị hủy",
            Duration::from_secs(5),
            Duration::from_secs(20),
            |_| {},
        )
        .await
        .unwrap_err();
        assert_eq!(cancel_error.code, "cancelled");
        assert!(cancel_started.elapsed() < Duration::from_secs(3));
        begin_job();
    }

    #[test]
    fn imported_vietnamese_becomes_translated_text() {
        let cues = vec![Cue {
            index: 1,
            start_ms: 0,
            end_ms: 900,
            text: "Xin chào".into(),
        }];
        let dialogue = dialogue_from_cues(&cues, "vi");
        assert_eq!(dialogue[0].translated_text, "Xin chào");
        assert!(dialogue[0].source_text.is_empty());
        assert!(dialogue[0].machine_translated_text.is_empty());
    }

    #[test]
    fn character_bible_uses_notes_and_glossary() {
        let mut project = DubbingProject {
            id: "x".into(),
            title: "Phim".into(),
            video_path: "a.mp4".into(),
            preview_path: None,
            source_language: "ja".into(),
            target_language: "vi".into(),
            profile: "anime".into(),
            status: "draft".into(),
            created_at_ms: 0,
            updated_at_ms: 0,
            glossary: vec![GlossaryEntry {
                source: "Senpai".into(),
                target: "tiền bối".into(),
                note: String::new(),
            }],
            speakers: default_speakers(),
            cues: vec![],
            audio_settings: AudioSettings {
                separate_background: false,
                background_volume: 1.0,
                dialogue_volume: 1.0,
                min_tempo: 0.92,
                max_tempo: 1.12,
            },
            assets: ProjectAssets::default(),
            character_bible: CharacterBible::default(),
            translation_guide: TranslationGuide::default(),
            warnings: vec![],
            tts_cache_version: CURRENT_TTS_CACHE_VERSION,
            last_job: None,
        };
        project.speakers[0].notes = "chị của nhân vật 2".into();
        let bible = build_bible(&project);
        assert!(bible.relationships[0].contains("chị"));
        assert!(bible.address_rules[0].contains("tiền bối"));
    }

    #[test]
    fn imported_cues_use_one_default_speaker() {
        let source = vec![
            Cue {
                index: 1,
                start_ms: 0,
                end_ms: 1_000,
                text: "A".into(),
            },
            Cue {
                index: 2,
                start_ms: 1_000,
                end_ms: 2_000,
                text: "B".into(),
            },
        ];
        let cues = dialogue_from_cues(&source, "vi");
        assert!(cues.iter().all(|cue| cue.speaker_id == "speaker-1"));
    }

    #[test]
    fn memory_tokens_ignore_punctuation_and_short_words() {
        let tokens = memory_tokens("Xin chào, tôi là Tân!");
        assert!(tokens.contains("xin"));
        assert!(tokens.contains("chào"));
        assert!(tokens.contains("tân"));
        assert!(!tokens.contains("là"));
    }

    #[test]
    fn old_cue_schema_loads_with_translation_metadata_defaults() {
        let cue: DialogueCue = serde_json::from_value(serde_json::json!({
            "id": "cue-00001",
            "index": 1,
            "startMs": 0,
            "endMs": 900,
            "sourceText": "Hello",
            "translatedText": "Xin chào",
            "speakerId": "speaker-1",
            "confidence": 1.0,
            "audioPath": null,
            "audioDurationMs": null,
            "speed": 1.0,
            "volume": 1.0,
            "status": "translated",
            "warnings": []
        }))
        .expect("old cue schema should remain readable");

        assert!(cue.machine_translated_text.is_empty());
        assert!(cue.scene_id.is_empty());
        assert!(cue.utterance_group_id.is_empty());
    }
}
