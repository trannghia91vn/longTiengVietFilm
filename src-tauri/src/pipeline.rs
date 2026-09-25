use crate::srt::{self, Cue, SubtitleDocument};
use crate::{diagnostics, models};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio::time::{sleep, Duration};
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};
use uuid::Uuid;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemStatus {
    pub ffmpeg: bool,
    pub ffprobe: bool,
    pub whisper_cli: bool,
    pub llama_cli: bool,
    pub llama_server: bool,
    pub vieneu_cli: bool,
    pub audio_cpp: bool,
    pub app_data_dir: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub path: String,
    pub file_name: String,
    pub duration_ms: i64,
    pub width: Option<u64>,
    pub height: Option<u64>,
    pub audio_codec: Option<String>,
    pub video_codec: Option<String>,
    pub audio_channels: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub output: SubtitleDocument,
    pub offset_ms: i64,
    pub speed_ratio: f64,
    pub matched_cues: usize,
    pub confidence: f64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoPreviewProgress {
    pub percent: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DubResult {
    pub path: String,
    pub rendered_cues: usize,
    pub duration_ms: i64,
    pub voice: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DubProgress {
    pub phase: String,
    pub current: usize,
    pub total: usize,
    pub percent: f64,
    pub message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationWarning {
    pub cue_index: Option<usize>,
    pub code: String,
    pub message: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationResult {
    pub output: SubtitleDocument,
    pub warnings: Vec<TranslationWarning>,
    pub edited_cues: usize,
    pub fallback_cues: usize,
    pub quality: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationProgress {
    pub phase: String,
    pub current: usize,
    pub total: usize,
    pub percent: f64,
    pub message: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedSceneContext {
    pub id: String,
    pub start_index: usize,
    pub end_index: usize,
    pub summary: String,
    pub tone: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedTranslationGuide {
    pub synopsis: String,
    pub tone: String,
    pub relationships: Vec<String>,
    pub address_rules: Vec<String>,
    pub scenes: Vec<PreparedSceneContext>,
}

pub trait AsrProvider {
    fn command(&self, audio: &Path, output_prefix: &Path, cpu_only: bool) -> Command;
    fn detect_command(&self, audio: &Path, cpu_only: bool) -> Command;
}

struct WhisperCppProvider {
    executable: PathBuf,
    model: PathBuf,
    vad_model: PathBuf,
    language: String,
}

impl AsrProvider for WhisperCppProvider {
    fn command(&self, audio: &Path, output_prefix: &Path, cpu_only: bool) -> Command {
        let mut command = Command::new(&self.executable);
        command
            .arg("-m")
            .arg(&self.model)
            .arg("-f")
            .arg(audio)
            .args(["-l", &self.language, "--vad", "-vm"])
            .arg(&self.vad_model)
            .args(["-mc", "0", "-sns", "-osrt", "-of"])
            .arg(output_prefix);
        if cpu_only {
            command.arg("-ng");
        }
        command
    }

    fn detect_command(&self, audio: &Path, cpu_only: bool) -> Command {
        let mut command = Command::new(&self.executable);
        command
            .arg("-m")
            .arg(&self.model)
            .arg("-f")
            .arg(audio)
            .args(["-l", "auto", "--detect-language", "--vad", "-vm"])
            .arg(&self.vad_model);
        if cpu_only {
            command.arg("-ng");
        }
        command
    }
}

pub trait TtsProvider {
    fn engine_available(&self) -> bool;
}

struct VieNeuTtsProvider {
    executable: Option<PathBuf>,
}

impl TtsProvider for VieNeuTtsProvider {
    fn engine_available(&self) -> bool {
        self.executable.is_some()
    }
}

pub(crate) fn find_executable(app: &AppHandle, candidates: &[&str]) -> Option<PathBuf> {
    let mut roots = vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("engines")];
    if let Ok(resource_dir) = app.path().resource_dir() {
        roots.insert(0, resource_dir.join("engines"));
    }
    for candidate in candidates {
        for root in &roots {
            let path = root.join(candidate);
            if path.is_file() {
                return Some(path);
            }
        }
        if let Ok(path) = which::which(candidate) {
            return Some(path);
        }
    }
    None
}

fn executable(app: &AppHandle, candidates: &[&str]) -> Option<PathBuf> {
    find_executable(app, candidates)
}

pub fn status(app: &AppHandle) -> Result<SystemStatus, String> {
    let data = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    let tts = VieNeuTtsProvider {
        executable: executable(app, &["vieneu-cli", "audiocpp_cli", "audiocpp-cli"]),
    };
    Ok(SystemStatus {
        ffmpeg: executable(app, &["ffmpeg"]).is_some(),
        ffprobe: executable(app, &["ffprobe"]).is_some(),
        whisper_cli: executable(app, &["whisper-cli", "whisper-cpp"]).is_some(),
        llama_cli: executable(app, &["llama-cli"]).is_some(),
        llama_server: executable(app, &["llama-server"]).is_some(),
        vieneu_cli: tts.engine_available(),
        audio_cpp: executable(app, &["audiocpp_cli", "audiocpp-cli"]).is_some(),
        app_data_dir: data.to_string_lossy().into_owned(),
    })
}

async fn output(command: &mut Command, label: &str) -> Result<std::process::Output, String> {
    let result = command
        .output()
        .await
        .map_err(|error| format!("Không chạy được {label}: {error}"))?;
    if result.status.success() {
        Ok(result)
    } else {
        let stderr = String::from_utf8_lossy(&result.stderr);
        Err(format!("{label} thất bại: {}", stderr.trim()))
    }
}

fn is_metal_memory_error(error: &str) -> bool {
    error.contains("failed to allocate Metal buffer") || error.contains("out of memory")
}

async fn whisper_output(
    app: &AppHandle,
    provider: &WhisperCppProvider,
    audio: &Path,
    output_prefix: Option<&Path>,
) -> Result<std::process::Output, String> {
    let first = match output_prefix {
        Some(prefix) => output(&mut provider.command(audio, prefix, false), "Whisper").await,
        None => output(&mut provider.detect_command(audio, false), "Whisper detect").await,
    };
    match first {
        Err(error) if is_metal_memory_error(&error) => {
            let _ = diagnostics::append(
                app,
                diagnostics::entry(
                    "warning",
                    "whisper.runtime",
                    "Metal không đủ bộ nhớ, chuyển sang CPU",
                    Some(error),
                ),
            );
            match output_prefix {
                Some(prefix) => {
                    output(&mut provider.command(audio, prefix, true), "Whisper CPU").await
                }
                None => {
                    output(
                        &mut provider.detect_command(audio, true),
                        "Whisper detect CPU",
                    )
                    .await
                }
            }
        }
        result => result,
    }
}

fn parse_detected_language(result: &std::process::Output) -> Option<String> {
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    parse_detected_language_text(&text)
}

fn parse_detected_language_text(text: &str) -> Option<String> {
    let marker = "auto-detected language:";
    let start = text.find(marker)? + marker.len();
    text[start..]
        .split_whitespace()
        .next()
        .map(|value| value.trim_matches(|character: char| !character.is_ascii_alphabetic()))
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn language_name(code: &str) -> &'static str {
    match code {
        "en" => "tiếng Anh",
        "ja" => "tiếng Nhật",
        "ko" => "tiếng Hàn",
        "zh" => "tiếng Trung",
        "fr" => "tiếng Pháp",
        "es" => "tiếng Tây Ban Nha",
        _ => "ngôn ngữ nguồn",
    }
}

pub async fn inspect_media(app: &AppHandle, path: &str) -> Result<MediaInfo, String> {
    let ffprobe = executable(app, &["ffprobe"]).ok_or_else(|| {
        "Không tìm thấy ffprobe. Hãy cài hoặc đóng gói FFmpeg sidecar.".to_string()
    })?;
    let result = output(
        Command::new(ffprobe).args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            path,
        ]),
        "ffprobe",
    )
    .await?;
    let json: Value = serde_json::from_slice(&result.stdout).map_err(|error| error.to_string())?;
    let streams = json["streams"].as_array().cloned().unwrap_or_default();
    let video = streams
        .iter()
        .find(|stream| stream["codec_type"] == "video");
    let audio = streams
        .iter()
        .find(|stream| stream["codec_type"] == "audio");
    let duration = json["format"]["duration"]
        .as_str()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or_default();
    Ok(MediaInfo {
        path: path.to_string(),
        file_name: Path::new(path)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(path)
            .to_string(),
        duration_ms: (duration * 1000.0).round() as i64,
        width: video.and_then(|stream| stream["width"].as_u64()),
        height: video.and_then(|stream| stream["height"].as_u64()),
        audio_codec: audio
            .and_then(|stream| stream["codec_name"].as_str())
            .map(str::to_string),
        video_codec: video
            .and_then(|stream| stream["codec_name"].as_str())
            .map(str::to_string),
        audio_channels: audio.and_then(|stream| stream["channels"].as_u64()),
    })
}

fn browser_can_play_direct(path: &str, media: &MediaInfo) -> bool {
    let extension = Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let container_supported = matches!(extension.as_str(), "mp4" | "m4v" | "mov");
    let video_supported = media.video_codec.as_deref() == Some("h264");
    let audio_supported = media.audio_codec.is_none()
        || matches!(media.audio_codec.as_deref(), Some("aac" | "mp3" | "alac"));
    container_supported && video_supported && audio_supported
}

fn preview_cache_path(app: &AppHandle, input_path: &str) -> Result<PathBuf, String> {
    let metadata = std::fs::metadata(input_path).map_err(|error| error.to_string())?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_secs())
        .unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(b"preview-mp4v-v3");
    hasher.update(input_path.as_bytes());
    hasher.update(metadata.len().to_le_bytes());
    hasher.update(modified.to_le_bytes());
    let key = hex::encode(hasher.finalize());
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("previews");
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory.join(format!("{key}.mp4")))
}

pub async fn prepare_video_preview(
    app: &AppHandle,
    input_path: &str,
    force_proxy: bool,
) -> Result<String, String> {
    let media = inspect_media(app, input_path).await?;
    if !force_proxy && browser_can_play_direct(input_path, &media) {
        return Ok(input_path.to_string());
    }

    let output_path = preview_cache_path(app, input_path)?;
    if output_path
        .metadata()
        .map(|item| item.len() > 0)
        .unwrap_or(false)
    {
        return Ok(output_path.to_string_lossy().into_owned());
    }

    let ffmpeg = executable(app, &["ffmpeg"])
        .ok_or_else(|| "Không tìm thấy FFmpeg để tạo video preview.".to_string())?;
    let temporary_path = output_path.with_file_name(format!("preview-{}.mp4", Uuid::new_v4()));
    let can_copy_video = media.video_codec.as_deref() == Some("h264");
    let mut command = Command::new(ffmpeg);
    command
        .args(["-y", "-i", input_path, "-map", "0:v:0", "-map", "0:a:0?"])
        .args(["-sn", "-dn"]);
    if can_copy_video {
        command.args(["-c:v", "copy"]);
    } else {
        command.args([
            "-vf",
            "scale='min(854,iw)':-2",
            "-c:v",
            "mpeg4",
            "-q:v",
            "12",
        ]);
    }
    command
        .args([
            "-c:a",
            "aac",
            "-b:a",
            "96k",
            "-movflags",
            "+faststart",
            "-progress",
            "pipe:1",
            "-nostats",
            "-loglevel",
            "error",
        ])
        .arg(&temporary_path);

    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| format!("Không chạy được FFmpeg tạo video preview: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Không đọc được tiến độ FFmpeg.".to_string())?;
    let mut lines = BufReader::new(stdout).lines();
    while let Some(line) = lines.next_line().await.map_err(|error| error.to_string())? {
        if let Some(value) = line.strip_prefix("out_time_ms=") {
            if let Ok(microseconds) = value.parse::<f64>() {
                let duration_microseconds = media.duration_ms.max(1) as f64 * 1_000.0;
                let percent = (microseconds / duration_microseconds * 100.0).clamp(0.0, 99.5);
                let _ = app.emit("video-preview-progress", VideoPreviewProgress { percent });
            }
        }
    }
    let status = child.wait().await.map_err(|error| error.to_string())?;
    let mut stderr_bytes = Vec::new();
    if let Some(mut stderr) = child.stderr.take() {
        stderr
            .read_to_end(&mut stderr_bytes)
            .await
            .map_err(|error| error.to_string())?;
    }
    if !status.success() {
        let _ = std::fs::remove_file(&temporary_path);
        return Err(format!(
            "FFmpeg tạo video preview thất bại: {}",
            String::from_utf8_lossy(&stderr_bytes).trim()
        ));
    }
    std::fs::rename(&temporary_path, &output_path).map_err(|error| error.to_string())?;
    let _ = app.emit(
        "video-preview-progress",
        VideoPreviewProgress { percent: 100.0 },
    );
    let _ = diagnostics::append(
        app,
        diagnostics::entry(
            "info",
            "preview.ready",
            "Đã tạo video preview tương thích",
            Some(output_path.to_string_lossy().into_owned()),
        ),
    );
    Ok(output_path.to_string_lossy().into_owned())
}

fn workspace_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("workspace")
        .join(Uuid::new_v4().to_string());
    std::fs::create_dir_all(&path).map_err(|error| error.to_string())?;
    Ok(path)
}

pub async fn transcribe_preview(
    app: &AppHandle,
    video_path: &str,
    seconds: u64,
    requested_language: &str,
) -> Result<SubtitleDocument, String> {
    let ffmpeg =
        executable(app, &["ffmpeg"]).ok_or_else(|| "Không tìm thấy FFmpeg.".to_string())?;
    let whisper = executable(app, &["whisper-cli", "whisper-cpp"]).ok_or_else(|| {
        "Đã có model nhưng chưa tìm thấy whisper-cli. Bản phát hành cần đóng gói sidecar này."
            .to_string()
    })?;
    let model = models::model_path(
        app,
        "whisper-large-v3-turbo-q5",
        "ggml-large-v3-turbo-q5_0.bin",
    )?;
    if !model.exists() {
        return Err("Chưa tải Whisper large-v3-turbo Q5 trong Model Manager.".to_string());
    }
    let vad_model = models::model_path(app, "whisper-large-v3-turbo-q5", "ggml-silero-v6.2.0.bin")?;
    if !vad_model.exists() {
        return Err(
            "Thiếu Silero VAD. Vào Model Manager và bấm Tải tiếp cho Whisper (~865 KB)."
                .to_string(),
        );
    }
    let work = workspace_dir(app)?;
    let wav = work.join("preview.wav");
    output(
        Command::new(ffmpeg)
            .args([
                "-y",
                "-i",
                video_path,
                "-t",
                &seconds.to_string(),
                "-vn",
                "-ar",
                "16000",
                "-ac",
                "1",
                "-c:a",
                "pcm_s16le",
            ])
            .arg(&wav),
        "FFmpeg trích âm thanh",
    )
    .await?;
    let mut provider = WhisperCppProvider {
        executable: whisper,
        model,
        vad_model,
        language: requested_language.to_string(),
    };
    if requested_language == "auto" {
        let detection = whisper_output(app, &provider, &wav, None).await?;
        provider.language = parse_detected_language(&detection)
            .ok_or_else(|| "Whisper không xác định được ngôn ngữ nguồn.".to_string())?;
        let _ = diagnostics::append(
            app,
            diagnostics::entry(
                "info",
                "whisper.language",
                format!("Đã nhận diện ngôn ngữ: {}", provider.language),
                None,
            ),
        );
    }
    let prefix = work.join(format!("source_{}", provider.language));
    whisper_output(app, &provider, &wav, Some(&prefix)).await?;
    let srt_path = prefix.with_extension("srt");
    srt::read(&srt_path, &provider.language)
}

#[derive(Deserialize)]
struct ChatCompletion {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Deserialize)]
struct ChatMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
struct TranslationPayload {
    translations: Vec<TranslationItem>,
}

#[derive(Deserialize)]
struct TranslationItem {
    id: usize,
    text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TranslationChunk {
    start: usize,
    end: usize,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PipelineSceneContext {
    id: String,
    start_index: usize,
    end_index: usize,
    summary: String,
    tone: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PipelineTranslationGuide {
    synopsis: String,
    tone: String,
    user_notes: String,
    address_rules: Vec<String>,
    scenes: Vec<PipelineSceneContext>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct PipelineMemoryExample {
    source: String,
    machine: String,
    preferred: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct AcceptedTranslation {
    id: usize,
    text: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PipelineProjectContext {
    translation_guide: PipelineTranslationGuide,
    only_scene_id: Option<String>,
    memory_examples: Vec<PipelineMemoryExample>,
    accepted_translations: Vec<AcceptedTranslation>,
}

struct LocalLlamaServer {
    child: tokio::process::Child,
    client: reqwest::Client,
    endpoint: String,
}

impl LocalLlamaServer {
    async fn start(executable: &Path, model: &Path, reasoning_off: bool) -> Result<Self, String> {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
            .map_err(|error| format!("Không chọn được cổng cho llama-server: {error}"))?;
        let port = listener
            .local_addr()
            .map_err(|error| error.to_string())?
            .port();
        drop(listener);
        let port_string = port.to_string();

        let mut command = Command::new(executable);
        command
            .arg("-m")
            .arg(model)
            .args([
                "--host",
                "127.0.0.1",
                "--port",
                &port_string,
                "-c",
                "8192",
                "-ngl",
                "99",
                "--parallel",
                "1",
                "--jinja",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if reasoning_off {
            command.args(["--reasoning", "off"]);
        }
        let mut child = command
            .spawn()
            .map_err(|error| format!("Không chạy được llama-server: {error}"))?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(180))
            .build()
            .map_err(|error| error.to_string())?;
        let endpoint = format!("http://127.0.0.1:{port}");

        for _ in 0..720 {
            if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
                let mut details = String::new();
                if let Some(mut stderr) = child.stderr.take() {
                    let _ = stderr.read_to_string(&mut details).await;
                }
                return Err(format!(
                    "llama-server dừng khi nạp model ({status}): {}",
                    details.trim()
                ));
            }
            if client
                .get(format!("{endpoint}/health"))
                .send()
                .await
                .map(|response| response.status().is_success())
                .unwrap_or(false)
            {
                return Ok(Self {
                    child,
                    client,
                    endpoint,
                });
            }
            sleep(Duration::from_millis(250)).await;
        }
        let _ = child.start_kill();
        Err("llama-server nạp model quá 3 phút.".to_string())
    }

    async fn chat(
        &self,
        system: &str,
        prompt: &str,
        ids: &[usize],
        temperature: f64,
        top_p: f64,
        top_k: u64,
        repeat_penalty: f64,
    ) -> Result<HashMap<usize, String>, String> {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "translations": {
                    "type": "array",
                    "minItems": ids.len(),
                    "maxItems": ids.len(),
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "integer", "enum": ids },
                            "text": { "type": "string", "minLength": 1 }
                        },
                        "required": ["id", "text"],
                        "additionalProperties": false
                    }
                }
            },
            "required": ["translations"],
            "additionalProperties": false
        });
        let response = self
            .client
            .post(format!("{}/v1/chat/completions", self.endpoint))
            .json(&serde_json::json!({
                "messages": [
                    { "role": "system", "content": system },
                    { "role": "user", "content": prompt }
                ],
                "temperature": temperature,
                "top_p": top_p,
                "top_k": top_k,
                "repeat_penalty": repeat_penalty,
                "max_tokens": 4096,
                "response_format": {
                    "type": "json_schema",
                    "json_schema": {
                        "name": "subtitle_translations",
                        "strict": true,
                        "schema": schema
                    }
                }
            }))
            .send()
            .await
            .map_err(|error| format!("Không gọi được llama-server: {error}"))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| format!("Không đọc được phản hồi llama-server: {error}"))?;
        if !status.is_success() {
            return Err(format!("llama-server trả lỗi {status}: {body}"));
        }
        let completion: ChatCompletion = serde_json::from_str(&body)
            .map_err(|error| format!("Phản hồi llama-server không hợp lệ: {error}"))?;
        let content = completion
            .choices
            .first()
            .and_then(|choice| choice.message.content.as_deref())
            .ok_or_else(|| "Model không trả nội dung.".to_string())?;
        parse_translation_json(content, ids)
    }

    async fn stop(mut self) {
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
    }
}

