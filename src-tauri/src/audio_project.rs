//! Immutable OCR inputs and a bounded, zero-anchored WAV trial; video projects are unchanged.
use crate::{diagnostics, models, ocr, pipeline, project};
use fs2::FileExt;
use long_tieng_capture_host::{ocr::cues::Cue, review};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::AppHandle;
use uuid::Uuid;
mod full;

const RATE: u32 = 48_000;
const MAX_END_MS: u64 = 7_200_000;
const PIPELINE: &str = "ocr-wav-trial-v1-48k-mono-fit80-no-trim";

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn validate_hash(value: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("ID/revision audio không hợp lệ.".into());
    }
    Ok(())
}
fn no_link(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => Err("Từ chối dữ liệu audio symlink.".into()),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    no_link(path)?;
    if fs::metadata(path).map_err(|e| e.to_string())?.len() > 32 * 1024 * 1024 {
        return Err("Manifest audio quá lớn.".into());
    }
    serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
fn atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    no_link(path)?;
    let parent = path.parent().ok_or("Thiếu thư mục audio")?;
    let temp = parent.join(format!(".{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        fs::rename(&temp, path).map_err(|e| e.to_string())?;
        File::open(parent)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}
fn file_hash(path: &Path) -> Result<String, String> {
    no_link(path)?;
    let mut input = BufReader::new(File::open(path).map_err(|e| e.to_string())?);
    let mut digest = Sha256::new();
    let mut bytes = [0; 65536];
    loop {
        let n = input.read(&mut bytes).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        digest.update(&bytes[..n]);
    }
    Ok(hex::encode(digest.finalize()))
}
fn directory(data: &Path, id: &str) -> Result<PathBuf, String> {
    validate_hash(id)?;
    let root = data.join("audio-projects");
    let dir = root.join(id);
    for path in [data, root.as_path(), dir.as_path()] {
        no_link(path)?;
    }
    Ok(dir)
}
fn lock(dir: &Path, name: &str) -> Result<File, String> {
    let path = dir.join(name);
    no_link(&path)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|e| e.to_string())?;
    FileExt::try_lock_exclusive(&file).map_err(|_| "Dự án audio đang được xử lý ở cửa sổ khác.")?;
    Ok(file)
}
fn project_id(session: &str, revision: &str) -> String {
    sha(format!("ocr-audio-project-v1:{session}:{revision}").as_bytes())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    session_id: String,
    revision: String,
    title: String,
    url: String,
    cues: Vec<Cue>,
    coverage: serde_json::Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrialSettings {
    pub start_ms: u64,
    pub duration_ms: u64,
    pub voice: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full: Option<full::Options>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CueMetric {
    pub index: usize,
    pub start_ms: u64,
    pub end_ms: u64,
    pub raw_duration_ms: i64,
    pub audio_duration_ms: i64,
    pub tempo: f64,
    pub reused: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrialJob {
    #[serde(flatten)]
    pub progress: project::LongJobState,
    pub settings: TrialSettings,
    pub metrics: Vec<CueMetric>,
    pub warnings: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub key: String,
    pub path: String,
    pub wav_hash: String,
    pub report_hash: String,
    pub duration_ms: u64,
    pub settings: TrialSettings,
    pub source_revision: String,
    pub rendered_cues: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial: Option<bool>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioProject {
    pub schema_version: u32,
    pub id: String,
    pub title: String,
    pub url: String,
    pub session_id: String,
    pub source_revision: String,
    pub source_hash: String,
    pub created_at_ms: u64,
    pub updated_at_ms: u64,
    pub last_job: Option<TrialJob>,
    pub last_preview: Option<Artifact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_full: Option<Artifact>,
}
fn save(dir: &Path, p: &mut AudioProject) -> Result<(), String> {
    p.updated_at_ms = now();
    atomic(
        &dir.join("project.json"),
        &serde_json::to_vec_pretty(p).map_err(|e| e.to_string())?,
    )
}
fn source(dir: &Path, p: &AudioProject) -> Result<Source, String> {
    if file_hash(&dir.join("source.json"))? != p.source_hash {
        return Err("Snapshot phụ đề audio bị hỏng; không tạo giọng.".into());
    }
    let s: Source = read_json(&dir.join("source.json"))?;
    if s.revision != p.source_revision
        || s.session_id != p.session_id
        || project_id(&s.session_id, &s.revision) != p.id
    {
        return Err("Snapshot sai revision/UUID.".into());
    }
    Ok(s)
}
fn verify_artifact(dir: &Path, a: &mut Artifact) -> Result<(), String> {
    validate_hash(&a.key)?;
    let root = dir.join("previews");
    let folder = root.join(&a.key);
    no_link(&root)?;
    no_link(&folder)?;
    let path = folder.join("preview.wav");
    if file_hash(&path)? != a.wav_hash || file_hash(&folder.join("report.json"))? != a.report_hash {
        return Err("WAV đã lưu bị thiếu/hỏng.".into());
    }
    let report: serde_json::Value = read_json(&folder.join("report.json"))?;
    if report["wavHash"] != a.wav_hash
        || report["sourceRevision"] != a.source_revision
        || report["durationMs"] != a.duration_ms
        || report["settings"] != serde_json::to_value(&a.settings).map_err(|e| e.to_string())?
    {
        return Err("WAV/report không khớp revision hoặc cấu hình đã lưu.".into());
    }
    if a.settings.full.is_some()
        && (report["srtHash"] != file_hash(&folder.join("subtitles.vi.srt"))?
            || report["anchorMs"] != 0
            || report["sampleRate"] != RATE
            || report["channels"] != 1)
    {
        return Err("Metadata/SRT WAV toàn timeline bị hỏng.".into());
    }
    if a.settings.full.is_some() {
        let input: Source = read_json(&dir.join("source.json"))?;
        let coverage = full::validate(&input, &a.settings);
        if !coverage.issues.is_empty()
            || report["schemaVersion"] != 2
            || report["kind"] != "ocr-narration"
            || report["sourceHash"] != file_hash(&dir.join("source.json"))?
            || report["sourceSessionId"] != input.session_id
            || report["sourceUrl"] != input.url
            || report["title"] != input.title
            || report["partial"] != coverage.partial
            || report["coverageComplete"] != !coverage.partial
            || report["allCuesReviewed"] != true
            || report["bitsPerSample"] != 16
            || report["offsetMs"] != 0
            || report["wholeEpisodeVerified"] != false
            || report["durationSource"]
                != if coverage.known_duration_ms > 0 {
                    "capturedVideo"
                } else {
                    "userProvided"
                }
            || report["missingRanges"]
                != serde_json::to_value(&coverage.gaps).map_err(|e| e.to_string())?
            || report["srtHash"] != sha(&full::subtitle_bytes(&input))
            || a.rendered_cues != input.cues.len()
        {
            return Err("Metadata WAV không đúng nguồn/coverage đã lưu.".into());
        }
    }
    let reader = hound::WavReader::open(&path).map_err(|e| e.to_string())?;
    if reader.spec() != wav_spec() || reader.duration() as u64 != a.duration_ms * RATE as u64 / 1000
    {
        return Err("WAV không đúng timeline đã lưu.".into());
    }
    a.path = path.to_string_lossy().into_owned();
    a.partial = report["partial"].as_bool();
    Ok(())
}
fn read_project(data: &Path, id: &str) -> Result<AudioProject, String> {
    let dir = directory(data, id)?;
    let mut p: AudioProject = read_json(&dir.join("project.json"))?;
    if p.schema_version != 1 || p.id != id {
        return Err("Schema audio project không hợp lệ.".into());
    }
    source(&dir, &p)?;
    if p.last_job
        .as_ref()
        .is_some_and(|j| j.progress.status == "running")
    {
        if let Ok(_guard) = lock(&dir, ".job.lock") {
            if let Some(job) = p.last_job.as_mut() {
                job.progress.status = "interrupted".into();
                job.progress.message = "Tác vụ bị gián đoạn; các cue đã lưu vẫn được giữ.".into();
            }
            save(&dir, &mut p)?;
        }
    }
    if let Some(job) = p
        .last_job
        .as_mut()
        .filter(|j| j.progress.status == "running")
    {
        job.progress.elapsed_seconds = now().saturating_sub(job.progress.started_at_ms) / 1000;
    }
    Ok(p)
}
pub fn load(data: &Path, id: &str) -> Result<AudioProject, String> {
    let dir = directory(data, id)?;
    let mut p = read_project(data, id)?;
    for artifact in [&p.last_preview, &p.last_full].into_iter().flatten() {
        if artifact.source_revision != p.source_revision {
            return Err("WAV thuộc revision khác với dự án audio.".into());
        }
    }
    if let Some(a) = p.last_preview.as_mut() {
        verify_artifact(&dir, a)?;
    }
    if let Some(a) = p.last_full.as_mut() {
        verify_artifact(&dir, a)?;
    }
    Ok(p)
}
pub fn create(data: &Path, session: &str, expected: &str) -> Result<AudioProject, String> {
    validate_hash(expected)?;
    let doc = review::load(data, session)?;
    if doc.revision != expected {
        return Err("REVISION_CONFLICT: Hãy lưu/tải lại bản OCR trước khi tạo dự án audio.".into());
    }
    if doc.cues.is_empty() {
        return Err("Bản OCR không có câu để tạo giọng.".into());
    }
    let id = project_id(session, expected);
    let dir = directory(data, &id)?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let _guard = lock(&dir, ".create.lock")?;
    if dir.join("project.json").exists() {
        return load(data, &id);
    }
    let summary = review::list(data)?
        .into_iter()
        .find(|s| s.session_id == session)
        .ok_or("Không tìm thấy bản thu")?;
    let s = Source {
        session_id: session.into(),
        revision: expected.into(),
        title: summary.title.clone(),
        url: summary.url.clone(),
        cues: doc.cues,
        coverage: doc.coverage,
    };
    let bytes = serde_json::to_vec_pretty(&s).map_err(|e| e.to_string())?;
    atomic(&dir.join("source.json"), &bytes)?;
    let mut p = AudioProject {
        schema_version: 1,
        id,
        title: summary.title,
        url: summary.url,
        session_id: session.into(),
        source_revision: expected.into(),
        source_hash: sha(&bytes),
        created_at_ms: now(),
        updated_at_ms: now(),
        last_job: None,
        last_preview: None,
        last_full: None,
    };
    save(&dir, &mut p)?;
    File::open(dir.parent().unwrap())
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())?;
    Ok(p)
}
fn voice_valid(voice: &str) -> bool {
    matches!(
        voice,
        "thuy_dung"
            | "thuc_doan"
            | "my_duyen"
            | "kim_thanh"
            | "thai_son"
            | "minh_triet"
            | "duc_tri"
    )
}
fn select(source: &Source, settings: &TrialSettings) -> Result<Vec<Cue>, String> {
    if settings.full.is_some() {
        let preflight = full::validate(source, settings);
        if !preflight.issues.is_empty() {
            return Err(preflight
                .issues
                .iter()
                .take(20)
                .map(|i| i.message.as_str())
                .collect::<Vec<_>>()
                .join("\n"));
        }
        let mut cues = source.cues.clone();
        cues.sort_by_key(|c| (c.start_ms, c.end_ms));
        return Ok(cues);
    }
    if !voice_valid(&settings.voice) || !matches!(settings.duration_ms, 30_000 | 60_000) {
        return Err("Chọn giọng hợp lệ và đoạn thử 30 hoặc 60 giây.".into());
    }
    let end = settings
        .start_ms
        .checked_add(settings.duration_ms)
        .filter(|v| *v <= MAX_END_MS)
        .ok_or("Phạm vi thử vượt giới hạn 2 giờ.")?;
    let duration = source.coverage["durationMs"].as_u64().unwrap_or(0);
    if duration > 0 && end > duration {
        return Err("Đoạn thử vượt thời lượng video đã ghi.".into());
    }
    let covered = source.coverage["ranges"].as_array().is_some_and(|ranges| {
        ranges.iter().any(|r| {
            r["startMs"]
                .as_u64()
                .is_some_and(|s| s <= settings.start_ms.saturating_add(250))
                && r["endMs"]
                    .as_u64()
                    .is_some_and(|e| e.saturating_add(250) >= end)
        })
    });
    if !covered {
        return Err("Đoạn thử nằm ngoài một vùng thu liên tục; hãy chọn lại phạm vi.".into());
    }
    let mut cues: Vec<_> = source
        .cues
        .iter()
        .filter(|c| c.start_ms < end && c.end_ms > settings.start_ms)
        .cloned()
        .collect();
    if cues.is_empty() {
        return Err("Đoạn thử không có câu tiếng Việt.".into());
    }
    if cues.len() > 64 {
        return Err("Đoạn thử có hơn 64 cue; hãy gộp/sửa cue vụn trong OCR trước.".into());
    }
    cues.sort_by_key(|c| (c.start_ms, c.end_ms));
    for (i, c) in cues.iter().enumerate() {
        if c.start_ms < settings.start_ms || c.end_ms > end {
            return Err(format!(
                "Cue {} vắt qua biên đoạn thử; không cắt lời. Hãy đổi phạm vi.",
                c.index
            ));
        }
        if !c.reviewed {
            return Err(format!(
                "Cue {} chưa được kiểm tra và lưu trong OCR.",
                c.index
            ));
        }
        if c.text.trim().is_empty() || c.end_ms <= c.start_ms {
            return Err(format!("Cue {} không hợp lệ.", c.index));
        }
        if i > 0 && c.start_ms < cues[i - 1].end_ms {
            return Err(format!(
                "Cue {} và {} chồng lấn; hãy sửa trong OCR trước.",
                cues[i - 1].index,
                c.index
            ));
        }
    }
    Ok(cues)
}
fn wav_spec() -> hound::WavSpec {
    hound::WavSpec {
        channels: 1,
        sample_rate: RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    }
}
fn check_cancel(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("Đã hủy WAV thử; không publish output dở dang.".into())
    } else {
        Ok(())
    }
}
pub(crate) fn write_timeline(
    cues: &[(Cue, PathBuf)],
    end_ms: u64,
    path: &Path,
    cancel: &AtomicBool,
) -> Result<(), String> {
    if end_ms > MAX_END_MS || cues.is_empty() {
        return Err("Timeline thử không hợp lệ.".into());
    }
    for (i, (cue, _)) in cues.iter().enumerate() {
        if cue.end_ms <= cue.start_ms
            || cue.end_ms > end_ms
            || (i > 0 && cue.start_ms < cues[i - 1].0.end_ms)
        {
            return Err("Timestamp cue không hợp lệ hoặc chồng lấn.".into());
        }
    }
    // Validate every full cue before opening the output; never take/truncate speech samples.
    for (c, p) in cues {
        let reader = hound::WavReader::open(p).map_err(|e| e.to_string())?;
        if reader.spec() != wav_spec() || reader.duration() == 0 {
            return Err(format!(
                "Audio cue {} không phải PCM 48 kHz mono 16-bit.",
                c.index
            ));
        }
        if reader.duration() as u64 > (c.end_ms - c.start_ms) * RATE as u64 / 1000 {
            return Err(format!(
                "Audio cue {} vượt khoảng thoại; không cắt mất lời.",
                c.index
            ));
        }
    }
    let mut output = hound::WavWriter::new(
        BufWriter::new(File::create(path).map_err(|e| e.to_string())?),
        wav_spec(),
    )
    .map_err(|e| e.to_string())?;
    let mut written = 0u64;
    let total = end_ms * RATE as u64 / 1000;
    for (c, p) in cues {
        let start = c.start_ms * RATE as u64 / 1000;
        if start < written {
            return Err("Audio chồng lấn; không dồn/ghi đè lời thoại.".into());
        }
        while written < start {
            if written.is_multiple_of(4096) {
                check_cancel(cancel)?;
            }
            output.write_sample(0i16).map_err(|e| e.to_string())?;
            written += 1;
        }
        let mut reader = hound::WavReader::open(p).map_err(|e| e.to_string())?;
        for sample in reader.samples::<i16>() {
            if written.is_multiple_of(4096) {
                check_cancel(cancel)?;
            }
            output
                .write_sample(sample.map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            written += 1;
        }
    }
    if written > total {
        return Err("Audio vượt biên thử; không cắt lời.".into());
    }
    while written < total {
        if written.is_multiple_of(4096) {
            check_cancel(cancel)?;
        }
        output.write_sample(0i16).map_err(|e| e.to_string())?;
        written += 1;
    }
    check_cancel(cancel)?;
    output.finalize().map_err(|e| e.to_string())?;
    File::open(path)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())
}
struct CancelMonitor {
    flag: Arc<AtomicBool>,
    task: tauri::async_runtime::JoinHandle<()>,
}
impl Drop for CancelMonitor {
    fn drop(&mut self) {
        self.task.abort();
    }
}
fn monitor(dir: PathBuf, job: String) -> CancelMonitor {
    let flag = Arc::new(AtomicBool::new(false));
    let value = flag.clone();
    let task = tauri::async_runtime::spawn(async move {
        loop {
            if let Ok(v) = read_json::<serde_json::Value>(&dir.join("cancel.json")) {
                if v["jobId"] == job {
                    value.store(true, Ordering::Relaxed);
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    });
    CancelMonitor { flag, task }
}
pub fn cancel(data: &Path, id: &str, job: &str) -> Result<(), String> {
    long_tieng_capture_host::validation::session_id(job)?;
    let dir = directory(data, id)?;
    let p: AudioProject = read_json(&dir.join("project.json"))?;
    if p.last_job
        .as_ref()
        .is_some_and(|j| j.progress.id != job && j.progress.status == "running")
    {
        return Err("Tác vụ này không còn chạy; không hủy job khác.".into());
    }
    atomic(
        &dir.join("cancel.json"),
        &serde_json::to_vec(&serde_json::json!({"jobId":job})).map_err(|e| e.to_string())?,
    )
}
fn checkpoint(dir: &Path, p: &mut AudioProject, job: &mut TrialJob) -> Result<(), String> {
    job.progress.updated_at_ms = now();
    job.progress.elapsed_seconds = now().saturating_sub(job.progress.started_at_ms) / 1000;
    job.progress.eta_seconds = if job.progress.status == "running" && job.progress.processed > 0 {
        Some(
            job.progress.elapsed_seconds
                * job.progress.total.saturating_sub(job.progress.processed) as u64
                / job.progress.processed as u64,
        )
    } else {
        None
    };
    job.progress.percent = if job.progress.status == "completed" {
        100.
    } else if job.progress.stage == "timeline" {
        95.
    } else {
        job.progress.processed as f64 / job.progress.total.max(1) as f64 * 90.
    };
    p.last_job = Some(job.clone());
    save(dir, p)
}
fn diagnostic(app: &AppHandle, job: &TrialJob) {
    let _ = diagnostics::append(
        app,
        diagnostics::entry(
            if job.progress.status == "completed" {
                "info"
            } else {
                "error"
            },
            "ocr.audio-preview",
            job.progress.message.clone(),
            serde_json::to_string(job).ok(),
        ),
    );
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Cache {
    raw_hash: String,
    processed_hash: String,
    metric: CueMetric,
}
fn error_text(error: project::JobError) -> String {
    format!(
        "{}{}",
        error.message,
        error.details.map(|s| format!("\n{s}")).unwrap_or_default()
    )
}
fn fingerprint(paths: &[PathBuf], backend: &str) -> Result<String, String> {
    let mut bytes = PIPELINE.to_string();
    bytes.push_str(backend);
    for p in paths {
        bytes.push_str(&file_hash(p)?);
    }
    Ok(sha(bytes.as_bytes()))
}
pub async fn render(
    app: &AppHandle,
    id: &str,
    job_id: &str,
    settings: TrialSettings,
) -> Result<AudioProject, String> {
    long_tieng_capture_host::validation::session_id(job_id)?;
    let _media = project::media_job_guard()?;
    let data = ocr::data(app)?;
    let dir = directory(&data, id)?;
    let _quota = lock(&data.join("audio-projects"), ".quota.lock")?;
    let _guard = lock(&dir, ".job.lock")?;
    let mut p = load(&data, id)?;
    let input = source(&dir, &p)?;
    let cues = select(&input, &settings)?;
    let required = (settings.start_ms + settings.duration_ms) * RATE as u64 / 1000 * 2
        + cues.len() as u64 * 5_000_000
        + 1_073_741_824;
    if settings.full.is_none() && fs2::available_space(&dir).map_err(|e| e.to_string())? < required
    {
        return Err("Không đủ dung lượng cho WAV thử và cache (giữ ít nhất 1 GiB trống).".into());
    }
    if settings.full.is_some() {
        let preflight = full::inspect(&data, id, &settings)?;
        if !preflight.issues.is_empty() {
            return Err(preflight
                .issues
                .iter()
                .take(20)
                .map(|i| i.message.as_str())
                .collect::<Vec<_>>()
                .join("\n"));
        }
    } else {
        full::check_budget(&data, &dir, required.saturating_sub(1_073_741_824))?;
    }
    let mut job = TrialJob {
        progress: project::LongJobState {
            id: job_id.into(),
            kind: if settings.full.is_some() {
                "ocr-audio-full"
            } else {
                "ocr-audio-preview"
            }
            .into(),
            status: "running".into(),
            stage: "preflight".into(),
            total: cues.len(),
            message: "Đang kiểm tra engine và voice...".into(),
            started_at_ms: now(),
            ..Default::default()
        },
        settings: settings.clone(),
        metrics: vec![],
        warnings: vec![],
    };
    checkpoint(&dir, &mut p, &mut job)?;
    let cancellation = monitor(dir.clone(), job_id.into());
    let resources = full::watch_budget(data.clone(), dir.clone(), cancellation.flag.clone());
    let result = render_inner(
        app,
        &dir,
        &input,
        &mut p,
        &mut job,
        &cues,
        &cancellation.flag,
    )
    .await
    .and_then(|artifact| {
        check_cancel(&cancellation.flag)?;
        Ok(artifact)
    });
    let resource_error = resources.error();
    let result = if let Some(ref e) = resource_error {
        Err(e.clone())
    } else {
        result
    };
    match result {
        Ok(a) => {
            if settings.full.is_some() {
                p.last_full = Some(a);
            } else {
                p.last_preview = Some(a);
            }
            job.progress.status = "completed".into();
            job.progress.stage = "complete".into();
            job.progress.eta_seconds = None;
            job.progress.processed = job.progress.total;
            job.progress.message = if settings.full.is_some() {
                "Đã lưu WAV toàn timeline"
            } else {
                "Đã lưu WAV thử"
            }
            .into();
            checkpoint(&dir, &mut p, &mut job)?;
            diagnostic(app, &job);
            Ok(p)
        }
        Err(e) => {
            job.progress.status =
                if cancellation.flag.load(Ordering::Relaxed) && resource_error.is_none() {
                    "cancelled"
                } else {
                    "failed"
                }
                .into();
            job.progress.message = e.clone();
            job.progress.error = Some(project::JobError {
                code: job.progress.status.clone(),
                stage: job.progress.stage.clone(),
                message: e.clone(),
                ..Default::default()
            });
            let _ = checkpoint(&dir, &mut p, &mut job);
            diagnostic(app, &job);
            Err(e)
        }
    }
}
async fn render_inner(
    app: &AppHandle,
    dir: &Path,
    input: &Source,
    p: &mut AudioProject,
    job: &mut TrialJob,
    cues: &[Cue],
    cancel: &Arc<AtomicBool>,
) -> Result<Artifact, String> {
    check_cancel(cancel)?;
    let executable = pipeline::find_executable(app, &["audiocpp_cli", "audiocpp-cli"])
        .ok_or("Không tìm thấy audio.cpp.")?;
    let ffmpeg = pipeline::find_executable(app, &["ffmpeg"]).ok_or("Không tìm thấy FFmpeg.")?;
    let g2p = pipeline::find_executable(app, &["libsea_g2p_rs.dylib"]).ok_or("Thiếu SEA-G2P.")?;
    let dictionary =
        pipeline::find_executable(app, &["sea_g2p.bin"]).ok_or("Thiếu từ điển SEA-G2P.")?;
    let model = models::model_path(app, "vieneu-v3-turbo-q8", "vieneu-v3-turbo-q8_0.gguf")?;
    if !model.is_file() {
        return Err("Chưa tải VieNeu-TTS v3 Turbo trong Model Manager.".into());
    }
    let backend = project::audio_backend(&executable, cancel).await?;
    let mut identities = std::collections::BTreeMap::new();
    for voice in cues
        .iter()
        .map(|c| full::voice_for(&job.settings, c))
        .collect::<std::collections::BTreeSet<_>>()
    {
        check_cancel(cancel)?;
        let reference = models::model_path(
            app,
            "vieneu-v3-turbo-q8",
            &format!("voices/{voice}/ref_codes.txt"),
        )?;
        let embedding = models::model_path(
            app,
            "vieneu-v3-turbo-q8",
            &format!("voices/{voice}/speaker.emb.txt"),
        )?;
        identities.insert(
            voice.to_owned(),
            fingerprint(
                &[
                    executable.clone(),
                    ffmpeg.clone(),
                    g2p.clone(),
                    dictionary.clone(),
                    model.clone(),
                    reference,
                    embedding,
                ],
                &backend,
            )?,
        );
    }
    let identity = if job.settings.full.is_none() {
        identities[&job.settings.voice].clone()
    } else {
        sha(&serde_json::to_vec(&identities).map_err(|e| e.to_string())?)
    };
    check_cancel(cancel)?;
    let cache = dir.join("cache");
    no_link(&cache)?;
    fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let mut timeline = vec![];
    // Bound inference batches while committing each normalized cue independently.
    let batch_size = if job.settings.full.is_some() { 8 } else { 1 };
    let data = ocr::data(app)?;
    let mut overflow = 0;
    for (batch_index, batch) in cues.chunks(batch_size).enumerate() {
        check_cancel(cancel)?;
        full::check_budget(&data, dir, 1_048_576)?;
        let mut prepared = vec![];
        for cue in batch {
            check_cancel(cancel)?;
            job.progress.current_cue_index = Some(cue.index);
            job.progress.stage = "tts".into();
            job.progress.message = format!(
                "Chuẩn bị câu {} · {}/{}",
                cue.index,
                job.progress.processed + 1,
                cues.len()
            );
            checkpoint(dir, p, job)?;
            let text = pipeline::speech_text(&cue.text);
            if text.is_empty() {
                return Err(format!(
                    "Cue {} không còn lời đọc sau khi bỏ tag.",
                    cue.index
                ));
            }
            let voice = full::voice_for(&job.settings, cue).to_owned();
            let cue_identity = &identities[&voice];
            let raw_key = project::raw_audio_cache_key(&text, &voice);
            let key = sha(format!(
                "{PIPELINE}:{}:{}:{cue_identity}:{raw_key}:{}:{}",
                input.revision, p.source_hash, cue.start_ms, cue.end_ms
            )
            .as_bytes());
            let folder = cache.join(&key);
            no_link(&folder)?;
            fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
            let raw = folder.join("raw.wav");
            let processed = folder.join("processed.wav");
            let meta = folder.join("cache.json");
            for path in [&raw, &processed, &meta] {
                no_link(path)?;
            }
            let cached_metric = if meta.exists() {
                let mut cached: Cache = read_json(&meta)?;
                if file_hash(&raw)? != cached.raw_hash
                    || file_hash(&processed)? != cached.processed_hash
                {
                    return Err(format!(
                        "Cache audio cue {} hỏng; không dùng nhầm audio.",
                        cue.index
                    ));
                }
                cached.metric.reused = true;
                Some(cached.metric)
            } else {
                // Uncommitted cache leftovers are preserved, but never trusted as completed audio.
                for file in [&raw, &processed] {
                    if file.exists() {
                        fs::rename(
                            file,
                            folder.join(format!(
                                "orphan-{}-{}",
                                Uuid::new_v4(),
                                file.file_name().unwrap().to_string_lossy()
                            )),
                        )
                        .map_err(|e| e.to_string())?;
                    }
                }
                None
            };
            let task = project::TtsTask {
                cue_id: format!("ocr-{}", cue.index),
                cue_index: cue.index,
                text,
                voice: voice.clone(),
                raw_key,
                raw_path: raw.clone(),
                processed_path: processed.clone(),
                target_duration_ms: (cue.end_ms - cue.start_ms).saturating_sub(80).max(100) as i64,
                speed: 1.,
                volume: 1.,
                had_markup: false,
            };
            prepared.push((cue.clone(), task, meta, cached_metric));
        }
        let voices = prepared
            .iter()
            .filter(|p| p.3.is_none())
            .map(|p| p.1.voice.clone())
            .collect::<std::collections::BTreeSet<_>>();
        for voice in voices {
            check_cancel(cancel)?;
            let tasks = prepared
                .iter()
                .filter(|p| p.3.is_none() && p.1.voice == voice)
                .map(|p| p.1.clone())
                .collect::<Vec<_>>();
            full::check_budget(&data, dir, tasks.len() as u64 * 32 * 1024 * 1024)?;
            let workspace = dir.join(format!(
                "job-{}/batch-{batch_index}-{voice}",
                job.progress.id
            ));
            job.progress.stage = "tts-batch".into();
            job.progress.message = format!(
                "Batch {} · {} câu · {}",
                batch_index + 1,
                tasks.len(),
                voice
            );
            checkpoint(dir, p, job)?;
            project::generate_tts_batch_scoped(
                project::TtsBatchContext {
                    app,
                    executable: &executable,
                    model: &model,
                    g2p_library: &g2p,
                    g2p_dictionary: &dictionary,
                    backend: &backend,
                    voice: &voice,
                    batch_number: batch_index + 1,
                    cancellation: cancel,
                },
                &mut job.progress,
                &tasks,
                &workspace,
            )
            .await
            .map_err(error_text)?;
        }
        for (cue, task, meta, cached_metric) in prepared {
            check_cancel(cancel)?;
            let raw = task.raw_path.clone();
            let processed = task.processed_path.clone();
            if fs::metadata(&raw).map_err(|e| e.to_string())?.len() > 16 * 1024 * 1024 {
                return Err(format!(
                    "Audio thô câu {} vượt 16 MiB; cần sửa/chia câu trước.",
                    cue.index
                ));
            }
            job.progress.current_cue_index = Some(cue.index);
            let metric = if let Some(metric) = cached_metric {
                job.progress.reused += 1;
                metric
            } else {
                let raw_duration = project::wav_duration_ms(&raw)?;
                let target = task.target_duration_ms as f64;
                job.progress.stage = "normalizing".into();
                checkpoint(dir, p, job)?;
                let (_, duration, _) = project::normalize_task_scoped(
                    ffmpeg.clone(),
                    task,
                    project::AudioSettings {
                        separate_background: false,
                        background_volume: 0.,
                        dialogue_volume: 1.,
                        min_tempo: 1.,
                        max_tempo: 1.25,
                    },
                    cancel,
                )
                .await
                .map_err(error_text)?;
                let metric = CueMetric {
                    index: cue.index,
                    start_ms: cue.start_ms,
                    end_ms: cue.end_ms,
                    raw_duration_ms: raw_duration,
                    audio_duration_ms: duration,
                    tempo: (raw_duration as f64 / target).clamp(1., 1.25),
                    reused: false,
                };
                File::open(&raw)
                    .and_then(|f| f.sync_all())
                    .map_err(|e| e.to_string())?;
                File::open(&processed)
                    .and_then(|f| f.sync_all())
                    .map_err(|e| e.to_string())?;
                let cache = Cache {
                    raw_hash: file_hash(&raw)?,
                    processed_hash: file_hash(&processed)?,
                    metric: metric.clone(),
                };
                atomic(
                    &meta,
                    &serde_json::to_vec_pretty(&cache).map_err(|e| e.to_string())?,
                )?;
                job.progress.generated += 1;
                metric
            };
            job.metrics.push(metric);
            job.progress.processed += 1;
            checkpoint(dir, p, job)?;
            let samples = hound::WavReader::open(&processed)
                .map_err(|e| e.to_string())?
                .duration() as u64;
            if samples > (cue.end_ms - cue.start_ms) * RATE as u64 / 1000 {
                let warning=format!("Audio cue {} dài hơn khoảng thoại ở tempo tối đa 1.25×; sửa lời hoặc timestamp trong OCR. Không xuất WAV bị cắt lời.",cue.index);
                job.warnings.push(warning.clone());
                checkpoint(dir, p, job)?;
                overflow += 1;
                if job.settings.full.is_none() {
                    return Err(warning);
                }
                continue;
            }
            timeline.push((cue, processed));
        }
    }
    if overflow > 0 {
        return Err(format!(
            "{overflow} câu đọc quá dài; đã giữ cache và danh sách lỗi, không xuất WAV bị cắt lời."
        ));
    }
    job.progress.current_cue_index = None;
    job.progress.stage = "timeline".into();
    job.progress.message = "Đang ghép WAV theo mốc video 0...".into();
    checkpoint(dir, p, job)?;
    let end = job.settings.start_ms + job.settings.duration_ms;
    full::check_budget(&data, dir, end * RATE as u64 * 2 / 1000 + 1_048_576)?;
    let key=sha(&serde_json::to_vec(&serde_json::json!({"pipeline":PIPELINE,"source":p.source_hash,"revision":input.revision,"runtime":identity,"settings":job.settings})).map_err(|e|e.to_string())?);
    let previews = dir.join("previews");
    no_link(&previews)?;
    fs::create_dir_all(&previews).map_err(|e| e.to_string())?;
    let folder = previews.join(&key);
    no_link(&folder)?;
    fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let output = folder.join("preview.wav");
    let report = folder.join("report.json");
    no_link(&output)?;
    no_link(&report)?;
    if output.exists() {
        let stored: serde_json::Value = read_json(&report)?;
        let mut artifact = Artifact {
            key: key.clone(),
            path: String::new(),
            wav_hash: stored["wavHash"]
                .as_str()
                .ok_or("WAV report thiếu checksum")?
                .into(),
            report_hash: file_hash(&report)?,
            duration_ms: end,
            settings: job.settings.clone(),
            source_revision: input.revision.clone(),
            rendered_cues: cues.len(),
            partial: None,
        };
        verify_artifact(dir, &mut artifact)?;
        return Ok(artifact);
    }
    let temporary = folder.join(format!("{}.part.wav", job.progress.id));
    no_link(&temporary)?;
    let temp_for_worker = temporary.clone();
    let flag = cancel.clone();
    tauri::async_runtime::spawn_blocking(move || {
        write_timeline(&timeline, end, &temp_for_worker, &flag)
    })
    .await
    .map_err(|e| e.to_string())??;
    check_cancel(cancel)?;
    let mut report_value = serde_json::json!({"schemaVersion":1,"sourceSessionId":input.session_id,"sourceRevision":input.revision,"sourceHash":p.source_hash,"pipeline":PIPELINE,"runtime":identity,"settings":job.settings,"durationMs":end,"anchorMs":0,"partial":true,"wholeEpisodeVerified":false,"metrics":job.metrics,"wavHash":file_hash(&temporary)?});
    if job.settings.full.is_some() {
        let status = full::validate(input, &job.settings);
        let srt = full::subtitle_bytes(input);
        atomic(&folder.join("subtitles.vi.srt"), &srt)?;
        report_value["schemaVersion"] = serde_json::json!(2);
        report_value["kind"] = serde_json::json!("ocr-narration");
        report_value["title"] = serde_json::json!(input.title);
        report_value["sourceUrl"] = serde_json::json!(input.url);
        report_value["sampleRate"] = serde_json::json!(RATE);
        report_value["channels"] = serde_json::json!(1);
        report_value["bitsPerSample"] = serde_json::json!(16);
        report_value["offsetMs"] = serde_json::json!(0);
        report_value["srtHash"] = serde_json::json!(sha(&srt));
        report_value["partial"] = serde_json::json!(status.partial);
        report_value["coverageComplete"] = serde_json::json!(!status.partial);
        report_value["durationSource"] = serde_json::json!(if status.known_duration_ms > 0 {
            "capturedVideo"
        } else {
            "userProvided"
        });
        report_value["missingRanges"] =
            serde_json::to_value(&status.gaps).map_err(|e| e.to_string())?;
        report_value["allCuesReviewed"] = serde_json::json!(true);
        report_value["warnings"] =
            serde_json::to_value(&status.warnings).map_err(|e| e.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(&report_value).map_err(|e| e.to_string())?;
    atomic(&report, &bytes)?;
    fs::rename(&temporary, &output).map_err(|e| e.to_string())?;
    File::open(&folder)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())?;
    File::open(&previews)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())?;
    Ok(Artifact {
        key,
        path: output.to_string_lossy().into_owned(),
        wav_hash: file_hash(&output)?,
        report_hash: sha(&bytes),
        duration_ms: end,
        settings: job.settings.clone(),
        source_revision: input.revision.clone(),
        rendered_cues: cues.len(),
        partial: report_value["partial"].as_bool(),
    })
}
pub fn export(data: &Path, id: &str, key: &str, path: &Path) -> Result<(), String> {
    let p = load(data, id)?;
    let artifact = p.last_preview.ok_or("Chưa có WAV thử đã lưu")?;
    if artifact.key != key {
        return Err("WAV đã thay đổi; hãy tải lại bản đã lưu.".into());
    }
    no_link(path)?;
    let parent = path
        .parent()
        .ok_or("Thiếu thư mục xuất")?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if parent.starts_with(data.canonicalize().map_err(|e| e.to_string())?) {
        return Err("Không xuất đè vào dữ liệu nội bộ app.".into());
    }
    let target = parent.join(path.file_name().ok_or("Thiếu tên file")?);
    let temp = parent.join(format!(".{}.wav.tmp", Uuid::new_v4()));
    fs::copy(artifact.path, &temp).map_err(|e| e.to_string())?;
    File::open(&temp)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())?;
    fs::rename(&temp, target).map_err(|e| e.to_string())?;
    File::open(parent)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn new_ocr_audio_job_id() -> String {
    Uuid::new_v4().to_string()
}
#[tauri::command]
pub async fn create_ocr_audio_project(
    app: AppHandle,
    session_id: String,
    expected_revision: String,
) -> Result<AudioProject, String> {
    let data = ocr::data(&app)?;
    tauri::async_runtime::spawn_blocking(move || create(&data, &session_id, &expected_revision))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn load_ocr_audio_project(
    app: AppHandle,
    project_id: String,
) -> Result<AudioProject, String> {
    let data = ocr::data(&app)?;
    tauri::async_runtime::spawn_blocking(move || load(&data, &project_id))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn get_ocr_audio_job(
    app: AppHandle,
    project_id: String,
) -> Result<Option<TrialJob>, String> {
    let data = ocr::data(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        read_project(&data, &project_id).map(|p| p.last_job)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn render_ocr_audio_preview(
    app: AppHandle,
    project_id: String,
    job_id: String,
    settings: TrialSettings,
) -> Result<AudioProject, String> {
    render(&app, &project_id, &job_id, settings).await
}
#[tauri::command]
pub fn cancel_ocr_audio_preview(
    app: AppHandle,
    project_id: String,
    job_id: String,
) -> Result<(), String> {
    cancel(&ocr::data(&app)?, &project_id, &job_id)
}
#[tauri::command]
pub async fn export_ocr_audio_preview(
    app: AppHandle,
    project_id: String,
    preview_key: String,
    path: String,
) -> Result<(), String> {
    let data = ocr::data(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        export(&data, &project_id, &preview_key, Path::new(&path))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn preflight_ocr_audio_full(
    app: AppHandle,
    project_id: String,
    settings: TrialSettings,
) -> Result<full::Preflight, String> {
    let data = ocr::data(&app)?;
    tauri::async_runtime::spawn_blocking(move || full::inspect(&data, &project_id, &settings))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn export_ocr_audio_full(
    app: AppHandle,
    project_id: String,
    artifact_key: String,
    directory: String,
) -> Result<String, String> {
    let data = ocr::data(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        full::export_bundle(&data, &project_id, &artifact_key, Path::new(&directory))
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn open_ocr_audio_folder(app: AppHandle, project_id: String) -> Result<(), String> {
    let folder = directory(&ocr::data(&app)?, &project_id)?;
    if !folder.is_dir() {
        return Err("Không tìm thấy dự án audio.".into());
    }
    ocr::open_folder(&folder)
}

#[cfg(test)]
mod tests {
    use super::*;
    const SESSION: &str = "9c4212da-a38b-4452-bb9c-ebfb5d55c66c";
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!("long-tieng-audio-test-{}", Uuid::new_v4()));
            fs::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn cue(index: usize, start: u64, end: u64) -> Cue {
        Cue {
            index,
            start_ms: start,
            end_ms: end,
            text: "Xin chào con.".into(),
            confidence: 95.,
            segment: 0,
            evidence_sequences: vec![index as u64],
            start_range_ms: [start, start],
            end_range_ms: [end, end],
            warnings: vec![],
            reviewed: true,
        }
    }
    pub(super) fn settings() -> TrialSettings {
        TrialSettings {
            start_ms: 30000,
            duration_ms: 30000,
            voice: "thuy_dung".into(),
            full: None,
        }
    }
    pub(super) fn input() -> Source {
        Source {
            session_id: SESSION.into(),
            revision: "a".repeat(64),
            title: "Phim QA".into(),
            url: "https://example.invalid".into(),
            cues: vec![cue(1, 31000, 36000), cue(2, 45000, 50000)],
            coverage: serde_json::json!({"durationMs":120000,"ranges":[{"startMs":0,"endMs":120000,"segment":0}],"gaps":[],"wholeEpisodeVerified":false}),
        }
    }
    fn fixture() -> (Temp, PathBuf, AudioProject) {
        let tmp = Temp::new();
        let data = tmp.0.join("data");
        let s = input();
        let dir = data.join("ocr-sessions").join(SESSION);
        let output = dir.join("ocr-revisions").join(&s.revision);
        fs::create_dir_all(&output).unwrap();
        let cues=serde_json::to_vec_pretty(&serde_json::json!({"schemaVersion":1,"sessionId":SESSION,"revision":s.revision,"cues":s.cues,"coverage":s.coverage})).unwrap();
        let srt = crate::srt::serialize(
            &s.cues
                .iter()
                .map(|c| crate::srt::Cue {
                    index: c.index,
                    start_ms: c.start_ms as i64,
                    end_ms: c.end_ms as i64,
                    text: c.text.clone(),
                })
                .collect::<Vec<_>>(),
        );
        for (name, bytes) in [
            ("subtitles.vi.srt", srt.as_bytes()),
            ("subtitles.vi.txt", b"text".as_slice()),
            ("cues.json", cues.as_slice()),
        ] {
            fs::write(output.join(name), bytes).unwrap();
        }
        fs::write(dir.join("journal.ndjson"), b"capture\n").unwrap();
        fs::write(dir.join("session.json"),serde_json::to_vec(&serde_json::json!({"id":SESSION,"title":s.title,"url":s.url,"sampleCount":2,"nativeStore":{"status":"captured"}})).unwrap()).unwrap();
        fs::write(dir.join("ocr.json"),serde_json::to_vec(&serde_json::json!({"schemaVersion":1,"sessionId":SESSION,"revision":s.revision,"state":"partial","committed":true,"srtHash":sha(srt.as_bytes()),"txtHash":sha(b"text"),"cuesHash":sha(&cues)})).unwrap()).unwrap();
        let p = create(&data, SESSION, &s.revision).unwrap();
        (tmp, data, p)
    }
    fn tone(path: &Path, duration_ms: u64, value: i16) {
        let mut w = hound::WavWriter::create(path, wav_spec()).unwrap();
        for _ in 0..duration_ms * RATE as u64 / 1000 {
            w.write_sample(value).unwrap();
        }
        w.finalize().unwrap();
    }
    #[test]
    fn zero_anchored_timeline_keeps_all_samples_and_gaps() {
        let temp = Temp::new();
        let a = temp.0.join("a.wav");
        let b = temp.0.join("b.wav");
        let output = temp.0.join("trial.wav");
        tone(&a, 500, 1000);
        tone(&b, 750, -1000);
        write_timeline(
            &[(cue(1, 31000, 36000), a), (cue(2, 45000, 50000), b)],
            60000,
            &output,
            &AtomicBool::new(false),
        )
        .unwrap();
        let mut r = hound::WavReader::open(output).unwrap();
        assert_eq!(r.duration(), 60 * RATE);
        let samples = r.samples::<i16>().collect::<Result<Vec<_>, _>>().unwrap();
        assert!(samples[..31 * RATE as usize].iter().all(|s| *s == 0));
        assert_eq!(samples[31 * RATE as usize], 1000);
        assert_eq!(samples[31 * RATE as usize + 23999], 1000);
        assert_eq!(samples[31 * RATE as usize + 24000], 0);
        assert_eq!(samples[45 * RATE as usize], -1000);
        assert_eq!(samples.last(), Some(&0));
        assert_eq!(samples.iter().filter(|v| **v != 0).count(), 60000);
    }
    #[test]
    fn overflow_overlap_bad_format_and_cancel_never_publish_truncated_wav() {
        let temp = Temp::new();
        let raw = temp.0.join("speech.wav");
        tone(&raw, 1500, 1000);
        let output = temp.0.join("trial.wav");
        assert!(write_timeline(
            &[(cue(1, 1000, 2000), raw.clone())],
            30000,
            &output,
            &AtomicBool::new(false)
        )
        .is_err());
        assert!(!output.exists());
        assert!(write_timeline(
            &[
                (cue(1, 1000, 3000), raw.clone()),
                (cue(2, 2000, 4000), raw.clone())
            ],
            30000,
            &output,
            &AtomicBool::new(false)
        )
        .is_err());
        let bad = temp.0.join("bad.wav");
        let mut w = hound::WavWriter::create(
            &bad,
            hound::WavSpec {
                sample_rate: 16000,
                ..wav_spec()
            },
        )
        .unwrap();
        w.write_sample(1i16).unwrap();
        w.finalize().unwrap();
        assert!(write_timeline(
            &[(cue(1, 1000, 3000), bad)],
            30000,
            &output,
            &AtomicBool::new(false)
        )
        .is_err());
        assert!(write_timeline(
            &[(cue(1, 1000, 3000), raw)],
            30000,
            &output,
            &AtomicBool::new(true)
        )
        .is_err());
    }
    #[test]
    fn trial_requires_review_coverage_and_whole_cues() {
        let mut s = input();
        assert_eq!(select(&s, &settings()).unwrap().len(), 2);
        s.cues[0].reviewed = false;
        assert!(select(&s, &settings())
            .unwrap_err()
            .contains("chưa được kiểm tra"));
        s.cues[0].reviewed = true;
        let mut params = settings();
        params.start_ms = 32000;
        assert!(select(&s, &params).is_err());
        params = settings();
        params.voice = "../../evil".into();
        assert!(select(&s, &params).is_err());
        params = settings();
        params.duration_ms = 90000;
        assert!(select(&s, &params).is_err());
        s.cues[1].start_ms = 35000;
        assert!(select(&s, &settings()).is_err());
        s = input();
        s.coverage["ranges"] = serde_json::json!([]);
        assert!(select(&s, &settings()).is_err());
        s = input();
        s.cues = (0..65)
            .map(|i| cue(i + 1, 31000 + i as u64 * 200, 31100 + i as u64 * 200))
            .collect();
        assert!(select(&s, &settings()).unwrap_err().contains("64 cue"));
    }
    #[test]
    fn source_snapshot_is_durable_idempotent_and_has_no_video_path() {
        let (_tmp, data, p) = fixture();
        assert_eq!(create(&data, SESSION, &p.source_revision).unwrap().id, p.id);
        assert_eq!(
            load(&data, &p.id).unwrap().source_revision,
            p.source_revision
        );
        assert!(!serde_json::to_value(&p)
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("videoPath"));
        assert!(!data.join("projects").exists());
        assert!(create(&data, SESSION, &"c".repeat(64)).is_err());
        assert!(load(&data, "../escape").is_err());
        let old = source(&directory(&data, &p.id).unwrap(), &p).unwrap();
        let doc = review::load(&data, SESSION).unwrap();
        let edits = doc
            .cues
            .iter()
            .map(|c| review::EditCue {
                start_ms: c.start_ms,
                end_ms: c.end_ms,
                text: "Câu mới đã sửa.".into(),
                reviewed: true,
                evidence_sequences: c.evidence_sequences.clone(),
            })
            .collect();
        let review = review::save(&data, SESSION, &p.source_revision, edits).unwrap();
        let next = create(&data, SESSION, &review.revision).unwrap();
        assert_ne!(p.id, next.id);
        assert_ne!(p.source_hash, next.source_hash);
        assert_eq!(
            source(&directory(&data, &p.id).unwrap(), &p).unwrap().cues[0].text,
            old.cues[0].text
        );
        fs::write(
            directory(&data, &p.id).unwrap().join("source.json"),
            b"corrupt",
        )
        .unwrap();
        assert!(load(&data, &p.id).is_err());
    }
    #[test]
    fn interrupted_job_and_cancel_are_scoped_to_current_id() {
        let (_tmp, data, mut p) = fixture();
        let dir = directory(&data, &p.id).unwrap();
        let id = Uuid::new_v4().to_string();
        p.last_job = Some(TrialJob {
            progress: project::LongJobState {
                id: id.clone(),
                kind: "ocr-audio-preview".into(),
                status: "running".into(),
                ..Default::default()
            },
            settings: settings(),
            metrics: vec![],
            warnings: vec![],
        });
        save(&dir, &mut p).unwrap();
        let guard = lock(&dir, ".job.lock").unwrap();
        assert_eq!(
            load(&data, &p.id)
                .unwrap()
                .last_job
                .unwrap()
                .progress
                .status,
            "running"
        );
        assert!(lock(&dir, ".job.lock").is_err());
        assert!(cancel(&data, &p.id, &Uuid::new_v4().to_string()).is_err());
        cancel(&data, &p.id, &id).unwrap();
        drop(guard);
        assert_eq!(
            load(&data, &p.id)
                .unwrap()
                .last_job
                .unwrap()
                .progress
                .status,
            "interrupted"
        );
    }
    #[tokio::test]
    async fn cancellation_monitor_ignores_an_older_job() {
        let temp = Temp::new();
        let id = Uuid::new_v4().to_string();
        atomic(
            &temp.0.join("cancel.json"),
            &serde_json::to_vec(&serde_json::json!({"jobId":Uuid::new_v4().to_string()})).unwrap(),
        )
        .unwrap();
        let monitor = monitor(temp.0.clone(), id.clone());
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        assert!(!monitor.flag.load(Ordering::Relaxed));
        atomic(
            &temp.0.join("cancel.json"),
            &serde_json::to_vec(&serde_json::json!({"jobId":id})).unwrap(),
        )
        .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        assert!(monitor.flag.load(Ordering::Relaxed));
    }
    #[tokio::test]
    async fn cancel_before_job_checkpoint_is_not_lost() {
        let (_temp, data, p) = fixture();
        assert!(p.last_job.is_none());
        let id = Uuid::new_v4().to_string();
        cancel(&data, &p.id, &id).unwrap();
        let watcher = monitor(directory(&data, &p.id).unwrap(), id);
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        assert!(watcher.flag.load(Ordering::Relaxed));
    }
    #[test]
    fn committed_wav_hash_and_export_survive_reload() {
        let (tmp, data, mut p) = fixture();
        let dir = directory(&data, &p.id).unwrap();
        let key = "d".repeat(64);
        let folder = dir.join("previews").join(&key);
        fs::create_dir_all(&folder).unwrap();
        let raw = tmp.0.join("raw.wav");
        tone(&raw, 500, 1000);
        write_timeline(
            &[(cue(1, 31000, 36000), raw)],
            60000,
            &folder.join("preview.wav"),
            &AtomicBool::new(false),
        )
        .unwrap();
        let report=serde_json::to_vec(&serde_json::json!({"wavHash":file_hash(&folder.join("preview.wav")).unwrap(),"sourceRevision":p.source_revision,"durationMs":60000,"settings":settings()})).unwrap();
        atomic(&folder.join("report.json"), &report).unwrap();
        p.last_preview = Some(Artifact {
            key: key.clone(),
            path: "/ignored/from/manifest.wav".into(),
            wav_hash: file_hash(&folder.join("preview.wav")).unwrap(),
            report_hash: sha(&report),
            duration_ms: 60000,
            settings: settings(),
            source_revision: p.source_revision.clone(),
            rendered_cues: 1,
            partial: None,
        });
        save(&dir, &mut p).unwrap();
        let loaded = load(&data, &p.id).unwrap();
        assert_eq!(
            loaded.last_preview.unwrap().path,
            folder.join("preview.wav").to_string_lossy()
        );
        let target = tmp.0.join("export.wav");
        export(&data, &p.id, &key, &target).unwrap();
        assert_eq!(
            file_hash(&target).unwrap(),
            p.last_preview.as_ref().unwrap().wav_hash
        );
        assert!(export(&data, &p.id, &key, &dir.join("overwrite.wav")).is_err());
        fs::write(folder.join("preview.wav"), b"broken").unwrap();
        assert!(load(&data, &p.id).is_err());
        assert!(read_project(&data, &p.id).is_ok()); // Job polling cannot publish an unchecked artifact.
    }
    #[test]
    fn cache_identity_changes_with_runtime_voice_revision_and_timing() {
        let raw = project::raw_audio_cache_key("Xin chào", "thuy_dung");
        assert_ne!(raw, project::raw_audio_cache_key("Xin chào", "thai_son"));
        assert_ne!(raw, project::raw_audio_cache_key("Câu khác", "thuy_dung"));
        assert_ne!(
            project_id(SESSION, &"a".repeat(64)),
            project_id(SESSION, &"b".repeat(64))
        );
        let temp = Temp::new();
        let runtime = temp.0.join("engine");
        fs::write(&runtime, b"v1").unwrap();
        let old = fingerprint(std::slice::from_ref(&runtime), "cpu").unwrap();
        fs::write(&runtime, b"v2").unwrap();
        assert_ne!(old, fingerprint(&[runtime], "cpu").unwrap());
    }

    #[test]
    fn full_bundle_atomic_export_and_legacy_manifest_compatibility() {
        let (tmp, data, mut p) = fixture();
        let serialized = serde_json::to_value(&p).unwrap();
        assert!(!serialized.as_object().unwrap().contains_key("lastFull"));
        assert!(serde_json::from_value::<AudioProject>(serialized)
            .unwrap()
            .last_full
            .is_none());
        assert_eq!(
            serde_json::to_value(settings()).unwrap(),
            serde_json::json!({"startMs":30000,"durationMs":30000,"voice":"thuy_dung"})
        );
        let dir = directory(&data, &p.id).unwrap();
        let params = TrialSettings {
            start_ms: 0,
            duration_ms: 120000,
            full: Some(full::Options::default()),
            ..settings()
        };
        let key = "e".repeat(64);
        let folder = dir.join("previews").join(&key);
        fs::create_dir_all(&folder).unwrap();
        let raw = tmp.0.join("tone.wav");
        tone(&raw, 200, 1000);
        write_timeline(
            &[
                (cue(1, 31000, 36000), raw.clone()),
                (cue(2, 45000, 50000), raw),
            ],
            120000,
            &folder.join("preview.wav"),
            &AtomicBool::new(false),
        )
        .unwrap();
        let srt = full::subtitle_bytes(&input());
        atomic(&folder.join("subtitles.vi.srt"), &srt).unwrap();
        let wav_hash = file_hash(&folder.join("preview.wav")).unwrap();
        let report=serde_json::to_vec(&serde_json::json!({"schemaVersion":2,"kind":"ocr-narration","sourceHash":p.source_hash,"sourceSessionId":p.session_id,"sourceUrl":p.url,"title":p.title,"missingRanges":[],"allCuesReviewed":true,"coverageComplete":true,"bitsPerSample":16,"offsetMs":0,"wholeEpisodeVerified":false,"durationSource":"capturedVideo","wavHash":wav_hash,"sourceRevision":p.source_revision,"durationMs":120000,"settings":params,"srtHash":sha(&srt),"anchorMs":0,"sampleRate":RATE,"channels":1,"partial":false})).unwrap();
        atomic(&folder.join("report.json"), &report).unwrap();
        p.last_full = Some(Artifact {
            key: key.clone(),
            path: String::new(),
            wav_hash,
            report_hash: sha(&report),
            duration_ms: 120000,
            settings: params,
            source_revision: p.source_revision.clone(),
            rendered_cues: 2,
            partial: Some(false),
        });
        save(&dir, &mut p).unwrap();
        let exported = full::export_bundle(&data, &p.id, &key, &tmp.0).unwrap();
        assert_eq!(
            full::export_bundle(&data, &p.id, &key, &tmp.0).unwrap(),
            exported
        );
        assert_eq!(
            file_hash(&PathBuf::from(&exported).join("narration.vi.wav")).unwrap(),
            p.last_full.as_ref().unwrap().wav_hash
        );
        assert!(full::export_bundle(&data, &p.id, &key, &dir).is_err());
        fs::write(
            PathBuf::from(&exported).join("subtitles.vi.srt"),
            b"modified",
        )
        .unwrap();
        assert!(full::export_bundle(&data, &p.id, &key, &tmp.0).is_err());
        fs::write(folder.join("subtitles.vi.srt"), b"corrupt").unwrap();
        assert!(load(&data, &p.id).is_err());
    }
    /// Operator-reviewed film crops, only on a separately copied QA session.
    #[test]
    #[ignore = "requires an isolated copy of the documented 2B film capture"]
    fn prepare_reviewed_film_trial() {
        let data = PathBuf::from(std::env::var("LONG_TIENG_AUDIO_FILM_QA").unwrap());
        let data = data.canonicalize().unwrap();
        assert!(data.starts_with("/private/tmp"));
        assert!(data.join(".audio-film-qa-copy").is_file());
        let id = "6978a924-3a1e-4f1c-aee3-2d0ee943407e";
        let doc = review::load(&data, id).unwrap();
        assert_eq!(
            doc.revision,
            "7d4c8b8a5abe87cf3ea187a26cefe5550e56ed19a1f2c34203b94ef87ad38783"
        );
        let groups = [
            (132, 134, "Tôi xin lỗi ạ!"),
            (135, 137, "Thiệt tình, giới trẻ hiện nay thật là..."),
            (
                138,
                138,
                "Thiệt tình, cậu còn bắt tôi nói đi\nnói lại bao nhiêu lần nữa,",
            ),
            (139, 144, "Hanagaki Takemichi-kun?"),
            (
                146,
                150,
                "Khi DVD được khách trả lại,\nphải lập tức đưa lên kệ.",
            ),
            (151, 152, "Tôi đã nói nhiều lắm rồi nhỉ?"),
            (153, 157, "- Tôi xin...\n- Rồi rồi, chỉ biết xin lỗi..."),
            (158, 159, "Đụt thật sự luôn..."),
        ];
        let mut edits = vec![];
        for cue in &doc.cues {
            if cue.index == 145 {
                continue;
            } // Crop 625 has DVD labels, no subtitle.
            if let Some(&(first, last, text)) = groups.iter().find(|g| g.0 == cue.index) {
                let members: Vec<_> = doc
                    .cues
                    .iter()
                    .filter(|c| c.index >= first && c.index <= last)
                    .collect();
                edits.push(review::EditCue {
                    start_ms: members[0].start_ms,
                    end_ms: members.last().unwrap().end_ms,
                    text: text.into(),
                    reviewed: true,
                    evidence_sequences: members
                        .iter()
                        .flat_map(|c| c.evidence_sequences.clone())
                        .collect(),
                });
            } else if !groups.iter().any(|g| cue.index >= g.0 && cue.index <= g.1) {
                edits.push(review::EditCue {
                    start_ms: cue.start_ms,
                    end_ms: cue.end_ms,
                    text: cue.text.clone(),
                    reviewed: cue.reviewed,
                    evidence_sequences: cue.evidence_sequences.clone(),
                });
            }
        }
        let saved = review::save(&data, id, &doc.revision, edits).unwrap();
        let p = create(&data, id, &saved.revision).unwrap();
        let params = TrialSettings {
            start_ms: 85650,
            duration_ms: 30000,
            voice: "thuy_dung".into(),
            full: None,
        };
        assert_eq!(
            select(
                &source(&directory(&data, &p.id).unwrap(), &p).unwrap(),
                &params
            )
            .unwrap()
            .len(),
            8
        );
        println!(
            "Reviewed film audio project: {} revision {}",
            p.id, p.source_revision
        );
    }
}
