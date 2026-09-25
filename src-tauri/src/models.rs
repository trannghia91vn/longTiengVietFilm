use crate::diagnostics;
use futures_util::StreamExt;
use reqwest::{header::RANGE, StatusCode};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

static ACTIVE_DOWNLOADS: LazyLock<Mutex<HashSet<String>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

struct DownloadGuard {
    id: String,
}

impl DownloadGuard {
    fn claim(id: &str) -> Result<Self, String> {
        let mut active = ACTIVE_DOWNLOADS
            .lock()
            .map_err(|_| "Không thể khóa trình tải model.".to_string())?;
        if !active.insert(id.to_string()) {
            return Err("Model này đang được tải hoặc kiểm tra checksum. Vui lòng chờ tác vụ hiện tại hoàn tất.".to_string());
        }
        Ok(Self { id: id.to_string() })
    }
}

impl Drop for DownloadGuard {
    fn drop(&mut self) {
        if let Ok(mut active) = ACTIVE_DOWNLOADS.lock() {
            active.remove(&self.id);
        }
    }
}

#[derive(Clone)]
struct ModelFile {
    relative_path: &'static str,
    url: &'static str,
    size: u64,
    sha256: Option<&'static str>,
}

#[derive(Clone)]
struct ModelSpec {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    provider: &'static str,
    license: &'static str,
    files: Vec<ModelFile>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    id: String,
    name: String,
    description: String,
    provider: String,
    size_bytes: u64,
    license: String,
    state: String,
    installed_bytes: u64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadProgress {
    model_id: String,
    file_name: String,
    downloaded_bytes: u64,
    total_bytes: u64,
    percent: f64,
    phase_percent: Option<f64>,
    phase: String,
}

fn emit_progress(
    app: &AppHandle,
    model_id: &str,
    file_name: &str,
    downloaded_bytes: u64,
    total_bytes: u64,
    phase: &str,
) {
    let _ = app.emit(
        "model-download-progress",
        DownloadProgress {
            model_id: model_id.to_string(),
            file_name: file_name.to_string(),
            downloaded_bytes,
            total_bytes,
            percent: (downloaded_bytes as f64 / total_bytes as f64 * 100.0).clamp(0.0, 100.0),
            phase_percent: None,
            phase: phase.to_string(),
        },
    );
}

fn emit_verification_progress(
    app: &AppHandle,
    model_id: &str,
    file_name: &str,
    total_bytes: u64,
    verified_bytes: u64,
    file_size: u64,
) {
    let _ = app.emit(
        "model-download-progress",
        DownloadProgress {
            model_id: model_id.to_string(),
            file_name: file_name.to_string(),
            downloaded_bytes: total_bytes,
            total_bytes,
            percent: 100.0,
            phase_percent: Some(
                (verified_bytes as f64 / file_size.max(1) as f64 * 100.0).clamp(0.0, 100.0),
            ),
            phase: "verifying".to_string(),
        },
    );
}

fn specs() -> Vec<ModelSpec> {
    vec![
        ModelSpec {
            id: "whisper-large-v3-turbo-q5",
            name: "Whisper large-v3-turbo Q5",
            description: "Nhận dạng đa ngôn ngữ với Silero VAD, tối ưu cho Apple Silicon.",
            provider: "asr",
            license: "MIT",
            files: vec![
                ModelFile {
                    relative_path: "ggml-large-v3-turbo-q5_0.bin",
                    url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
                    size: 574_041_195,
                    sha256: Some("394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2"),
                },
                ModelFile {
                    relative_path: "ggml-silero-v6.2.0.bin",
                    url: "https://huggingface.co/ggml-org/whisper-vad/resolve/main/ggml-silero-v6.2.0.bin",
                    size: 885_098,
                    sha256: Some("2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987"),
                },
            ],
        },
        ModelSpec {
            id: "hy-mt2-7b-q4",
            name: "Hy-MT2 7B Q4_K_M",
            description: "Model dịch Nhật/Anh sang Việt, ưu tiên đúng nghĩa và thuật ngữ.",
            provider: "translation",
            license: "Apache-2.0",
            files: vec![ModelFile {
                relative_path: "Hy-MT2-7B-Q4_K_M.gguf",
                url: "https://huggingface.co/tencent/Hy-MT2-7B-GGUF/resolve/main/Hy-MT2-7B-Q4_K_M.gguf",
                size: 4_624_648_896,
                sha256: Some("9f96256500f3fc1ab4d64336b58f52a949a95ad7516b0c229476eef782f9f77b"),
            }],
        },
        ModelSpec {
            id: "qwen3-14b-q4",
            name: "Qwen3 14B Q4_K_M",
            description: "Biên tập lời thoại theo nhân vật, bối cảnh và phong cách của Project Editor.",
            provider: "translation",
            license: "Apache-2.0",
            files: vec![ModelFile {
                relative_path: "Qwen3-14B-Q4_K_M.gguf",
                url: "https://huggingface.co/Qwen/Qwen3-14B-GGUF/resolve/main/Qwen3-14B-Q4_K_M.gguf",
                size: 9_001_752_960,
                sha256: Some("500a8806e85ee9c83f3ae08420295592451379b4f8cf2d0f41c15dffeb6b81f0"),
            }],
        },
        ModelSpec {
            id: "vieneu-v3-turbo-q8",
            name: "VieNeu-TTS v3 Turbo Q8",
            description: "Giọng Việt 48 kHz với bảy preset nam và nữ cho nhiều nhân vật.",
            provider: "tts",
            license: "Apache-2.0",
            files: vec![
                ModelFile {
                    relative_path: "vieneu-v3-turbo-q8_0.gguf",
                    url: "https://huggingface.co/pnnbao-ump/VieNeu-TTS-v3-Turbo/resolve/main/gguf/vieneu-v3-turbo-q8_0.gguf",
                    size: 187_917_664,
                    sha256: Some("a60569e5d7dd6f24cbb7bb2e7c472bb988ad19563a4e78f0cc67cc53fd189bb4"),
                },
                ModelFile { relative_path: "voices/manifest.json", url: "https://huggingface.co/pnnbao-ump/VieNeu-TTS-v3-Turbo/resolve/main/gguf/voices/manifest.json", size: 3_855, sha256: None },
                ModelFile { relative_path: "voices/thuy_dung/ref_codes.txt", url: "https://huggingface.co/pnnbao-ump/VieNeu-TTS-v3-Turbo/resolve/main/gguf/voices/thuy_dung/ref_codes.txt", size: 3_460, sha256: None },
                ModelFile { relative_path: "voices/thuy_dung/speaker.emb.txt", url: "https://huggingface.co/pnnbao-ump/VieNeu-TTS-v3-Turbo/resolve/main/gguf/voices/thuy_dung/speaker.emb.txt", size: 2_206, sha256: None },
                ModelFile { relative_path: "voices/thuc_doan/ref_codes.txt", url: "https://huggingface.co/pnnbao-ump/VieNeu-TTS-v3-Turbo/resolve/main/gguf/voices/thuc_doan/ref_codes.txt", size: 2_499, sha256: None },
                ModelFile { relative_path: "voices/thuc_doan/speaker.emb.txt", url: "https://huggingface.co/pnnbao-ump/VieNeu-TTS-v3-Turbo/resolve/main/gguf/voices/thuc_doan/speaker.emb.txt", size: 2_205, sha256: None },
                ModelFile { relative_path: "voices/my_duyen/ref_codes.txt", url: "https://huggingface.co/pnnbao-ump/VieNeu-TTS-v3-Turbo/resolve/main/gguf/voices/my_duyen/ref_codes.txt", size: 4_729, sha256: None },
                ModelFile { relative_path: "voices/my_duyen/speaker.emb.txt", url: "https://huggingface.co/pnnbao-ump/VieNeu-TTS-v3-Turbo/resolve/main/gguf/voices/my_duyen/speaker.emb.txt", size: 2_215, sha256: None },
                ModelFile { relative_path: "voices/thai_son/ref_codes.txt", url: "https://huggingface.co/pnnbao-ump/VieNeu-TTS-v3-Turbo/resolve/main/gguf/voices/thai_son/ref_codes.txt", size: 3_117, sha256: None },
                ModelFile { relative_path: "voices/thai_son/speaker.emb.txt", url: "https://huggingface.co/pnnbao-ump/VieNeu-TTS-v3-Turbo/resolve/main/gguf/voices/thai_son/speaker.emb.txt", size: 2_209, sha256: None },
                ModelFile { relative_path: "voices/minh_triet/ref_codes.txt", url: "https://huggingface.co/pnnbao-ump/VieNeu-TTS-v3-Turbo/resolve/main/gguf/voices/minh_triet/ref_codes.txt", size: 3_559, sha256: None },
                ModelFile { relative_path: "voices/minh_triet/speaker.emb.txt", url: "https://huggingface.co/pnnbao-ump/VieNeu-TTS-v3-Turbo/resolve/main/gguf/voices/minh_triet/speaker.emb.txt", size: 2_209, sha256: None },
                ModelFile { relative_path: "voices/kim_thanh/ref_codes.txt", url: "https://huggingface.co/pnnbao-ump/VieNeu-TTS-v3-Turbo/resolve/main/gguf/voices/kim_thanh/ref_codes.txt", size: 4_587, sha256: None },
                ModelFile { relative_path: "voices/kim_thanh/speaker.emb.txt", url: "https://huggingface.co/pnnbao-ump/VieNeu-TTS-v3-Turbo/resolve/main/gguf/voices/kim_thanh/speaker.emb.txt", size: 2_209, sha256: None },
                ModelFile { relative_path: "voices/duc_tri/ref_codes.txt", url: "https://huggingface.co/pnnbao-ump/VieNeu-TTS-v3-Turbo/resolve/main/gguf/voices/duc_tri/ref_codes.txt", size: 4_864, sha256: None },
                ModelFile { relative_path: "voices/duc_tri/speaker.emb.txt", url: "https://huggingface.co/pnnbao-ump/VieNeu-TTS-v3-Turbo/resolve/main/gguf/voices/duc_tri/speaker.emb.txt", size: 2_204, sha256: None },
            ],
        },
        ModelSpec {
            id: "mel-band-roformer-q8",
            name: "Mel-Band RoFormer Q8",
            description: "Tách vocal khỏi nhạc và hiệu ứng trước khi mix lời thoại Việt.",
            provider: "audio",
            license: "MIT",
            files: vec![ModelFile {
                relative_path: "mel-band-roformer-q8_0.gguf",
                url: "https://huggingface.co/audio-cpp/audio.cpp-gguf/resolve/main/Mel-Band-RoFormer-GGUF/mel-band-roformer-q8_0.gguf",
                size: 251_748_928,
                sha256: Some("2dd898ceb0e3812c18d6125dcd60174d35d3da22c94add76b029fbb21fc238fd"),
            }],
        },
    ]
}

pub fn models_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|path| path.join("models"))
        .map_err(|error| error.to_string())
}

