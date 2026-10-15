# Development

## Setup

Install Node.js/npm, Rust stable, Tauri prerequisites, CMake and macOS build tools. For best native performance, install full Xcode so Metal compilation is available; Command Line Tools alone can still build a CPU fallback for `audio.cpp`.

```bash
npm install
./scripts/prepare-engines-macos.sh
npm run tauri dev
```

The engine preparation script downloads and builds native dependencies. It writes binaries and license files into `src-tauri/engines`, which is used in development and bundled as a Tauri resource for builds.

## Useful Commands

```bash
npm run dev
npm run build
npm run tauri dev
npm run tauri build
cd src-tauri && cargo test
```

Use `npm run dev` only for frontend layout work. File dialogs, native media paths, model downloads and pipeline commands need `npm run tauri dev`.

## Editing Contracts

When adding a backend capability:

1. Implement domain logic in the owning Rust module.
2. Register a Tauri command in `src-tauri/src/lib.rs`.
3. Add or update types in `src/types.ts`.
4. Add a wrapper in `src/lib/tauri.ts`.
5. Use the wrapper from React components.
6. Add tests where the behavior can be checked without real model downloads.

Rust structs exposed to the frontend should keep `#[serde(rename_all = "camelCase")]`. New fields in persisted project data should use defaults when older `project.json` files may not contain them.

## Long Jobs

Long jobs should:

- call `begin_job()` at start and check cancellation between expensive steps.
- emit `project-progress` with stable `stage`, `message`, counts and percent.
- save project checkpoints through `last_job`.
- write diagnostic entries on failure, cancellation and completion when useful.
- preserve reusable outputs so a retry can skip completed work.

Avoid process spawning that cannot be cancelled or timed out. Reuse the managed command helpers in `project.rs` for TTS and other native processes that can stall.

## Testing Guidance

Run the baseline before finishing changes:

```bash
npm run build
cd src-tauri && cargo test
```

Add focused Rust tests for:

- SRT edge cases: BOM, CRLF, dot milliseconds, invalid timestamps.
- translation chunking, utterance grouping and scene boundaries.
- retaining cue IDs/timestamps through translation.
- project schema compatibility and default values.
- translation memory cap/filtering.
- cache invalidation when TTS or audio settings change.
- model download state and checksum behavior where practical.

Frontend changes should at least pass `npm run build`. When layout or workflow changes are meaningful, run the app and verify the main path manually: create/open project, import SRT, inspect workflow button states and check error messaging.

## Chrome Extension

Source nằm trong `browser-extension/`, dùng chung dependencies từ root. Chạy `npm run extension:build` và `npm run extension:test`, Load unpacked thư mục `browser-extension/dist` trong Chrome. Reload extension sau mỗi build; dữ liệu IndexedDB vẫn được giữ.

Capture thông minh schema 2 vẫn thử nghiệm, raw mặc định. Test detector/storage với extension suite và helper tests; replay chỉ đọc source journal/PNG rồi import/OCR vào HOME QA riêng. Lệnh `ocr:smart:replay`, `smart-qa.mjs`, `smart-audit.mjs`, kết quả đo và phần chưa nghiệm thu ở [OCR_SMART_CAPTURE.md](OCR_SMART_CAPTURE.md). UI fixture `tests/recorder-smart.html` dùng transport mock; không coi metrics của fixture là số đo Chrome thật. Không reload hoặc thay helper giữa phiên live.

`npm run extension:dev` phục vụ fixture/UI preview tại loopback port 1431, không tự nạp extension. UI preview ngoài extension không capture được tab. Setup, quyền iframe và ZIP inspection ở [extension README](../browser-extension/README.md); scope và kết quả nghiệm thu ở [OCR_MILESTONE_1.md](OCR_MILESTONE_1.md).

Mốc 2A thêm `npm run native:build`, `npm run native:test`, `npm run native:register -- <extension-id>` cho helper storage dev macOS. Đăng ký/chạy helper local qua Chrome cần xác nhận người dùng; không mở quyền wildcard. Read [native-host README](../browser-extension/native-host/README.md) và [báo cáo 2A](OCR_MILESTONE_2A.md) trước khi đổi protocol/ACK/recovery. Root app build không tự đăng ký native host hoặc cài engine OCR.

Mốc 2B thêm `npm run ocr:prepare` (Tesseract portable + Vision adapter + cache model checksum-verified), `npm run ocr:process -- <UUID>` và `npm run ocr:integration`. Worker dùng cùng app data nhưng không Tauri GUI. Engine/model chuẩn bị trước `native:register`; installer copy sidecars/model/license, vẫn cần xác nhận người dùng trước cập nhật Chrome. Xem [báo cáo 2B](OCR_MILESTONE_2B.md) cho output revision, checkpoint và quality gate chưa đạt trên phim thật. Fixture UI ở `http://127.0.0.1:1431/tests/ocr-preview.html` dùng transport mock, không phải capture thật; cần chạy integration để có SRT/crop mẫu.