impl Drop for LocalLlamaServer {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

fn emit_translation_progress(
    app: &AppHandle,
    phase: &str,
    current: usize,
    total: usize,
    percent: f64,
    message: impl Into<String>,
) {
    let message = message.into();
    let percent = percent.clamp(0.0, 100.0);
    let _ = app.emit(
        "translation-progress",
        TranslationProgress {
            phase: phase.to_string(),
            current,
            total,
            percent,
            message: message.clone(),
        },
    );
    let _ = app.emit(
        "project-progress",
        serde_json::json!({
            "stage": phase,
            "current": current,
            "total": total,
            "percent": percent,
            "message": message,
        }),
    );
}

fn translation_chunks(cues: &[Cue]) -> Vec<TranslationChunk> {
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < cues.len() {
        let mut end = start;
        let mut estimated_tokens = 0;
        while end < cues.len() && end - start < 24 {
            let cue_tokens = cues[end].text.chars().count().div_ceil(4).max(1);
            if end > start && estimated_tokens + cue_tokens > 1_500 {
                break;
            }
            estimated_tokens += cue_tokens;
            end += 1;
        }
        chunks.push(TranslationChunk { start, end });
        start = end;
    }
    chunks
}

fn semantic_scene_chunks(cues: &[Cue]) -> Vec<TranslationChunk> {
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < cues.len() {
        let scene_start = cues[start].start_ms;
        let mut end = start + 1;
        while end < cues.len() {
            let count = end - start;
            let duration = cues[end].end_ms - scene_start;
            let gap = cues[end].start_ms - cues[end - 1].end_ms;
            if count >= 48 || duration > 120_000 || (count >= 4 && gap >= 2_500) {
                break;
            }
            end += 1;
        }
        chunks.push(TranslationChunk { start, end });
        start = end;
    }
    chunks
}

fn guided_scene_chunks(cues: &[Cue], context: &PipelineProjectContext) -> Vec<TranslationChunk> {
    let mut chunks = context
        .translation_guide
        .scenes
        .iter()
        .filter(|scene| {
            context
                .only_scene_id
                .as_deref()
                .map_or(true, |selected| selected == scene.id)
        })
        .filter_map(|scene| {
            let start = cues.iter().position(|cue| cue.index == scene.start_index)?;
            let end = cues
                .iter()
                .rposition(|cue| cue.index == scene.end_index)?
                .saturating_add(1);
            (start < end).then_some(TranslationChunk { start, end })
        })
        .collect::<Vec<_>>();
    if chunks.is_empty() && context.only_scene_id.is_none() {
        chunks = semantic_scene_chunks(cues);
    }
    chunks
}

fn scene_for_chunk<'a>(
    cues: &[Cue],
    chunk: TranslationChunk,
    context: &'a PipelineProjectContext,
) -> Option<&'a PipelineSceneContext> {
    let first = cues.get(chunk.start)?.index;
    context
        .translation_guide
        .scenes
        .iter()
        .find(|scene| first >= scene.start_index && first <= scene.end_index)
}