pub fn model_path(app: &AppHandle, id: &str, file: &str) -> Result<PathBuf, String> {
    Ok(models_dir(app)?.join(id).join(file))
}

fn find_spec(id: &str) -> Result<ModelSpec, String> {
    specs()
        .into_iter()
        .find(|spec| spec.id == id)
        .ok_or_else(|| format!("Model không tồn tại: {id}"))
}

fn file_is_installed(path: &Path, expected_size: u64) -> bool {
    path.metadata()
        .map(|meta| meta.len() == expected_size)
        .unwrap_or(false)
}

fn partial_path(destination: &Path) -> PathBuf {
    destination.with_extension("part")
}

pub fn list(app: &AppHandle) -> Result<Vec<ModelInfo>, String> {
    let root = models_dir(app)?;
    let retired_speaker_model = root.join("speaker-diarization-pyannote");
    if retired_speaker_model.exists() {
        std::fs::remove_dir_all(&retired_speaker_model).map_err(|error| {
            format!(
                "Không thể dọn model nhận diện giọng đã ngừng dùng tại {}: {error}",
                retired_speaker_model.display()
            )
        })?;
    }
    Ok(specs()
        .into_iter()
        .map(|spec| {
            let size_bytes = spec.files.iter().map(|file| file.size).sum();
            let installed_bytes = spec
                .files
                .iter()
                .map(|file| {
                    let destination = root.join(spec.id).join(file.relative_path);
                    destination
                        .metadata()
                        .or_else(|_| partial_path(&destination).metadata())
                        .map(|meta| meta.len().min(file.size))
                        .unwrap_or(0)
                })
                .sum();
            let installed = spec.files.iter().all(|file| {
                file_is_installed(&root.join(spec.id).join(file.relative_path), file.size)
            });
            ModelInfo {
                id: spec.id.to_string(),
                name: spec.name.to_string(),
                description: spec.description.to_string(),
                provider: spec.provider.to_string(),
                size_bytes,
                license: spec.license.to_string(),
                state: if installed {
                    "installed"
                } else if installed_bytes > 0 {
                    "partial"
                } else {
                    "missing"
                }
                .to_string(),
                installed_bytes,
            }
        })
        .collect())
}

