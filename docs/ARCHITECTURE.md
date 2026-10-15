# Architecture

Long Tieng is a local-first desktop studio for Vietnamese dubbing. The frontend is React/Vite, the native shell and backend are Tauri v2/Rust, and AI/media work is delegated to local sidecar binaries plus downloaded GGUF/model files.

## Runtime Layers

```text
React UI
  -> src/lib/tauri.ts typed API wrapper
  -> Tauri commands in src-tauri/src/lib.rs
  -> Rust domain modules
  -> native engines in resources/engines or src-tauri/engines
  -> app data projects, models, cache and diagnostics
```

Frontend code should use `src/lib/tauri.ts` as the API boundary. Rust commands are registered in `src-tauri/src/lib.rs` and return serde camelCase payloads to match `src/types.ts`.

## Frontend

- `src/App.tsx` owns navigation, high-level system/model/memory status and Settings.
- `src/components/ProjectEditor.tsx` owns the main workflow: create/open project, import SRT, translate, synthesize, test, export and cue editing.
- `src/components/OcrWorkspace.tsx` owns capture-library polling, review drafts/history, cue/evidence inspection and exports. It stays mounted (hidden) across sidebar changes; OCR commands remain in the typed API wrapper.
- `src/components/ModelManager.tsx` lists model state and drives downloads/removal.
- `src/components/Diagnostics.tsx` reads local diagnostic entries and builds support reports.
- `src/styles.css` contains the app surface, workflow, timeline, inspector, models and diagnostics styling.

The frontend listens to these long-running event streams:

- `model-download-progress`
- `project-progress`
- `diagnostic-entry`
- Legacy/specialized streams: `video-preview-progress`, `dub-progress`, `translation-progress`

## Rust Modules

- `lib.rs`: Tauri command registration and thin adapters.
- `project.rs`: project schema, storage, imports, translation context, translation memory, long-job state, TTS generation, render flow and cancellation.
- `pipeline.rs`: media probing, preview proxy, Whisper ASR, llama-server orchestration, SRT translation, sync, and older direct render helpers.
- `models.rs`: model specs, download resume, checksum verification, install/remove state.
- `diagnostics.rs`: append/read/clear local logs with a 300-entry cap.
- `srt.rs`: SubRip parser and serializer.
- `ocr.rs`: thin Tauri adapters to the shared native-host crate, blocking disk/engine work off the UI thread; session-scoped processing/cancellation and review exports.
- `audio_project.rs`: independent audio-only schema, immutable review snapshot, bounded trial selection, cache, zero-anchored PCM timeline and atomic artifact export.

## App Data

The app uses Tauri `app_data_dir()` for mutable runtime state:

```text
app-data/
|-- projects/<project-id>/project.json
|-- projects/<project-id>/source.srt
|-- projects/<project-id>/vietnamese.srt
|-- audio-projects/<sha256>/source.json
|-- audio-projects/<sha256>/project.json
|-- audio-projects/<sha256>/cache/<key>/...
|-- audio-projects/<sha256>/previews/<key>/{preview.wav,report.json}
|-- models/<model-id>/...
|-- previews/<hash>.mp4
|-- workspace/<uuid>/...
|-- translation-memory.json
|-- translation-memory-settings.json
`-- diagnostics.json
```

Project writes are atomic: write `project.json.tmp`, then rename. A project loaded with a running `last_job` is marked `interrupted` so users can continue from saved state.

## Dubbing Project Schema

`DubbingProject` stores:

- source video path, preview path, source/target language and profile.
- speakers, glossary, character bible and translation guide.
- dialogue cues with source text, translated text, speaker, audio path, timing, status and warnings.
- audio settings and generated assets.
- `tts_cache_version` for invalidating stale generated audio.
- `last_job` for resumable synthesize/render tasks.

When adding fields, give Rust serde defaults where older projects need to load, mirror the field in `src/types.ts`, and add migration-oriented tests when behavior changes.

## Translation Pipeline

For source SRT in English or Japanese:

1. Build or refresh project context from speakers, glossary, user notes and source cues.
2. Qwen3 reads semantic scene chunks and produces synopsis, tone, relationships, address rules and scene metadata.
3. Hy-MT2 translates target chunks with neighboring context, scene context, utterance grouping and recent accepted Vietnamese lines.
4. Qwen3 edits the Vietnamese draft for natural dialogue, xưng hô, continuity and project style.
5. Warnings attach to affected cues and scenes.
6. `vietnamese.srt` is written from `translated_text`.

For imported Vietnamese SRT, cues start with `translated_text` populated and translation is skipped.

## Translation Memory

Translation memory is local and capped at 2,000 entries. It stores only user corrections to machine-translated cues, keyed by source language, profile, source text and corrected text. It is disabled/enabled through `translation-memory-settings.json`.

Do not store imported Vietnamese SRT lines in memory because there is no machine source to learn from.

## TTS And Rendering

`project::synthesize` generates audio for selected or all translated cues. It:

- builds raw and processed cache keys.
- reuses valid cached WAV when possible.
- runs `audiocpp_cli` with VieNeu-TTS voice assets.
- normalizes/generated audio to cue timing, speed and volume.
- checkpoints progress and records failures with cue/voice/batch details.

`project::render` mixes the source video with generated Vietnamese audio. Test renders can limit duration to five minutes. Final render can attempt background separation with Mel-Band RoFormer when enabled; if unavailable or failed, it falls back to ducking the original audio.

## Model And Engine Strategy

Sidecar engines are bundled or found in development:

1. packaged Tauri resources `engines/`
2. development directory `src-tauri/engines`
3. `PATH`

Large model files are not bundled in Git. `models.rs` downloads them to app data, supports `.part` resume, verifies size/checksum, and reports state to the UI.

## Native OCR Capture

`browser-extension/native-host` is a standalone Rust library/binary, not a Tauri window or HTTP service. Chrome Native Messaging commits capture PNG/journal with durable ACK (2A). Version 0.3 adds a separate OCR worker after capture finalize, plus `ocrStart/ocrStatus/ocrCancel` scoped to a canonical session UUID.

Extension 0.3.3 binds the selected video to its real iframe/document ancestry using optional `webNavigation` on the selected tab. Element tokens are resolved once through content scripts; geometry probes target exact frame/document identities. ROI is anchored to rendered video, not all page iframes. Fullscreen geometry stops at the top-layer root; scrolled HTML cannot clip it. Stream size follows viewport/DPR within native limits. Actual player movement still invalidates selection; unrelated ads do not. See [capture stability report](OCR_CAPTURE_STABILITY.md).

`src/ocr/engine.rs` manages bounded/timed/cancellable Vision/Tesseract sidecars; `src/ocr/cues.rs` assembles exact/consensus observations and evidence/timing warnings; `src/ocr/mod.rs` owns locks, checkpoint recovery, coverage and atomic output revisions. The SRT parser/serializer is reused from `src-tauri/src/srt.rs`. The Vision adapter lives in `native-host/vision-ocr`, has no network code, and uses Vietnamese only when the OS reports support. Tesseract `vie` fast is registered in the existing Model Manager.

Capture owns `session.json/journal.ndjson`; raw schema 1 remains compatible. Experimental smart schema 2 adds dense timeline observations referencing separately committed PNG evidence, negotiated via `smart-observations-v1` on protocol 1. The helper validates references before durable ACK. Coverage/assembly consume observations rather than sparse image spacing. Stable groups reuse recognition only after representative OCR agrees; disagreements recognize every retained image. See [smart capture report](OCR_SMART_CAPTURE.md) for acceptance gates and measured limitations.

OCR has `ocr.json`, `ocr-work/<fingerprint>/observations.ndjson` and `ocr-revisions/<revision>/` canonical SRT/TXT/cue files. Smart work also has atomic per-image `recognition/*.json` caches with original JSON checksums and source image hashes, allowing restart before group-derived observations finish. The app library verifies committed revision/hashes, not root aliases. Every cue is initially unreviewed; source coverage and recognition quality are independent of a successful file commit. Audio-only trials (4A) and full-timeline rendering (4B) are implemented; source-film quality/coverage acceptance remains separate.

`native-host/src/review.rs` is shared by the desktop via a Cargo path dependency. A saved edit writes `review-revisions/<hash>/{subtitles.vi.srt,subtitles.vi.txt,cues.json,manifest.json}` then atomically replaces `review.json`. The pointer stores file hashes, parent revision and a complete `baseOcr` snapshot. OCR workers own only OCR files, never this pointer. Review readers validate both edited files and the pinned original revision, so a later worker result cannot silently replace edits. Save requires a per-session `.review.lock` and expected revision; orphan outputs are not published after a crash. Evidence keeps exact observation JSON checksums and PNG hashes. Export is restricted from overwriting internal app data. Recheck returns candidates without changing a cue or any persisted result.

The desktop OCR job uses the same process/recovery code in a blocking task. Closing the desktop during its own OCR job may interrupt it; reopen/retry resumes checkpoints. Chrome-started detached helper workers retain the 2B lifecycle. Packaged OCR sidecars are resolved from app resources as well as helper-adjacent/development paths, not arbitrary browser input.

## OCR Audio Trials

`OcrAudioPreview` is mounted within the OCR workspace. `create_ocr_audio_project` checks the expected saved revision and copies its cues/coverage to a hashed immutable `source.json`. The ID includes session and revision; editing OCR creates a different audio project. No local video path is required. `LONG_TIENG_OCR_TEST_DATA` redirects OCR/audio writes only in debug; model lookup still uses the real Model Manager directory.

Commands are `new_ocr_audio_job_id`, `create_ocr_audio_project`, `load_ocr_audio_project`, `render_ocr_audio_preview`, `cancel_ocr_audio_preview`, and `export_ocr_audio_preview`. UUID allocation occurs in Rust because native WebView may lack `crypto.randomUUID`. UI polls persisted job state; shared TTS progress is routed to `ocr-audio-engine-progress`, never the video editor's stream. A shared process-local media gate rejects concurrent video TTS/render/voice-preview and OCR audio jobs. A per-project file lock protects audio writers, and cancellation is scoped by UUID.

Trials cover 30 or 60 seconds, at most 64 whole reviewed nonoverlapping cues, inside one captured range; total output end is capped at two hours. The timeline writer copies full processed PCM at each video timestamp with silence before/between/after cues. FFmpeg normalization allows tempo 1–1.25 with 80 ms headroom; overlong speech fails without publishing a trimmed WAV. A successful file is PCM16 mono 48 kHz from video 0 to trial end, not a clip reset to zero at the chosen start.

Cache identity includes source/revision, text, voice, timing, pipeline, backend and executable/model/voice hashes. Files are synced before cache metadata. WAV/report are durable before `project.json` points at them; load validates source and artifact checksums and marks an unlocked running job interrupted. A failed/cancelled retry retains the previous preview. See the [4A report](OCR_MILESTONE_4A.md) for tested limits and pending gates.

## Full-Timeline Audio

`audio_project/full.rs` owns full preflight, coverage union/gaps, quotas, a resource watcher and bundle export. `TrialSettings.full` and `AudioProject.lastFull` are optional additive fields: absent legacy settings serialize exactly as before, and trial/full artifacts remain independent. Full settings use start 0 and captured video duration; when duration is unknown the user can provide an end time only with explicit partial consent. Both modes retain the two-hour bound; full mode allows up to 10,000 cues.

Full preflight validates every cue, unique indices, readable text, review flags, overlaps, end bounds and voice overrides. Coverage merges replayed ranges with a 250 ms sampling-edge tolerance; large missing ranges remain gaps. Consent never bypasses cue validation. `wholeEpisodeVerified` remains false, even with all cues reviewed and no measured coverage gap: those facts cannot prove OCR recall.

The renderer batches at most eight cues, grouping by preset voice, and checkpoints each normalized cue. A full job collects all overlong-cue failures before refusing final publication. The original speech-processing fingerprint/cache is reused for unchanged cue inputs, including 4A trials from the same snapshot. A source revision change creates a new project; voice assignments are tied to that revision's cue IDs, not transferred automatically across reindexing.

`.quota.lock` serializes writers across audio projects/processes. Defaults: 2 GiB per audio project, 10 GiB audio library, 1 GiB free-disk reserve; these are separate from capture/image quotas and model storage. Preflight is conservative, checks repeat around batches/final output, and a periodic watcher stops managed processes on quota/free-space or oversized cue/workspace WAV (>16 MiB). Checks are application-level, not an OS filesystem quota: temporary overshoot between scans is possible. Partial files/orphans are preserved, never automatically removed.

`get_ocr_audio_job` reads only the source/checkpoint and interruption state for polling. It cannot publish an unchecked artifact. Opening/loading/exporting still verifies WAV/SRT/report checksums, format, source revision/identity and coverage metadata. Full outputs use report schema 2, kind `ocr-narration`, with source URL/title/session/revision/hash, SRT/WAV hashes, PCM format, duration source, video-zero origin/offset, missing ranges and partial/review flags. External exports contain `narration.vi.wav`, `narration.vi.json`, `subtitles.vi.srt` in one immutable `narration-<key>/` directory, synced then atomically renamed; a repeated identical export is idempotent, and modified existing exports are never overwritten.

New commands: `preflight_ocr_audio_full`, `get_ocr_audio_job`, `export_ocr_audio_full`, `open_ocr_audio_folder`. Full rendering reuses the existing typed render/cancel endpoint and scoped media gate; its job kind is `ocr-audio-full`, routed to the audio-only event stream. See the [4B report](OCR_MILESTONE_4B.md).