fn guide_brief(context: &PipelineProjectContext) -> Value {
    serde_json::json!({
        "synopsis": context.translation_guide.synopsis,
        "tone": context.translation_guide.tone,
        "userNotes": context.translation_guide.user_notes,
        "addressRules": context.translation_guide.address_rules,
    })
}

fn compact_prompt_context(raw: Option<&str>) -> Option<String> {
    let mut value = serde_json::from_str::<Value>(raw?).ok()?;
    if let Some(object) = value.as_object_mut() {
        object.remove("translationGuide");
        object.remove("onlySceneId");
        object.remove("acceptedTranslations");
        object.remove("memoryExamples");
    }
    serde_json::to_string(&value).ok()
}

fn ends_utterance(text: &str) -> bool {
    text.trim()
        .trim_end_matches(['"', '\'', '”', '’', '」', '』', ')', ']', '…'])
        .ends_with(['.', '!', '?', '。', '！', '？', '…'])
}

pub fn utterance_groups(cues: &[Cue]) -> Vec<String> {
    let mut group = 1_usize;
    cues.iter()
        .enumerate()
        .map(|(index, cue)| {
            if index > 0 {
                let previous = &cues[index - 1];
                let gap = cue.start_ms - previous.end_ms;
                if gap > 700 || ends_utterance(&previous.text) {
                    group += 1;
                }
            }
            format!("utterance-{group:05}")
        })
        .collect()
}

fn chunk_context<'a>(cues: &'a [Cue], chunk: TranslationChunk) -> (&'a [Cue], &'a [Cue]) {
    let context_start = chunk.start.saturating_sub(4);
    let context_end = (chunk.end + 4).min(cues.len());
    (
        &cues[context_start..context_end],
        &cues[chunk.start..chunk.end],
    )
}

fn cue_json(cue: &Cue) -> Value {
    serde_json::json!({
        "id": cue.index,
        "start_ms": cue.start_ms,
        "end_ms": cue.end_ms,
        "text": cue.text
    })
}

fn parse_translation_json(
    raw: &str,
    expected_ids: &[usize],
) -> Result<HashMap<usize, String>, String> {
    let clean = raw
        .trim()
        .strip_prefix("```json")
        .or_else(|| raw.trim().strip_prefix("```"))
        .unwrap_or(raw.trim())
        .trim_end_matches("```")
        .trim();
    let json = if clean.starts_with('{') {
        clean
    } else {
        let start = clean
            .find('{')
            .ok_or_else(|| "Model không trả JSON.".to_string())?;
        let end = clean
            .rfind('}')
            .ok_or_else(|| "JSON bị thiếu dấu đóng.".to_string())?;
        &clean[start..=end]
    };
    let payload: TranslationPayload = serde_json::from_str(json)
        .map_err(|error| format!("Không đọc được JSON bản dịch: {error}"))?;
    let expected: HashSet<usize> = expected_ids.iter().copied().collect();
    let mut result = HashMap::new();
    for item in payload.translations {
        let text = item.text.trim();
        if !expected.contains(&item.id) {
            return Err(format!("Model trả ID ngoài cụm: {}", item.id));
        }
        if text.is_empty() || result.insert(item.id, text.to_string()).is_some() {
            return Err(format!("ID {} bị trống hoặc trùng.", item.id));
        }
    }
    let missing = expected_ids
        .iter()
        .copied()
        .filter(|id| !result.contains_key(id))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!("Model thiếu ID: {missing:?}"));
    }
    Ok(result)
}

async fn chat_with_retry(
    server: &LocalLlamaServer,
    system: &str,
    prompt: &str,
    ids: &[usize],
    parameters: (f64, f64, u64, f64),
) -> Result<HashMap<usize, String>, String> {
    let first = server
        .chat(
            system,
            prompt,
            ids,
            parameters.0,
            parameters.1,
            parameters.2,
            parameters.3,
        )
        .await;
    match first {
        Ok(value) => Ok(value),
        Err(first_error) => server
            .chat(
                system,
                &format!(
                    "{prompt}\n\nLần trước sai định dạng: {first_error}. Hãy trả đúng đủ các ID, mỗi ID đúng một lần."
                ),
                ids,
                parameters.0,
                parameters.1,
                parameters.2,
                parameters.3,
            )
            .await
            .map_err(|second_error| format!("{first_error}; thử lại vẫn lỗi: {second_error}")),
    }
}

fn profile_lines(value: &str) -> Vec<String> {
    value
        .lines()
        .map(|line| {
            line.trim()
                .trim_start_matches(['-', '*', '•', ' '])
                .trim()
                .to_string()
        })
        .filter(|line| !line.is_empty())
        .collect()
}

pub async fn prepare_translation_guide(
    app: &AppHandle,
    cues: &[Cue],
    source_language: &str,
    profile: &str,
    manual_context: &str,
) -> Result<PreparedTranslationGuide, String> {
    if cues.is_empty() {
        return Err("Không có lời thoại để chuẩn bị ngữ cảnh dịch.".into());
    }
    let llama_server = executable(app, &["llama-server"])
        .ok_or_else(|| "Không tìm thấy llama-server để chuẩn bị ngữ cảnh.".to_string())?;
    let model = models::model_path(app, "qwen3-14b-q4", "Qwen3-14B-Q4_K_M.gguf")?;
    if !model.exists() {
        return Err("Chưa tải Qwen3 14B để chuẩn bị ngữ cảnh dịch.".to_string());
    }
    let chunks = semantic_scene_chunks(cues);
    emit_translation_progress(
        app,
        "context-loading",
        0,
        chunks.len(),
        1.0,
        "Đang nạp Qwen3 để đọc toàn bộ kịch bản nguồn...",
    );
    let server = LocalLlamaServer::start(&llama_server, &model, true).await?;
    let mut scenes = Vec::with_capacity(chunks.len());
    let mut rolling_summary = String::new();
    let mut relationships = Vec::new();
    let mut address_rules = Vec::new();
    for (position, chunk) in chunks.iter().copied().enumerate() {
        emit_translation_progress(
            app,
            "context-scenes",
            position + 1,
            chunks.len(),
            3.0 + position as f64 / chunks.len() as f64 * 82.0,
            format!("Đang đọc cảnh {}/{}", position + 1, chunks.len()),
        );
        let material = cues[chunk.start..chunk.end]
            .iter()
            .map(cue_json)
            .collect::<Vec<_>>();
        let ids = [1, 2, 3, 4];
        let prompt = format!(
            "Phân tích cảnh phim bằng {} để chuẩn bị dịch sang tiếng Việt. ID 1: tóm tắt diễn biến, hàm ý và thông tin mới của cảnh. ID 2: sắc thái cảm xúc và phong cách lời thoại. ID 3: quan hệ nhân vật hoặc vai vế có căn cứ, mỗi mục một dòng. ID 4: quy tắc xưng hô hoặc thuật ngữ cần giữ, mỗi mục một dòng. Không sửa từng cue ở bước này, không bịa tên hoặc quan hệ.\nPhong cách dự án: {profile}\nGhi chú thủ công: {manual_context}\nDiễn biến trước đó: {}\nCảnh hiện tại: {}",
            language_name(source_language),
            if rolling_summary.is_empty() { "Chưa có" } else { &rolling_summary },
            serde_json::to_string(&material).unwrap_or_default()
        );
        let mut result = chat_with_retry(
            &server,
            "Bạn là biên tập viên kịch bản Nhật-Anh-Việt. Chỉ trả JSON đúng schema.",
            &prompt,
            &ids,
            (0.2, 0.8, 20, 1.05),
        )
        .await
        .map_err(|error| format!("Không phân tích được cảnh {}: {error}", position + 1))?;
        let summary = result.remove(&1).unwrap_or_default();
        let tone = result.remove(&2).unwrap_or_default();
        for value in profile_lines(&result.remove(&3).unwrap_or_default()) {
            if !relationships.contains(&value) {
                relationships.push(value);
            }
        }
        for value in profile_lines(&result.remove(&4).unwrap_or_default()) {
            if !address_rules.contains(&value) {
                address_rules.push(value);
            }
        }
        if !summary.is_empty() {
            if !rolling_summary.is_empty() {
                rolling_summary.push_str(" | ");
            }
            rolling_summary.push_str(&summary);
            if rolling_summary.chars().count() > 2_000 {
                rolling_summary = rolling_summary
                    .chars()
                    .rev()
                    .take(2_000)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect();
            }
        }
        scenes.push(PreparedSceneContext {
            id: format!("scene-{:04}", position + 1),
            start_index: cues[chunk.start].index,
            end_index: cues[chunk.end - 1].index,
            summary,
            tone,
        });
    }
    emit_translation_progress(
        app,
        "context-merge",
        scenes.len(),
        scenes.len(),
        88.0,
        "Đang tổng hợp mạch phim, quan hệ và cách xưng hô...",
    );
    let summaries = scenes
        .iter()
        .map(|scene| {
            serde_json::json!({
                "scene": scene.id,
                "summary": scene.summary,
                "tone": scene.tone
            })
        })
        .collect::<Vec<_>>();
    let merge_material = if summaries.len() > 24 {
        let mut chapters = Vec::new();
        for (chapter_index, chapter) in summaries.chunks(20).enumerate() {
            let ids = [1];
            let mut result = chat_with_retry(
                &server,
                "Bạn là biên tập viên kịch bản. Chỉ trả JSON đúng schema.",
                &format!(
                    "Tóm tắt tuần tự nhóm cảnh này thành một chương, giữ các quan hệ, bước ngoặt và thông tin cần cho xưng hô. ID 1 là nội dung chương. Không bịa thêm.\nCảnh: {}",
                    serde_json::to_string(chapter).unwrap_or_default()
                ),
                &ids,
                (0.15, 0.75, 20, 1.05),
            )
            .await?;
            chapters.push(serde_json::json!({
                "chapter": chapter_index + 1,
                "summary": result.remove(&1).unwrap_or_default()
            }));
        }
        chapters
    } else {
        summaries
    };
    let ids = [1, 2];
    let mut merged = chat_with_retry(
        &server,
        "Bạn là biên tập viên thoại phim tiếng Việt. Chỉ trả JSON đúng schema.",
        &format!(
            "Tổng hợp các cảnh thành hồ sơ dịch. ID 1: synopsis toàn phim ngắn gọn nhưng đủ diễn biến và quan hệ. ID 2: mô tả giọng điệu tiếng Việt cần duy trì. Không thêm chi tiết không có trong dữ liệu.\nPhong cách: {profile}\nCảnh: {}",
            serde_json::to_string(&merge_material).unwrap_or_default()
        ),
        &ids,
        (0.15, 0.75, 20, 1.05),
    )
    .await?;
    server.stop().await;
    emit_translation_progress(
        app,
        "context-complete",
        scenes.len(),
        scenes.len(),
        100.0,
        "Đã đọc toàn bộ kịch bản nguồn",
    );
    Ok(PreparedTranslationGuide {
        synopsis: merged.remove(&1).unwrap_or_default(),
        tone: merged.remove(&2).unwrap_or_default(),
        relationships,
        address_rules,
        scenes,
    })
}