async fn verify_sha256(
    app: &AppHandle,
    model_id: &str,
    file_name: &str,
    path: &Path,
    expected: &str,
    total_bytes: u64,
    file_size: u64,
) -> Result<(), String> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|error| error.to_string())?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 8 * 1024 * 1024];
    let mut verified_bytes = 0_u64;
    let mut next_progress = 0_u64;
    loop {
        let read = file
            .read(&mut buffer)
            .await
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
        verified_bytes += read as u64;
        if verified_bytes >= next_progress || verified_bytes == file_size {
            emit_verification_progress(
                app,
                model_id,
                file_name,
                total_bytes,
                verified_bytes,
                file_size,
            );
            next_progress = verified_bytes.saturating_add(128 * 1024 * 1024);
        }
    }
    let actual = hex::encode(digest.finalize());
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "Checksum không đúng cho {} (nhận {}, cần {})",
            path.display(),
            actual,
            expected
        ))
    }
}

async fn download_inner(app: &AppHandle, id: &str) -> Result<(), String> {
    let spec = find_spec(id)?;
    let client = reqwest::Client::builder()
        .user_agent("LongTieng/0.1")
        .build()
        .map_err(|error| error.to_string())?;
    let total_bytes: u64 = spec.files.iter().map(|file| file.size).sum();
    let mut completed_bytes = 0_u64;

    for file in &spec.files {
        let destination = model_path(app, spec.id, file.relative_path)?;
        if file_is_installed(&destination, file.size) {
            completed_bytes += file.size;
            continue;
        }
        if let Some(parent) = destination.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|error| error.to_string())?;
        }
        let temp = partial_path(&destination);
        let mut existing_bytes = temp.metadata().map(|meta| meta.len()).unwrap_or(0);
        if existing_bytes > file.size {
            tokio::fs::remove_file(&temp)
                .await
                .map_err(|error| error.to_string())?;
            existing_bytes = 0;
        }

        if existing_bytes == file.size {
            emit_progress(
                app,
                spec.id,
                file.relative_path,
                completed_bytes + existing_bytes,
                total_bytes,
                "verifying",
            );
        } else {
            emit_progress(
                app,
                spec.id,
                file.relative_path,
                completed_bytes + existing_bytes,
                total_bytes,
                if existing_bytes > 0 {
                    "resuming"
                } else {
                    "connecting"
                },
            );
        }

        if existing_bytes < file.size {
            let mut request = client.get(format!("{}?download=true", file.url));
            if existing_bytes > 0 {
                request = request.header(RANGE, format!("bytes={existing_bytes}-"));
            }
            let response = request
                .send()
                .await
                .map_err(|error| format!("Không thể tải {}: {error}", file.relative_path))?
                .error_for_status()
                .map_err(|error| format!("Máy chủ từ chối {}: {error}", file.relative_path))?;

            let is_resume = existing_bytes > 0 && response.status() == StatusCode::PARTIAL_CONTENT;
            if !is_resume {
                existing_bytes = 0;
            }
            let mut output = tokio::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .append(is_resume)
                .truncate(!is_resume)
                .open(&temp)
                .await
                .map_err(|error| error.to_string())?;
            let mut stream = response.bytes_stream();
            let mut current_file_bytes = existing_bytes;
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|error| {
                    format!(
                        "Mất kết nối khi tải {} tại {} byte: {error}",
                        file.relative_path, current_file_bytes
                    )
                })?;
                output
                    .write_all(&chunk)
                    .await
                    .map_err(|error| error.to_string())?;
                current_file_bytes += chunk.len() as u64;
                emit_progress(
                    app,
                    spec.id,
                    file.relative_path,
                    completed_bytes + current_file_bytes,
                    total_bytes,
                    "downloading",
                );
            }
            output.flush().await.map_err(|error| error.to_string())?;
            drop(output);
        }

        let actual_size = temp.metadata().map_err(|error| error.to_string())?.len();
        if actual_size != file.size {
            return Err(format!(
                "File tải chưa hoàn tất: {} có {actual_size}/{} byte. Bấm Tải tiếp để tiếp tục.",
                file.relative_path, file.size
            ));
        }
        if let Some(expected) = file.sha256 {
            emit_progress(
                app,
                spec.id,
                file.relative_path,
                completed_bytes + actual_size,
                total_bytes,
                "verifying",
            );
            if let Err(error) = verify_sha256(
                app,
                spec.id,
                file.relative_path,
                &temp,
                expected,
                total_bytes,
                file.size,
            )
            .await
            {
                let _ = tokio::fs::remove_file(&temp).await;
                return Err(error);
            }
        }
        emit_progress(
            app,
            spec.id,
            file.relative_path,
            completed_bytes + actual_size,
            total_bytes,
            "installing",
        );
        tokio::fs::rename(&temp, &destination)
            .await
            .map_err(|error| error.to_string())?;
        completed_bytes += file.size;
    }
    emit_progress(app, spec.id, "", total_bytes, total_bytes, "complete");
    Ok(())
}