## Common Failure Modes

For milestone 3, run `npm run ocr:editor:test` plus native-host tests. Review tests cover atomic commits, stale revision conflicts, corruption, writer locks, evidence checksums, split/merge/delete and worker-result isolation. The optional `real_film_review_on_isolated_copy` test accepts `OCR_REVIEW_TEST_SOURCE` pointing to a completed session and writes only to a fresh temporary directory.

Frontend QA fixture: `http://127.0.0.1:1420/tests/ocr-workspace.html` with `npm run dev`; it imports the real workspace and replaces only API transport. Place derived `cues.json` and its first evidence `frame.png` under ignored `browser-extension/tests/results/ocr-review/`. This is not a production library or an OCR quality test. `?state=processing` and `?state=error` exercise status views.

For native GUI QA, set `LONG_TIENG_OCR_TEST_DATA` to an isolated copy when launching a **debug** app bundle. It overrides only OCR command storage and is absent in release builds; never set it for normal use. Do not use the installed production app as evidence for a newly built source change. Default app data and Chrome registration are not changed by a build.

## OCR Audio QA

Run `npm run ocr:audio:test`, app Rust tests, and `npm run tauri build -- --debug --bundles app`. Restore/prepare FFmpeg, audio.cpp, SEA-G2P and the installed VieNeu voice assets before real-model tests. `npm run ocr:audio:qa` creates a fresh synthetic two-cue session and prints its isolated directory; launch the debug bundle with `LONG_TIENG_OCR_TEST_DATA` set to that directory. This is synthetic QA, not captured-film OCR quality evidence.

`node tests/inspect-audio-qa.mjs <isolated-data-directory>` verifies source/WAV hashes, format, duration, silence gaps and exact preservation/placement of every processed cue's PCM. It copies derived WAV/report into ignored `browser-extension/tests/results/ocr-audio/`. It requires a completed job. Responsive fixture `http://127.0.0.1:1420/tests/audio-workspace.html` uses API mocks and these ignored outputs plus the milestone-3 `ocr-review/cues.json`; do not interpret its mock metrics as native-engine measurements.

Optional `prepare_reviewed_film_trial` Rust test only accepts `LONG_TIENG_AUDIO_FILM_QA` under `/private/tmp` with `.audio-film-qa-copy`, the documented completed 2B session and its exact original revision. Copy the session first; this test creates a reviewed revision with eight operator-checked film cues, never edits source capture or production data. Then use the debug UI for 85.650–115.650 seconds. See [4A evidence](OCR_MILESTONE_4A.md). If sandboxed macOS launch aborts in `RegisterApplication` before Rust logic, use an approved native QA launch outside the shell sandbox; this is not an OCR/audio engine failure.

For 4B, `npm run ocr:audio:qa -- --full` prepares an 80-cue **synthetic** 23:42 session; `--overflow` prepares two reviewed 180 ms slots that real TTS must reject. Launch the debug bundle against the printed isolated data directory, select OCR → WAV → Toàn tập. Cancel after a committed batch, quit/reopen, resume, and inspect with `node tests/inspect-audio-qa.mjs <data> --full`. The inspector checks all processed cue PCM, gap/tail silence, full duration and metadata hashes; its high-memory PCM decoding is QA-only, not the production timeline writer.

The audio UI fixture supports `?state=blocked` for gaps/unreviewed cues. Its transport is mocked; use a 60-second two-cue QA WAV at ignored `browser-extension/tests/results/ocr-audio/ui.wav` for the player. Synthetic counts/timings in this page are not native evidence. Test the real debug app for Save/Open dialogs, long WAVs, resume and export. Native helper/Chrome permissions are not changed by these tests. Run Cargo test/build serially for the app target: concurrent Tauri builds can replace shared Rust artifacts while rustdoc is using them.

- `Không tìm thấy ffmpeg/ffprobe/whisper/llama/audio.cpp`: run `./scripts/prepare-engines-macos.sh` or verify packaged resources.
- Model says `partial`: click Tải tiếp in Model Manager; the downloader resumes from `.part`.
- Checksum failure: remove the invalid model from Model Manager and download again.
- Qwen/Hy-MT2 errors during translation: check Diagnostics for llama-server stderr and model paths.
- TTS stalls or fails: Diagnostics should include cue index, voice, batch, timeout state and tail output.
- Video preview fails for unsupported codec: FFmpeg proxy generation should create a compatible MP4 in app data.

## Style Notes

- Keep UI text Vietnamese and product-facing.
- Prefer small, local changes over broad rewrites.
- Do not introduce cloud dependencies into the normal pipeline.
- Do not commit generated model files, native build artifacts, app data, `.part` files, cache or rendered videos.