fn hy_translation_prompt(
    all_cues: &[Cue],
    chunk: TranslationChunk,
    source_language: &str,
    project_context: Option<&str>,
    pipeline_context: &PipelineProjectContext,
    translated: &HashMap<usize, String>,
) -> String {
    let (context, targets) = chunk_context(all_cues, chunk);
    let context_json = context.iter().map(cue_json).collect::<Vec<_>>();
    let target_ids = targets.iter().map(|cue| cue.index).collect::<Vec<_>>();
    let previous_vi = all_cues[..chunk.start]
        .iter()
        .rev()
        .filter_map(|cue| {
            translated
                .get(&cue.index)
                .map(|text| serde_json::json!({ "id": cue.index, "text": text }))
        })
        .take(8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>();
    let scene = scene_for_chunk(all_cues, chunk, pipeline_context);
    let utterance_groups = utterance_groups(targets);
    format!(
        "Dịch các cue mục tiêu từ {} sang tiếng Việt theo đúng mạch của cả cảnh. Xem những cue chung utterance_group là các mảnh của cùng một câu: dịch như một câu hoàn chỉnh rồi chia lại tự nhiên, không lặp chủ ngữ hay đại từ ở mỗi cue. Ưu tiên chính xác ý nghĩa, hàm ý, tên riêng, số liệu, thuật ngữ và sắc thái. Tránh cấu trúc dịch từng chữ. Giữ tag HTML. Các cue lân cận chỉ là ngữ cảnh, không được xuất lặp. Trả đúng JSON theo schema với duy nhất các ID mục tiêu.\nTóm tắt cảnh: {}\nSắc thái cảnh: {}\nTám câu Việt trước đó: {}\nUtterance group theo thứ tự ID: {}\nChỉ dẫn toàn phim: {}\nHồ sơ dự án: {}\nID mục tiêu: {}\nNgữ cảnh và nội dung: {}",
        language_name(source_language),
        scene.map(|value| value.summary.as_str()).unwrap_or("Không có"),
        scene.map(|value| value.tone.as_str()).unwrap_or("Không có"),
        serde_json::to_string(&previous_vi).unwrap_or_default(),
        serde_json::to_string(&utterance_groups).unwrap_or_default(),
        serde_json::to_string(&guide_brief(pipeline_context)).unwrap_or_default(),
        project_context.unwrap_or("Không có"),
        serde_json::to_string(&target_ids).unwrap_or_default(),
        serde_json::to_string(&context_json).unwrap_or_default()
    )
}

fn editing_prompt(
    source_cues: &[Cue],
    drafts: &[Cue],
    chunk: TranslationChunk,
    style: &str,
    project_context: Option<&str>,
    pipeline_context: &PipelineProjectContext,
) -> String {
    let context_start = chunk.start.saturating_sub(4);
    let context_end = (chunk.end + 4).min(source_cues.len());
    let context = (context_start..context_end)
        .map(|index| {
            serde_json::json!({
                "id": source_cues[index].index,
                "start_ms": source_cues[index].start_ms,
                "end_ms": source_cues[index].end_ms,
                "source": source_cues[index].text,
                "draft_vi": drafts[index].text
            })
        })
        .collect::<Vec<_>>();
    let target_ids = source_cues[chunk.start..chunk.end]
        .iter()
        .map(|cue| cue.index)
        .collect::<Vec<_>>();
    let previous_vi = drafts[..chunk.start]
        .iter()
        .rev()
        .filter(|cue| !cue.text.trim().is_empty())
        .take(8)
        .map(cue_json)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>();
    let scene = scene_for_chunk(source_cues, chunk, pipeline_context);
    let memory = pipeline_context
        .memory_examples
        .iter()
        .map(|example| {
            serde_json::json!({
                "source": example.source,
                "machine": example.machine,
                "preferred": example.preferred
            })
        })
        .collect::<Vec<_>>();
    let direction = match style {
        "modern" => "Thoại hiện đại, đời thường, tự nhiên; tránh văn viết và từ Hán-Việt cứng.",
        "anime" => "Thoại anime giàu nhịp điệu và cảm xúc nhưng không cường điệu vô cớ; giữ hậu tố/tên riêng nhất quán.",
        "historical" => "Thoại cổ trang trang trọng vừa phải, nhất quán vai vế; tránh từ hiện đại lạc bối cảnh.",
        "documentary" => "Lời thuyết minh tài liệu rõ, chính xác, mạch lạc và trung tính.",
        _ => "Tự suy ra thể loại từ ngữ cảnh và dùng thoại điện ảnh trung tính, tự nhiên, dễ nghe toàn quốc.",
    };
    format!(
        "Biên tập toàn cảnh thành lời thoại tiếng Việt để lồng tiếng. {direction} Viết như người Việt đang đối thoại: bỏ chủ ngữ và đại từ lặp khi ngữ cảnh đã rõ, tránh dịch từng chữ, tránh từ Hán-Việt cứng, không giải thích thêm điều nhân vật chỉ ngầm nói. Câu trả lời phải ăn khớp câu trước. Dùng xưng hô nhất quán, giữ nguyên ý nghĩa, tên riêng, số liệu và tag. Chưa rút gọn chỉ vì số ký tự; ưu tiên câu trọn ý và có nhịp nói tự nhiên. Cue lân cận chỉ để hiểu ngữ cảnh, không xuất chúng. Trả đúng JSON theo schema.\nTóm tắt cảnh: {}\nSắc thái cảnh: {}\nTám câu Việt đã chốt trước đó: {}\nVí dụ văn phong người dùng thích: {}\nHồ sơ nhân vật và glossary: {}\nID mục tiêu: {}\nDữ liệu: {}",
        scene.map(|value| value.summary.as_str()).unwrap_or("Không có"),
        scene.map(|value| value.tone.as_str()).unwrap_or("Không có"),
        serde_json::to_string(&previous_vi).unwrap_or_default(),
        serde_json::to_string(&memory).unwrap_or_default(),
        project_context.unwrap_or("Không có"),
        serde_json::to_string(&target_ids).unwrap_or_default(),
        serde_json::to_string(&context).unwrap_or_default()
    )
}

fn continuity_prompt(
    source_cues: &[Cue],
    drafts: &[Cue],
    chunk: TranslationChunk,
    project_context: Option<&str>,
    pipeline_context: &PipelineProjectContext,
) -> String {
    let start = chunk.start.saturating_sub(8);
    let end = (chunk.end + 4).min(source_cues.len());
    let context = (start..end)
        .map(|index| {
            serde_json::json!({
                "id": source_cues[index].index,
                "source": source_cues[index].text,
                "vi": drafts[index].text,
                "target": index >= chunk.start && index < chunk.end
            })
        })
        .collect::<Vec<_>>();
    let target_ids = source_cues[chunk.start..chunk.end]
        .iter()
        .map(|cue| cue.index)
        .collect::<Vec<_>>();
    let scene = scene_for_chunk(source_cues, chunk, pipeline_context);
    format!(
        "Kiểm tra mạch thoại tiếng Việt của cả cảnh. Sửa các cue mục tiêu nếu xưng hô, tên riêng, thuật ngữ, quan hệ nguyên nhân-kết quả hoặc câu hỏi-trả lời chưa khớp; đồng thời loại bỏ lối dịch cứng và chủ ngữ lặp. Không làm văn hoa hơn, không đổi nghĩa, không sửa cue ngoài mục tiêu. Trả đủ mọi ID mục tiêu; cue không cần sửa giữ nguyên text.\nCảnh: {}\nSắc thái: {}\nHồ sơ: {}\nID mục tiêu: {}\nDữ liệu: {}",
        scene.map(|value| value.summary.as_str()).unwrap_or("Không có"),
        scene.map(|value| value.tone.as_str()).unwrap_or("Không có"),
        project_context.unwrap_or("Không có"),
        serde_json::to_string(&target_ids).unwrap_or_default(),
        serde_json::to_string(&context).unwrap_or_default()
    )
}

fn cues_from_map(source: &[Cue], translated: &HashMap<usize, String>) -> Vec<Cue> {
    source
        .iter()
        .map(|cue| Cue {
            index: cue.index,
            start_ms: cue.start_ms,
            end_ms: cue.end_ms,
            text: translated.get(&cue.index).cloned().unwrap_or_default(),
        })
        .collect()
}

fn cue_characters_per_second(cue: &Cue) -> f64 {
    let duration = (cue.end_ms - cue.start_ms).max(250) as f64 / 1_000.0;
    cue.text
        .chars()
        .filter(|character| *character != '\n')
        .count() as f64
        / duration
}

fn number_tokens(text: &str) -> HashSet<String> {
    text.split(|character: char| {
        !character.is_ascii_digit() && character != '.' && character != ','
    })
    .filter(|token| token.chars().any(|character| character.is_ascii_digit()))
    .map(ToString::to_string)
    .collect()
}

pub async fn translate_srt(
    app: &AppHandle,
    input_path: &str,
    source_language: &str,
    quality: &str,
    style: &str,
    project_context: Option<&str>,
) -> Result<TranslationResult, String> {
    if !matches!(quality, "natural" | "fast") {
        return Err("Chế độ dịch phải là natural hoặc fast.".to_string());
    }
    if !matches!(
        style,
        "cinematic-neutral" | "auto" | "modern" | "anime" | "historical" | "documentary"
    ) {
        return Err("Phong cách dịch chưa được hỗ trợ.".to_string());
    }
    let llama_server = executable(app, &["llama-server"]).ok_or_else(|| {
        "Chưa tìm thấy llama-server. Hãy cài lại bản app có engine dịch mới.".to_string()
    })?;
    let hy_model = models::model_path(app, "hy-mt2-7b-q4", "Hy-MT2-7B-Q4_K_M.gguf")?;
    if !hy_model.exists() {
        return Err("Chưa tải Hy-MT2 7B trong Model Manager.".to_string());
    }
    let qwen14 = models::model_path(app, "qwen3-14b-q4", "Qwen3-14B-Q4_K_M.gguf")?;
    let qwen_model = qwen14;
    if quality == "natural" && !qwen_model.exists() {
        return Err(
            "Chế độ Tự nhiên cần Qwen3 14B. Hãy tải model trong Model Manager.".to_string(),
        );
    }

    let source = srt::read(Path::new(input_path), source_language)?;
    if source.cues.is_empty() {
        return Err("File SRT không có cue để dịch.".to_string());
    }
    let pipeline_context = project_context
        .and_then(|value| serde_json::from_str::<PipelineProjectContext>(value).ok())
        .unwrap_or_default();
    let compact_context = compact_prompt_context(project_context);
    let prompt_context = compact_context.as_deref().or(project_context);
    let chunks = if quality == "natural" && !pipeline_context.translation_guide.scenes.is_empty() {
        guided_scene_chunks(&source.cues, &pipeline_context)
    } else {
        translation_chunks(&source.cues)
    };
    if chunks.is_empty() {
        return Err("Không tìm thấy cảnh phù hợp để dịch.".into());
    }
    let processed_ids = chunks
        .iter()
        .flat_map(|chunk| {
            source.cues[chunk.start..chunk.end]
                .iter()
                .map(|cue| cue.index)
        })
        .collect::<HashSet<_>>();
    emit_translation_progress(
        app,
        "loading-translator",
        0,
        chunks.len(),
        2.0,
        "Đang nạp Hy-MT2...",
    );
    let hy_server = LocalLlamaServer::start(&llama_server, &hy_model, false).await?;
    let mut translated = pipeline_context
        .accepted_translations
        .iter()
        .map(|item| (item.id, item.text.clone()))
        .collect::<HashMap<_, _>>();
    for (position, chunk) in chunks.iter().copied().enumerate() {
        emit_translation_progress(
            app,
            "translating",
            position + 1,
            chunks.len(),
            5.0 + (position as f64 / chunks.len() as f64)
                * if quality == "natural" { 40.0 } else { 82.0 },
            format!("Đang dịch nghĩa cụm {}/{}", position + 1, chunks.len()),
        );
        let targets = &source.cues[chunk.start..chunk.end];
        let ids = targets.iter().map(|cue| cue.index).collect::<Vec<_>>();
        let result = chat_with_retry(
            &hy_server,
            "Bạn là hệ thống dịch phụ đề chuyên nghiệp. Chỉ trả dữ liệu JSON theo schema.",
            &hy_translation_prompt(
                &source.cues,
                chunk,
                source_language,
                prompt_context,
                &pipeline_context,
                &translated,
            ),
            &ids,
            (0.7, 0.6, 20, 1.05),
        )
        .await
        .map_err(|error| {
            format!(
                "Hy-MT2 không dịch được cụm {}/{}: {error}",
                position + 1,
                chunks.len()
            )
        })?;
        translated.extend(result);
    }
    hy_server.stop().await;

    let mut drafts = cues_from_map(&source.cues, &translated);
    let mut warnings = Vec::new();
    let mut edited_cues = 0;
    let mut fallback_cues = 0;
    let mut editor = None;

    if quality == "natural" {
        let editing_chunks = chunks.clone();
        emit_translation_progress(
            app,
            "loading-editor",
            0,
            editing_chunks.len(),
            50.0,
            "Đang nạp Qwen3 để biên tập lời thoại...",
        );
        editor = Some(LocalLlamaServer::start(&llama_server, &qwen_model, true).await?);
        for (position, chunk) in editing_chunks.iter().copied().enumerate() {
            emit_translation_progress(
                app,
                "editing",
                position + 1,
                editing_chunks.len(),
                53.0 + (position as f64 / editing_chunks.len() as f64) * 36.0,
                format!(
                    "Đang biên tập cảnh {}/{}",
                    position + 1,
                    editing_chunks.len()
                ),
            );
            let ids = source.cues[chunk.start..chunk.end]
                .iter()
                .map(|cue| cue.index)
                .collect::<Vec<_>>();
            match chat_with_retry(
                editor.as_ref().expect("editor server exists"),
                "Bạn là biên tập viên thoại phim tiếng Việt. Không giải thích, chỉ trả JSON theo schema.",
                &editing_prompt(
                    &source.cues,
                    &drafts,
                    chunk,
                    style,
                    prompt_context,
                    &pipeline_context,
                ),
                &ids,
                (0.2, 0.8, 20, 1.05),
            )
            .await
            {
                Ok(edited) => {
                    for cue in &mut drafts[chunk.start..chunk.end] {
                        if let Some(text) = edited.get(&cue.index) {
                            cue.text.clone_from(text);
                            edited_cues += 1;
                        }
                    }
                    match chat_with_retry(
                        editor.as_ref().expect("editor server exists"),
                        "Bạn là kiểm định viên thoại phim tiếng Việt. Không giải thích, chỉ trả JSON theo schema.",
                        &continuity_prompt(
                            &source.cues,
                            &drafts,
                            chunk,
                            prompt_context,
                            &pipeline_context,
                        ),
                        &ids,
                        (0.12, 0.72, 20, 1.05),
                    )
                    .await
                    {
                        Ok(reviewed) => {
                            for cue in &mut drafts[chunk.start..chunk.end] {
                                if let Some(text) = reviewed.get(&cue.index) {
                                    cue.text.clone_from(text);
                                }
                            }
                        }
                        Err(error) => warnings.push(TranslationWarning {
                            cue_index: ids.first().copied(),
                            code: "continuity_fallback".to_string(),
                            message: format!(
                                "Cảnh bắt đầu ở cue {} giữ bản biên tập vì lượt kiểm tra mạch thoại lỗi: {error}",
                                ids[0]
                            ),
                        }),
                    }
                }
                Err(error) => {
                    fallback_cues += ids.len();
                    warnings.push(TranslationWarning {
                        cue_index: ids.first().copied(),
                        code: "editor_fallback".to_string(),
                        message: format!("Cụm bắt đầu ở cue {} dùng bản Hy-MT2 vì Qwen lỗi: {error}", ids[0]),
                    });
                }
            }
        }
    }

    emit_translation_progress(
        app,
        "validating",
        0,
        source.cues.len(),
        92.0,
        "Đang kiểm tra độ dài và số liệu...",
    );
    if let Some(server) = editor {
        server.stop().await;
    }

    for (source_cue, cue) in source.cues.iter().zip(&drafts) {
        if cue.text.trim().is_empty() || !processed_ids.contains(&cue.index) {
            continue;
        }
        let cps = cue_characters_per_second(cue);
        if cps > 18.0 {
            warnings.push(TranslationWarning {
                cue_index: Some(cue.index),
                code: "too_long".to_string(),
                message: format!(
                    "Cue {} còn dài ({cps:.1} ký tự/giây), nên kiểm tra trước khi lồng tiếng.",
                    cue.index
                ),
            });
        }
        let source_numbers = number_tokens(&source_cue.text);
        let translated_numbers = number_tokens(&cue.text);
        if !source_numbers.is_empty() && source_numbers != translated_numbers {
            warnings.push(TranslationWarning {
                cue_index: Some(cue.index),
                code: "number_mismatch".to_string(),
                message: format!("Cue {} có số liệu khác câu nguồn.", cue.index),
            });
        }
    }

    let work = workspace_dir(app)?;
    let output_path = work.join("translated_vi.srt");
    srt::write(&output_path, &drafts)?;
    let output = srt::read(&output_path, "vi")?;
    emit_translation_progress(
        app,
        "complete",
        source.cues.len(),
        source.cues.len(),
        100.0,
        "Dịch hoàn tất",
    );
    let _ = diagnostics::append(
        app,
        diagnostics::entry(
            "info",
            "translation.complete",
            format!("Đã dịch {} cue ở chế độ {quality}", source.cues.len()),
            serde_json::to_string_pretty(&serde_json::json!({
                "editedCues": edited_cues,
                "fallbackCues": fallback_cues,
                "warnings": &warnings
            }))
            .ok(),
        ),
    );
    Ok(TranslationResult {
        output,
        warnings,
        edited_cues,
        fallback_cues,
        quality: quality.to_string(),
    })
}

fn linear_fit(pairs: &[(f64, f64)]) -> (f64, f64) {
    let n = pairs.len() as f64;
    let mean_x = pairs.iter().map(|pair| pair.0).sum::<f64>() / n;
    let mean_y = pairs.iter().map(|pair| pair.1).sum::<f64>() / n;
    let denominator = pairs
        .iter()
        .map(|pair| (pair.0 - mean_x).powi(2))
        .sum::<f64>();
    if denominator.abs() < f64::EPSILON {
        return (mean_y - mean_x, 1.0);
    }
    let slope = pairs
        .iter()
        .map(|pair| (pair.0 - mean_x) * (pair.1 - mean_y))
        .sum::<f64>()
        / denominator;
    (mean_y - slope * mean_x, slope)
}

fn stable_timeline_fit(pairs: &[(f64, f64)]) -> (f64, f64) {
    let (offset, slope) = linear_fit(pairs);
    const PAL_TO_FILM: f64 = 25.0 / 23.976;
    const FILM_TO_PAL: f64 = 23.976 / 25.0;
    let is_small_drift = (slope - 1.0).abs() <= 0.015;
    let is_frame_rate_conversion =
        (slope - PAL_TO_FILM).abs() <= 0.006 || (slope - FILM_TO_PAL).abs() <= 0.006;
    if is_small_drift || is_frame_rate_conversion {
        return (offset, slope);
    }

    let mut offsets: Vec<f64> = pairs.iter().map(|(x, y)| y - x).collect();
    (median(&mut offsets), 1.0)
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(|a, b| a.total_cmp(b));
    values[values.len() / 2]
}

#[cfg(test)]
fn align(reference: &[Cue], candidate: &[Cue]) -> Result<(Vec<Cue>, i64, f64, usize, f64), String> {
    if reference.len() < 3 || candidate.len() < 3 {
        return Err("Cần ít nhất 3 cue trong mỗi file để đồng bộ.".to_string());
    }
    let sample_count = reference.len().min(candidate.len()).min(120);
    let mut pairs = Vec::with_capacity(sample_count);
    for position in 0..sample_count {
        let ratio = position as f64 / (sample_count - 1) as f64;
        let ref_index = (ratio * (reference.len() - 1) as f64).round() as usize;
        let candidate_index = (ratio * (candidate.len() - 1) as f64).round() as usize;
        pairs.push((
            candidate[candidate_index].start_ms as f64,
            reference[ref_index].start_ms as f64,
        ));
    }
    let first_fit = linear_fit(&pairs);
    let mut residuals: Vec<f64> = pairs
        .iter()
        .map(|(x, y)| (y - (first_fit.0 + first_fit.1 * x)).abs())
        .collect();
    let median_residual = median(&mut residuals);
    let threshold = (median_residual * 3.0).max(1_500.0);
    let filtered: Vec<(f64, f64)> = pairs
        .into_iter()
        .filter(|(x, y)| (y - (first_fit.0 + first_fit.1 * x)).abs() <= threshold)
        .collect();
    let (offset, slope) = linear_fit(&filtered);
    if !(0.90..=1.10).contains(&slope) {
        return Err(format!(
            "Hai file có vẻ không cùng bản phim (drift {:.2}%).",
            slope * 100.0
        ));
    }
    let transformed = candidate
        .iter()
        .enumerate()
        .map(|(index, cue)| Cue {
            index: index + 1,
            start_ms: (offset + slope * cue.start_ms as f64).round().max(0.0) as i64,
            end_ms: (offset + slope * cue.end_ms as f64).round().max(1.0) as i64,
            text: cue.text.clone(),
        })
        .collect();
    let count_balance =
        reference.len().min(candidate.len()) as f64 / reference.len().max(candidate.len()) as f64;
    let confidence = (count_balance * (1.0 - median_residual / 8_000.0)).clamp(0.0, 1.0);
    Ok((
        transformed,
        offset.round() as i64,
        slope,
        filtered.len(),
        confidence,
    ))
}

#[derive(Clone, Debug)]
struct TextMatch {
    reference_index: usize,
    candidate_index: usize,
    reference_ms: f64,
    candidate_ms: f64,
    score: f64,
}

impl TextMatch {
    fn offset_ms(&self) -> f64 {
        self.reference_ms - self.candidate_ms
    }
}

fn align_by_text(
    reference: &[Cue],
    candidate: &[Cue],
) -> Result<(Vec<Cue>, i64, f64, usize, f64), String> {
    if reference.len() < 3 {
        return Err("Cần ít nhất 3 câu trong phụ đề 5 phút đã dịch để căn thời gian.".into());
    }
    if candidate.len() < 3 {
        return Err("File SRT tiếng Việt đầy đủ có quá ít câu để căn thời gian.".into());
    }

    let normalized_reference: Vec<String> = reference
        .iter()
        .map(|cue| normalize_subtitle_text(&cue.text))
        .collect();
    let normalized_candidate: Vec<String> = candidate
        .iter()
        .map(|cue| normalize_subtitle_text(&cue.text))
        .collect();
    let searchable_reference_count = normalized_reference
        .iter()
        .filter(|text| text.chars().count() >= 6)
        .count();

    let mut possible_matches = Vec::new();
    for (reference_index, reference_text) in normalized_reference.iter().enumerate() {
        if reference_text.chars().count() < 6 {
            continue;
        }

        let threshold = if reference_text.split_whitespace().count() < 3 {
            0.66
        } else {
            0.28
        };
        let mut best_for_cue = Vec::new();
        for (candidate_index, candidate_text) in normalized_candidate.iter().enumerate() {
            if candidate_text.is_empty() {
                continue;
            }

            let mut score = text_similarity(reference_text, candidate_text);
            if let Some(next) = normalized_candidate.get(candidate_index + 1) {
                let joined = format!("{candidate_text} {next}");
                score = score.max(text_similarity(reference_text, &joined));
            }

            if score >= threshold {
                best_for_cue.push(TextMatch {
                    reference_index,
                    candidate_index,
                    reference_ms: reference[reference_index].start_ms as f64,
                    candidate_ms: candidate[candidate_index].start_ms as f64,
                    score,
                });
            }
        }

        best_for_cue.sort_by(|left, right| right.score.total_cmp(&left.score));
        possible_matches.extend(best_for_cue.into_iter().take(4));
    }

    let coherent_matches = best_monotonic_cluster(&possible_matches);
    if coherent_matches.len() < 3 {
        return Err(format!(
            "Chỉ tìm thấy {} câu Việt tương đồng theo đúng thứ tự. Hãy kiểm tra file SRT tải về có đúng phim/bản phát hành và bản dịch 5 phút có đủ sát nghĩa không.",
            coherent_matches.len()
        ));
    }

    let first_pairs: Vec<(f64, f64)> = coherent_matches
        .iter()
        .map(|item| (item.candidate_ms, item.reference_ms))
        .collect();
    let (first_offset, first_slope) = linear_fit(&first_pairs);
    let mut residuals: Vec<f64> = coherent_matches
        .iter()
        .map(|item| (item.reference_ms - (first_offset + first_slope * item.candidate_ms)).abs())
        .collect();
    let median_residual = median(&mut residuals);
    let residual_limit = (median_residual * 3.0).clamp(1_500.0, 4_000.0);
    let filtered_matches: Vec<TextMatch> = coherent_matches
        .into_iter()
        .filter(|item| {
            (item.reference_ms - (first_offset + first_slope * item.candidate_ms)).abs()
                <= residual_limit
        })
        .collect();

    if filtered_matches.len() < 3 {
        return Err("Các câu giống nhau không tạo thành một trục thời gian ổn định. Có thể hai file thuộc hai bản phim khác nhau.".into());
    }

    let pairs: Vec<(f64, f64)> = filtered_matches
        .iter()
        .map(|item| (item.candidate_ms, item.reference_ms))
        .collect();
    let (offset, slope) = stable_timeline_fit(&pairs);
    if !(0.90..=1.10).contains(&slope) {
        return Err(format!(
            "Độ lệch tốc độ {:.2}% quá lớn; có thể file SRT không cùng bản phim.",
            (slope - 1.0) * 100.0
        ));
    }

    let transformed = candidate
        .iter()
        .enumerate()
        .map(|(index, cue)| {
            let start_ms = (offset + slope * cue.start_ms as f64).round().max(0.0) as i64;
            let end_ms = (offset + slope * cue.end_ms as f64).round().max(0.0) as i64;
            Cue {
                index: index + 1,
                start_ms,
                end_ms: end_ms.max(start_ms + 1),
                text: cue.text.clone(),
            }
        })
        .collect();

    let average_score =
        filtered_matches.iter().map(|item| item.score).sum::<f64>() / filtered_matches.len() as f64;
    let average_residual = filtered_matches
        .iter()
        .map(|item| (item.reference_ms - (offset + slope * item.candidate_ms)).abs())
        .sum::<f64>()
        / filtered_matches.len() as f64;
    let coverage = filtered_matches.len() as f64 / searchable_reference_count.max(1) as f64;
    let residual_quality = (1.0 - average_residual / 4_000.0).clamp(0.0, 1.0);
    let confidence =
        (coverage.min(1.0) * 0.55 + average_score * 0.35 + residual_quality * 0.10).clamp(0.0, 1.0);

    Ok((
        transformed,
        offset.round() as i64,
        slope,
        filtered_matches.len(),
        confidence,
    ))
}

fn best_monotonic_cluster(possible_matches: &[TextMatch]) -> Vec<TextMatch> {
    const OFFSET_WINDOW_MS: f64 = 8_000.0;
    let mut best_path = Vec::new();
    let mut best_score = 0.0;

    for seed in possible_matches {
        let mut nodes: Vec<TextMatch> = possible_matches
            .iter()
            .filter(|item| (item.offset_ms() - seed.offset_ms()).abs() <= OFFSET_WINDOW_MS)
            .cloned()
            .collect();
        nodes.sort_by_key(|item| (item.reference_index, item.candidate_index));
        if nodes.len() < 3 {
            continue;
        }

        let mut path_lengths = vec![1usize; nodes.len()];
        let mut path_scores: Vec<f64> = nodes.iter().map(|item| item.score).collect();
        let mut previous = vec![None; nodes.len()];

        for current in 0..nodes.len() {
            for earlier in 0..current {
                if nodes[earlier].reference_index >= nodes[current].reference_index
                    || nodes[earlier].candidate_index >= nodes[current].candidate_index
                {
                    continue;
                }
                let next_length = path_lengths[earlier] + 1;
                let next_score = path_scores[earlier] + nodes[current].score;
                if next_length > path_lengths[current]
                    || (next_length == path_lengths[current] && next_score > path_scores[current])
                {
                    path_lengths[current] = next_length;
                    path_scores[current] = next_score;
                    previous[current] = Some(earlier);
                }
            }
        }

        let Some((mut cursor, _)) = path_lengths.iter().enumerate().max_by(
            |(left_index, left_length), (right_index, right_length)| {
                left_length
                    .cmp(right_length)
                    .then_with(|| path_scores[*left_index].total_cmp(&path_scores[*right_index]))
            },
        ) else {
            continue;
        };

        let mut path = Vec::new();
        loop {
            path.push(nodes[cursor].clone());
            match previous[cursor] {
                Some(index) => cursor = index,
                None => break,
            }
        }
        path.reverse();
        let path_score = path.iter().map(|item| item.score).sum::<f64>();
        if path.len() > best_path.len()
            || (path.len() == best_path.len() && path_score > best_score)
        {
            best_score = path_score;
            best_path = path;
        }
    }

    best_path
}

fn normalize_subtitle_text(text: &str) -> String {
    let folded = text.replace('đ', "d").replace('Đ', "D");
    let without_marks: String = folded
        .nfd()
        .filter(|character| !is_combining_mark(*character))
        .collect();
    let mut normalized = String::new();
    let mut previous_was_space = true;

    for character in without_marks.to_lowercase().chars() {
        if character.is_alphanumeric() {
            normalized.push(character);
            previous_was_space = false;
        } else if !previous_was_space {
            normalized.push(' ');
            previous_was_space = true;
        }
    }

    normalized.trim().to_string()
}

fn text_similarity(left: &str, right: &str) -> f64 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    if left == right {
        return 1.0;
    }

    let left_words: HashSet<&str> = left.split_whitespace().collect();
    let right_words: HashSet<&str> = right.split_whitespace().collect();
    let word_intersection = left_words.intersection(&right_words).count() as f64;
    let word_union = left_words.union(&right_words).count().max(1) as f64;
    let word_jaccard = word_intersection / word_union;

    let left_trigrams = character_ngrams(left, 3);
    let right_trigrams = character_ngrams(right, 3);
    let trigram_intersection = left_trigrams.intersection(&right_trigrams).count() as f64;
    let trigram_dice =
        2.0 * trigram_intersection / (left_trigrams.len() + right_trigrams.len()).max(1) as f64;

    let containment = if left.contains(right) || right.contains(left) {
        let short = left.chars().count().min(right.chars().count()) as f64;
        let long = left.chars().count().max(right.chars().count()) as f64;
        short / long
    } else {
        0.0
    };

    (word_jaccard * 0.55 + trigram_dice * 0.45).max(containment * 0.85)
}