pub async fn download(app: AppHandle, id: String) -> Result<(), String> {
    let _guard = DownloadGuard::claim(&id)?;
    let partial = list(&app)
        .ok()
        .and_then(|items| items.into_iter().find(|item| item.id == id))
        .map(|item| item.installed_bytes)
        .unwrap_or_default();
    let _ = diagnostics::append(
        &app,
        diagnostics::entry(
            "info",
            "model.download",
            format!("Bắt đầu tải {id}"),
            Some(format!("Dữ liệu có sẵn: {partial} byte")),
        ),
    );
    match download_inner(&app, &id).await {
        Ok(()) => {
            let _ = diagnostics::append(
                &app,
                diagnostics::entry("info", "model.download", format!("Đã cài model {id}"), None),
            );
            Ok(())
        }
        Err(error) => {
            let _ = diagnostics::append(
                &app,
                diagnostics::entry(
                    "error",
                    "model.download",
                    format!("Không thể cài model {id}"),
                    Some(error.clone()),
                ),
            );
            Err(error)
        }
    }
}

pub async fn remove(app: &AppHandle, id: &str) -> Result<(), String> {
    let _ = find_spec(id)?;
    let target = models_dir(app)?.join(id);
    if target.exists() {
        tokio::fs::remove_dir_all(target)
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}
