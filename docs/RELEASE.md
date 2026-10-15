# Release

## Release Shape

The installer bundles the Tauri app and native engines, but not the large AI model files. Users download models from Model Manager on first use. After models are installed, the app can run offline.

## Engine Preparation

Run this before packaging:

```bash
./scripts/prepare-engines-macos.sh
```

The script pins and prepares:

- `whisper-cli` from whisper.cpp
- `llama-cli` and `llama-server` from llama.cpp
- `ffmpeg` and `ffprobe`
- `audiocpp_cli`
- `libsea_g2p_rs.dylib`
- `sea_g2p.bin`
- `tesseract`, `vision-ocr` (macOS), and OCR dependency licenses
- license files for bundled components

Runtime lookup order is packaged resources, `src-tauri/engines`, then `PATH`.

## Model Expectations

Model Manager currently knows these model IDs:

- `whisper-large-v3-turbo-q5`
- `hy-mt2-7b-q4`
- `qwen3-14b-q4`
- `vieneu-v3-turbo-q8`
- `mel-band-roformer-q8`
- `ocr-vie-fast`

Each spec in `src-tauri/src/models.rs` defines relative path, URL, expected size and optional SHA-256. Update the TypeScript preview model list in `src/lib/tauri.ts` whenever model specs change so non-Tauri preview remains accurate.

## Packaging

### Local Build With OCR

```bash
npm run release:local
```

Lệnh này build extension, helper và app macOS `.app` theo thứ tự; app build tự chạy frontend build. Không tự đăng ký native host, reload Chrome, tải model hay mở app. Engine cần được chuẩn bị như trên từ trước.

Script kiểm tra engine OCR/WAV bắt buộc trước build; cảnh báo nếu Whisper/llama thiếu (OCR phụ đề Việt vẫn có thể dùng, transcription/dịch AI cần chuẩn bị thêm). Bản build ngày 14/10/2026 có đầy đủ engine OCR/WAV, chưa kèm ba executable Whisper/llama.

- App: `src-tauri/target/release/bundle/macos/Lồng tiếng.app`.
- Extension: `browser-extension/dist`, tiếp tục dùng đúng ID dev đã cài; Reload sau khi kết thúc phiên đang thu.
- Helper: `browser-extension/native-host/target/release/long-tieng-capture-host`. Helper đã đăng ký hiện tại không cần đổi cho extension 0.3.4 vì Rust/protocol không thay trong bản này.

0.3.4 bổ sung nén PNG lossless bằng worker, nhớ chế độ đã chọn và sửa nhịp poll capture. Smart vẫn có nhãn thử nghiệm vì chưa đạt các mục tiêu chất lượng/hiệu quả cũ; người dùng chọn dùng trước, không chặn bản build để yêu cầu thêm bài thu phim. Xem [bản local tối ưu](OCR_LOCAL_RELEASE.md). Model và extension vẫn là các thành phần riêng; `.app` không tự cài extension vào Chrome.

```bash
npm run build
cd src-tauri && cargo test
cd .. && npm run tauri build
```

The current macOS config uses ad-hoc signing:

```json
"signingIdentity": "-",
"hardenedRuntime": false
```

That is suitable for local builds. Distribution to other Macs still needs a Developer ID signing identity, hardened runtime, entitlements review and notarization.

## Preflight Checklist

- `src-tauri/engines` contains executable sidecars and required license files.
- `npm run build` passes.
- `cargo test` passes inside `src-tauri`.
- Tauri build succeeds from a clean checkout after engine preparation.
- Fresh app data can open the app, list model states and download at least one small model file.
- Existing app data with an interrupted `last_job` loads without crashing.
- Importing a Vietnamese SRT skips translation and enables TTS workflow.
- Importing an English/Japanese SRT requires translation models before translation.
- Test 5 minutes renders and opens a preview path.
- Final render falls back to ducking if source separation is unavailable.
- Diagnostics export includes engine status, model status and recent logs.
- OCR library sees capture-only sessions and committed results in app data without an import step; review saves/export survive restart, and worker reprocessing preserves the review pointer.
- OCR sidecars resolve from packaged resources, model checksum passes, and recheck runs from the packaged app.
- Native helper registration is still a separate user-approved macOS dev step; building the app does not install/reload the Chrome extension or native host.
- OCR → WAV thử uses a saved reviewed revision, no local video, and installed VieNeu/SEA-G2P/FFmpeg resources. Verify PCM16 mono 48 kHz anchored at video 0, no trimmed overflow, cache/cancel/restart/export, and unchanged existing video-project schema. Full episode export is not part of 4A.
- OCR → WAV → Toàn tập (4B): verify full preflight, explicit partial flags, preset overrides, 8-cue batches, quota/resource stops, cancel/restart/cache and atomic WAV/SRT/JSON bundle export. Validate metadata hashes/source/coverage; do not equate a synthetic long-track pass with reviewed full-film acceptance. See [4B acceptance status](OCR_MILESTONE_4B.md).

## Versioning Notes

When changing persisted data or generated artifacts:

- Add serde defaults for new project fields.
- Add a migration or load-time repair when old data is invalid.
- Increase `CURRENT_TTS_CACHE_VERSION` when old generated cue audio should be discarded.
- Change cache key namespaces when only a specific cache layer should be invalidated.
- Keep model IDs stable unless users must redownload; if IDs change, consider cleanup of retired model directories.