fn character_ngrams(text: &str, size: usize) -> HashSet<String> {
    let characters: Vec<char> = text
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect();
    if characters.len() <= size {
        return HashSet::from([characters.into_iter().collect()]);
    }
    characters
        .windows(size)
        .map(|window| window.iter().collect())
        .collect()
}

pub fn sync_subtitles(
    app: &AppHandle,
    reference_path: &str,
    candidate_path: &str,
) -> Result<SyncResult, String> {
    let reference = srt::read(Path::new(reference_path), "vi")?;
    let candidate = srt::read(Path::new(candidate_path), "vi")?;
    let (cues, offset_ms, speed_ratio, matched_cues, confidence) =
        align_by_text(&reference.cues, &candidate.cues)?;
    let work = workspace_dir(app)?;
    let output_path = work.join("synced_vi.srt");
    srt::write(&output_path, &cues)?;
    Ok(SyncResult {
        output: srt::read(&output_path, "vi")?,
        offset_ms,
        speed_ratio,
        matched_cues,
        confidence,
    })
}

fn emit_dub_progress(
    app: &AppHandle,
    phase: &str,
    current: usize,
    total: usize,
    message: impl Into<String>,
) {
    let percent = if total == 0 {
        0.0
    } else {
        current as f64 / total as f64 * 100.0
    };
    let _ = app.emit(
        "dub-progress",
        DubProgress {
            phase: phase.to_string(),
            current,
            total,
            percent: percent.clamp(0.0, 100.0),
            message: message.into(),
        },
    );
}

fn speech_text(text: &str) -> String {
    let mut clean = String::new();
    let mut inside_tag = false;
    for character in text.chars() {
        match character {
            '<' => inside_tag = true,
            '>' => inside_tag = false,
            '\n' | '\r' if !inside_tag => clean.push(' '),
            _ if !inside_tag => clean.push(character),
            _ => {}
        }
    }
    clean.split_whitespace().collect::<Vec<_>>().join(" ")
}

async fn media_duration_seconds(app: &AppHandle, path: &Path) -> Result<f64, String> {
    let ffprobe = executable(app, &["ffprobe"])
        .ok_or_else(|| "Không tìm thấy ffprobe để đo audio TTS.".to_string())?;
    let result = output(
        Command::new(ffprobe)
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "default=noprint_wrappers=1:nokey=1",
            ])
            .arg(path),
        "ffprobe audio TTS",
    )
    .await?;
    String::from_utf8_lossy(&result.stdout)
        .trim()
        .parse::<f64>()
        .map_err(|error| format!("Không đọc được thời lượng audio TTS: {error}"))
}

fn write_zero_samples(
    output: &mut BufWriter<std::fs::File>,
    sample_count: u64,
) -> Result<(), String> {
    static ZERO_CHUNK: [u8; 65_536] = [0; 65_536];
    let mut bytes_remaining = sample_count.saturating_mul(2);
    while bytes_remaining > 0 {
        let count = bytes_remaining.min(ZERO_CHUNK.len() as u64) as usize;
        output
            .write_all(&ZERO_CHUNK[..count])
            .map_err(|error| error.to_string())?;
        bytes_remaining -= count as u64;
    }
    Ok(())
}

async fn render_narration(
    app: &AppHandle,
    cues: &[Cue],
    duration_ms: i64,
    work: &Path,
) -> Result<(PathBuf, usize), String> {
    const SAMPLE_RATE: u64 = 48_000;
    let say = Path::new("/usr/bin/say");
    if !say.exists() {
        return Err("Không tìm thấy giọng đọc hệ thống macOS (/usr/bin/say).".into());
    }
    let ffmpeg = executable(app, &["ffmpeg"])
        .ok_or_else(|| "Không tìm thấy FFmpeg để xử lý giọng đọc.".to_string())?;
    let selected: Vec<&Cue> = cues
        .iter()
        .filter(|cue| cue.start_ms < duration_ms && cue.end_ms > 0)
        .collect();
    if selected.is_empty() {
        return Err("Không có câu phụ đề nào trong khoảng video cần lồng tiếng.".into());
    }

    let narration_path = work.join("narration-48k-mono.pcm");
    let file = std::fs::File::create(&narration_path).map_err(|error| error.to_string())?;
    let mut narration = BufWriter::new(file);
    let mut written_samples = 0_u64;
    let mut rendered_cues = 0_usize;

    for (position, cue) in selected.iter().enumerate() {
        let text = speech_text(&cue.text);
        if text.is_empty() {
            continue;
        }
        emit_dub_progress(
            app,
            "tts",
            position,
            selected.len(),
            format!("Đang tạo giọng câu {}/{}", position + 1, selected.len()),
        );

        let aiff_path = work.join(format!("cue-{position:05}.aiff"));
        let wav_path = work.join(format!("cue-{position:05}.wav"));
        output(
            Command::new(say)
                .args(["-v", "Linh", "-r", "190", "-o"])
                .arg(&aiff_path)
                .arg(&text),
            "macOS TTS",
        )
        .await?;

        let source_duration = media_duration_seconds(app, &aiff_path).await?;
        let cue_start_ms = cue.start_ms.max(0);
        let cue_end_ms = cue.end_ms.min(duration_ms).max(cue_start_ms + 1);
        let target_duration = (cue_end_ms - cue_start_ms) as f64 / 1_000.0;
        let tempo = (source_duration / target_duration).clamp(0.85, 1.18);
        let audio_filter =
            format!("aresample=48000,atempo={tempo:.5},apad,atrim=0:{target_duration:.5}");
        output(
            Command::new(&ffmpeg)
                .args(["-y", "-loglevel", "error", "-i"])
                .arg(&aiff_path)
                .args(["-af", &audio_filter, "-ac", "1", "-c:a", "pcm_s16le"])
                .arg(&wav_path),
            "FFmpeg căn thời lượng câu thoại",
        )
        .await?;

        let start_sample = cue_start_ms as u64 * SAMPLE_RATE / 1_000;
        let end_sample = cue_end_ms as u64 * SAMPLE_RATE / 1_000;
        if start_sample > written_samples {
            write_zero_samples(&mut narration, start_sample - written_samples)?;
            written_samples = start_sample;
        }

        let skip_samples = written_samples.saturating_sub(start_sample);
        let available_samples = end_sample.saturating_sub(written_samples);
        let mut reader = hound::WavReader::open(&wav_path).map_err(|error| error.to_string())?;
        let mut bytes = Vec::with_capacity((available_samples * 2) as usize);
        for sample in reader
            .samples::<i16>()
            .skip(skip_samples as usize)
            .take(available_samples as usize)
        {
            bytes.extend_from_slice(&sample.map_err(|error| error.to_string())?.to_le_bytes());
        }
        narration
            .write_all(&bytes)
            .map_err(|error| error.to_string())?;
        let samples_written = (bytes.len() / 2) as u64;
        written_samples += samples_written;
        if written_samples < end_sample {
            write_zero_samples(&mut narration, end_sample - written_samples)?;
            written_samples = end_sample;
        }
        rendered_cues += 1;
        let _ = std::fs::remove_file(aiff_path);
        let _ = std::fs::remove_file(wav_path);
    }

    let total_samples = duration_ms.max(1) as u64 * SAMPLE_RATE / 1_000;
    if written_samples < total_samples {
        write_zero_samples(&mut narration, total_samples - written_samples)?;
    }
    narration.flush().map_err(|error| error.to_string())?;
    Ok((narration_path, rendered_cues))
}

pub async fn render_dubbed_video(
    app: &AppHandle,
    video_path: &str,
    subtitle_path: &str,
    output_path: Option<&str>,
    max_seconds: Option<u64>,
) -> Result<DubResult, String> {
    let media = inspect_media(app, video_path).await?;
    let subtitles = srt::read(Path::new(subtitle_path), "vi")?;
    let duration_ms = max_seconds
        .map(|seconds| media.duration_ms.min(seconds as i64 * 1_000))
        .unwrap_or(media.duration_ms)
        .max(1);
    let work = workspace_dir(app)?;
    let (narration_path, rendered_cues) =
        render_narration(app, &subtitles.cues, duration_ms, &work).await?;
    emit_dub_progress(app, "mixing", 0, 1, "Đang ghép giọng vào video");

    let is_test = max_seconds.is_some();
    let destination = output_path
        .map(PathBuf::from)
        .unwrap_or_else(|| work.join("long-tieng-test-5-phut.mp4"));
    let ffmpeg = executable(app, &["ffmpeg"])
        .ok_or_else(|| "Không tìm thấy FFmpeg để xuất video.".to_string())?;
    let mut command = Command::new(ffmpeg);
    command
        .args([
            "-y", "-i", video_path, "-f", "s16le", "-ar", "48000", "-ac", "1", "-i",
        ])
        .arg(&narration_path);
    let has_original_audio = media.audio_codec.is_some();
    if has_original_audio {
        command.args([
            "-filter_complex",
            "[0:a:0]volume=0.24[bg];[1:a:0]volume=1.20[voice];[bg][voice]amix=inputs=2:duration=first:normalize=0[mixed]",
            "-map",
            "0:v:0",
            "-map",
            "[mixed]",
        ]);
    } else {
        command.args(["-map", "0:v:0", "-map", "1:a:0"]);
    }
    if is_test {
        command
            .args(["-t", &(duration_ms as f64 / 1_000.0).to_string()])
            .args([
                "-vf",
                "scale='min(854,iw)':-2",
                "-c:v",
                "mpeg4",
                "-q:v",
                "12",
            ]);
    } else {
        command.args(["-c:v", "copy"]);
    }
    command.args(["-c:a", "aac", "-b:a", "192k"]);
    if destination
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "mp4" | "m4v" | "mov"
            )
        })
    {
        command.args(["-movflags", "+faststart"]);
    }
    command.arg(&destination);
    let result = output(&mut command, "FFmpeg xuất video lồng tiếng").await;
    let _ = std::fs::remove_file(&narration_path);
    let _ = std::fs::remove_dir(&work);
    result?;

    emit_dub_progress(app, "complete", 1, 1, "Đã xuất video lồng tiếng");
    let _ = diagnostics::append(
        app,
        diagnostics::entry(
            "info",
            "dub.complete",
            format!("Đã lồng {rendered_cues} câu bằng giọng Linh (macOS)"),
            Some(destination.to_string_lossy().into_owned()),
        ),
    );
    Ok(DubResult {
        path: destination.to_string_lossy().into_owned(),
        rendered_cues,
        duration_ms,
        voice: "Linh (macOS)".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::speech_text;

    #[test]
    fn cleans_subtitle_markup_for_speech() {
        assert_eq!(
            speech_text("<i>Xin chào</i>\nViệt Nam"),
            "Xin chào Việt Nam"
        );
    }

    use super::*;

    #[test]
    fn translation_chunks_limit_cues_and_include_overlap_context() {
        let source = (0..50)
            .map(|index| Cue {
                index: index + 1,
                start_ms: index as i64 * 1_000,
                end_ms: index as i64 * 1_000 + 900,
                text: "Một câu thoại ngắn".to_string(),
            })
            .collect::<Vec<_>>();
        let chunks = translation_chunks(&source);
        assert_eq!(
            chunks,
            vec![
                TranslationChunk { start: 0, end: 24 },
                TranslationChunk { start: 24, end: 48 },
                TranslationChunk { start: 48, end: 50 },
            ]
        );
        let (context, targets) = chunk_context(&source, chunks[1]);
        assert_eq!(targets.first().unwrap().index, 25);
        assert_eq!(targets.last().unwrap().index, 48);
        assert_eq!(context.first().unwrap().index, 21);
        assert_eq!(context.last().unwrap().index, 50);
    }

    #[test]
    fn translation_chunks_honor_estimated_token_limit() {
        let source = (0..10)
            .map(|index| Cue {
                index: index + 1,
                start_ms: index as i64 * 1_000,
                end_ms: index as i64 * 1_000 + 900,
                text: "a".repeat(4_000),
            })
            .collect::<Vec<_>>();
        assert_eq!(translation_chunks(&source).len(), 10);
    }

    #[test]
    fn semantic_scene_chunks_limit_duration_and_cue_count() {
        let source = (0..130)
            .map(|index| Cue {
                index: index + 1,
                start_ms: index as i64 * 2_000,
                end_ms: index as i64 * 2_000 + 1_500,
                text: format!("Cue {index}"),
            })
            .collect::<Vec<_>>();
        let chunks = semantic_scene_chunks(&source);
        assert!(chunks.iter().all(|chunk| chunk.end - chunk.start <= 48));
        assert!(chunks.iter().all(|chunk| {
            source[chunk.end - 1].end_ms - source[chunk.start].start_ms <= 120_000
        }));
        assert_eq!(chunks.last().unwrap().end, source.len());
    }

    #[test]
    fn semantic_scene_chunks_split_on_long_gap() {
        let source = vec![
            Cue {
                index: 1,
                start_ms: 0,
                end_ms: 800,
                text: "Một".into(),
            },
            Cue {
                index: 2,
                start_ms: 900,
                end_ms: 1_700,
                text: "Hai".into(),
            },
            Cue {
                index: 3,
                start_ms: 1_800,
                end_ms: 2_600,
                text: "Ba".into(),
            },
            Cue {
                index: 4,
                start_ms: 2_700,
                end_ms: 3_500,
                text: "Bốn".into(),
            },
            Cue {
                index: 5,
                start_ms: 7_000,
                end_ms: 7_800,
                text: "Cảnh mới".into(),
            },
        ];
        let chunks = semantic_scene_chunks(&source);
        assert_eq!(
            chunks,
            vec![
                TranslationChunk { start: 0, end: 4 },
                TranslationChunk { start: 4, end: 5 }
            ]
        );
    }

    #[test]
    fn utterance_groups_join_continuation_cues() {
        let source = vec![
            Cue {
                index: 1,
                start_ms: 0,
                end_ms: 700,
                text: "Tôi nghĩ".into(),
            },
            Cue {
                index: 2,
                start_ms: 750,
                end_ms: 1_400,
                text: "chúng ta nên đi.".into(),
            },
            Cue {
                index: 3,
                start_ms: 1_500,
                end_ms: 2_100,
                text: "Được thôi!".into(),
            },
        ];
        let groups = utterance_groups(&source);
        assert_eq!(groups[0], groups[1]);
        assert_ne!(groups[1], groups[2]);
    }

    #[test]
    fn guided_scene_chunks_can_select_one_scene() {
        let source = (1..=6)
            .map(|index| Cue {
                index,
                start_ms: index as i64 * 1_000,
                end_ms: index as i64 * 1_000 + 800,
                text: format!("Cue {index}"),
            })
            .collect::<Vec<_>>();
        let context = PipelineProjectContext {
            translation_guide: PipelineTranslationGuide {
                scenes: vec![
                    PipelineSceneContext {
                        id: "scene-1".into(),
                        start_index: 1,
                        end_index: 3,
                        ..Default::default()
                    },
                    PipelineSceneContext {
                        id: "scene-2".into(),
                        start_index: 4,
                        end_index: 6,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            only_scene_id: Some("scene-2".into()),
            ..Default::default()
        };
        assert_eq!(
            guided_scene_chunks(&source, &context),
            vec![TranslationChunk { start: 3, end: 6 }]
        );
    }

    #[test]
    fn parses_exact_translation_ids_and_preserves_markup() {
        let raw = r#"```json
{"translations":[{"id":7,"text":"<i>Xin chào</i>\nViệt Nam"},{"id":8,"text":"Tám giờ"}]}
```"#;
        let parsed = parse_translation_json(raw, &[7, 8]).unwrap();
        assert_eq!(parsed[&7], "<i>Xin chào</i>\nViệt Nam");
        assert_eq!(parsed[&8], "Tám giờ");
    }

    #[test]
    fn rejects_missing_or_duplicate_translation_ids() {
        let missing = r#"{"translations":[{"id":1,"text":"Một"}]}"#;
        assert!(parse_translation_json(missing, &[1, 2]).is_err());
        let duplicate = r#"{"translations":[{"id":1,"text":"Một"},{"id":1,"text":"Lặp"}]}"#;
        assert!(parse_translation_json(duplicate, &[1]).is_err());
    }

    #[test]
    fn translated_cues_keep_ids_and_timestamps() {
        let source = vec![Cue {
            index: 42,
            start_ms: 12_345,
            end_ms: 14_678,
            text: "Original".to_string(),
        }];
        let translated = HashMap::from([(42, "Bản dịch".to_string())]);
        let result = cues_from_map(&source, &translated);
        assert_eq!(result[0].index, 42);
        assert_eq!(result[0].start_ms, 12_345);
        assert_eq!(result[0].end_ms, 14_678);
        assert_eq!(result[0].text, "Bản dịch");
    }

    fn cues(count: usize, offset: i64, scale: f64) -> Vec<Cue> {
        (0..count)
            .map(|index| {
                let start = ((index as i64 * 2_000 + offset) as f64 * scale) as i64;
                Cue {
                    index: index + 1,
                    start_ms: start,
                    end_ms: start + 1_200,
                    text: format!("Cue {index}"),
                }
            })
            .collect()
    }

    #[test]
    fn aligns_fixed_offset() {
        let reference = cues(20, 0, 1.0);
        let candidate = cues(20, 2_500, 1.0);
        let (result, offset, slope, _, confidence) = align(&reference, &candidate).unwrap();
        assert!((offset + 2500).abs() <= 1);
        assert!((slope - 1.0).abs() < 0.0001);
        assert_eq!(result[5].start_ms, reference[5].start_ms);
        assert!(confidence > 0.9);
    }

    #[test]
    fn aligns_small_drift() {
        let reference = cues(30, 0, 1.0);
        let candidate = cues(30, 1_000, 1.002);
        let (result, _, _, _, _) = align(&reference, &candidate).unwrap();
        assert!((result[25].start_ms - reference[25].start_ms).abs() < 5);
    }

    #[test]
    fn avoids_extrapolating_implausible_preview_drift() {
        let pairs = vec![
            (0.0, 2_000.0),
            (100_000.0, 104_800.0),
            (200_000.0, 207_600.0),
        ];
        let (offset, slope) = stable_timeline_fit(&pairs);
        assert_eq!(slope, 1.0);
        assert_eq!(offset, 4_800.0);
    }

    #[test]
    fn aligns_full_vietnamese_subtitle_from_preview_translation() {
        let dialogue = [
            "Chúng ta phải rời khỏi đây ngay bây giờ.",
            "Anh đã hứa sẽ quay lại trước khi trời tối.",
            "Không ai biết cánh cửa bí mật nằm ở đâu.",
            "Hãy giữ im lặng và đi theo tôi.",
            "Con tàu cuối cùng sẽ khởi hành lúc chín giờ.",
            "Tôi không thể bỏ mặc mọi người ở lại.",
            "Đây là cơ hội duy nhất để cứu ngôi làng.",
            "Ngày mai chúng ta sẽ bắt đầu một cuộc sống mới.",
        ];
        let reference: Vec<Cue> = dialogue
            .iter()
            .enumerate()
            .map(|(index, text)| Cue {
                index: index + 1,
                start_ms: index as i64 * 4_000,
                end_ms: index as i64 * 4_000 + 2_000,
                text: text.replace('.', "!"),
            })
            .collect();
        let mut candidate: Vec<Cue> = dialogue
            .iter()
            .enumerate()
            .map(|(index, text)| Cue {
                index: index + 1,
                start_ms: 2_500 + index as i64 * 4_000,
                end_ms: 4_500 + index as i64 * 4_000,
                text: text.to_string(),
            })
            .collect();
        candidate.extend((8..40).map(|index| Cue {
            index: index + 1,
            start_ms: 2_500 + index as i64 * 4_000,
            end_ms: 4_500 + index as i64 * 4_000,
            text: format!("Câu thoại đầy đủ duy nhất ở phần sau số {index}"),
        }));

        let (result, offset, slope, matched, confidence) =
            align_by_text(&reference, &candidate).unwrap();
        assert_eq!(result.len(), candidate.len());
        assert!((offset + 2_500).abs() <= 1);
        assert!((slope - 1.0).abs() < 0.0001);
        assert_eq!(result[5].start_ms, reference[5].start_ms);
        assert_eq!(matched, dialogue.len());
        assert!(confidence > 0.9);
    }

    #[test]
    fn rejects_unrelated_vietnamese_subtitles() {
        let reference = vec![
            Cue {
                index: 1,
                start_ms: 0,
                end_ms: 1_000,
                text: "Chiếc máy bay đang chuẩn bị hạ cánh".into(),
            },
            Cue {
                index: 2,
                start_ms: 2_000,
                end_ms: 3_000,
                text: "Phi công đã nhìn thấy đường băng".into(),
            },
            Cue {
                index: 3,
                start_ms: 4_000,
                end_ms: 5_000,
                text: "Hành khách vui lòng thắt dây an toàn".into(),
            },
        ];
        let candidate = vec![
            Cue {
                index: 1,
                start_ms: 0,
                end_ms: 1_000,
                text: "Món súp này cần thêm một chút muối".into(),
            },
            Cue {
                index: 2,
                start_ms: 2_000,
                end_ms: 3_000,
                text: "Nhà hàng sẽ đóng cửa vào tối nay".into(),
            },
            Cue {
                index: 3,
                start_ms: 4_000,
                end_ms: 5_000,
                text: "Người đầu bếp đang chuẩn bị bữa sáng".into(),
            },
        ];

        assert!(align_by_text(&reference, &candidate).is_err());
    }

    #[test]
    fn normalizes_vietnamese_for_text_matching() {
        assert_eq!(
            normalize_subtitle_text("Đừng đi, trời đã tối!"),
            "dung di troi da toi"
        );
    }

    #[test]
    fn aligns_human_translation_with_different_wording_and_segmentation() {
        let reference = vec![
            Cue {
                index: 1,
                start_ms: 10_000,
                end_ms: 12_000,
                text: "Tôi muốn xoa dịu nỗi đau của những đứa trẻ này".into(),
            },
            Cue {
                index: 2,
                start_ms: 18_000,
                end_ms: 21_000,
                text: "Tháng này tôi đã nhận được bảy báo cáo về tai nạn do quỷ gây ra".into(),
            },
            Cue {
                index: 3,
                start_ms: 26_000,
                end_ms: 28_000,
                text: "Dù quỷ lấy đi bao nhiêu mạng sống".into(),
            },
            Cue {
                index: 4,
                start_ms: 34_000,
                end_ms: 37_000,
                text: "Con người sẽ đứng dậy và chiến đấu lại".into(),
            },
        ];
        let candidate = vec![
            Cue {
                index: 1,
                start_ms: 13_000,
                end_ms: 14_000,
                text: "Ta muốn xoa dịu nỗi uất hận".into(),
            },
            Cue {
                index: 2,
                start_ms: 14_000,
                end_ms: 15_000,
                text: "của những đứa trẻ quá cố ở đây".into(),
            },
            Cue {
                index: 3,
                start_ms: 21_000,
                end_ms: 24_000,
                text: "Tháng này đã có tới bảy báo cáo về thiệt hại do bọn quỷ gây ra".into(),
            },
            Cue {
                index: 4,
                start_ms: 29_000,
                end_ms: 31_000,
                text: "Dù lũ quỷ có cướp đi bao nhiêu sinh mạng".into(),
            },
            Cue {
                index: 5,
                start_ms: 37_000,
                end_ms: 39_000,
                text: "con người vẫn sẽ vùng dậy chiến đấu".into(),
            },
            Cue {
                index: 6,
                start_ms: 50_000,
                end_ms: 52_000,
                text: "Một câu khác không liên quan ở phần sau".into(),
            },
        ];

        let (_, offset, _, matched, _) = align_by_text(&reference, &candidate).unwrap();
        assert!((offset + 3_000).abs() < 100);
        assert_eq!(matched, 4);
    }

    #[test]
    fn reads_whisper_detected_language() {
        let output = "whisper_full_with_state: auto-detected language: ja (p = 0.570550)";
        assert_eq!(parse_detected_language_text(output).as_deref(), Some("ja"));
    }
}
