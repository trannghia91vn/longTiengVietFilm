---
name: long-tieng-film
description: Maintain the Long Tieng local dubbing app and its in-repo Chrome capture/OCR helper, including docs, pipeline, model management, and release workflows.
metadata:
  short-description: Maintain the Long Tieng local dubbing app
---

# Long Tieng Film

Use this skill when working inside this repository or when the user asks to change the Long Tieng local dubbing app.

## Start Here

Read [AGENT.md](AGENT.md) first for repository-specific invariants. For larger changes, also read the relevant reference:

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for module boundaries, app data, commands, events and pipeline shape.
- [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) for setup, verification commands and common failure modes.
- [docs/RELEASE.md](docs/RELEASE.md) for engine preparation, model expectations and packaging.
- [docs/OCR_MILESTONE_2A.md](docs/OCR_MILESTONE_2A.md) and [docs/OCR_MILESTONE_2B.md](docs/OCR_MILESTONE_2B.md) when changing Chrome/native capture, OCR, or SRT storage; these distinguish tested behavior from pending quality and installation gates.
- [docs/OCR_MILESTONE_3.md](docs/OCR_MILESTONE_3.md) for app library/editor, review-pointer ownership, optimistic saves and isolated UI testing.
- [docs/OCR_MILESTONE_4A.md](docs/OCR_MILESTONE_4A.md) for audio-only snapshots, zero-anchored WAV trials, scoped cancellation, no-trim validation and real-model QA.
- [docs/OCR_MILESTONE_4B.md](docs/OCR_MILESTONE_4B.md) for full-timeline preflight, explicit partial outputs, bounded batches/storage, lightweight polling and atomic WAV/SRT/metadata bundles.
- [docs/OCR_SMART_CAPTURE.md](docs/OCR_SMART_CAPTURE.md) for smart detector, separate observation/image timelines, capability negotiation, replay measurements and the default-mode acceptance gate.
- [docs/OCR_CAPTURE_STABILITY.md](docs/OCR_CAPTURE_STABILITY.md) for selected-frame ancestry, geometry/ROI/fullscreen, optional navigation permission and capture health without weakening evidence guards.

## Working Rules

- Preserve the offline-first design: runtime engines are bundled, large models live in app data, and no Ollama/Python dependency should be introduced for normal app use.
- Keep frontend/native contracts synchronized across Rust serde structs, `src/types.ts`, and `src/lib/tauri.ts`.
- Keep long operations resumable and diagnosable: checkpoint project state, emit progress, support cancellation where practical, and log actionable diagnostics.
- Treat user-edited translations as local style memory only when they came from machine-translated source cues; do not learn from imported Vietnamese SRT.
- Avoid broad rewrites. Follow the existing Tauri command plus React component pattern and add tests around changed pipeline behavior.

## Verification

Prefer this baseline before handing back code changes:

```bash
npm run build
cd src-tauri && cargo test
```

When touching engine preparation or packaging, also check `./scripts/prepare-engines-macos.sh` and `npm run tauri build` if the local machine has the required native toolchain.

For extension/helper changes, add `npm run extension:build`, `npm run extension:test`, `npm run native:build`, and `npm run native:test`. For OCR engine/cue/recovery changes, use `npm run ocr:integration` with prepared engines and the documented fixture. Keep fixture scores separate from film quality. Registering or running a new native host in Chrome requires the user's confirmation at that step; a request to edit code does not grant that permission.

For OCR review changes, run `npm run ocr:editor:test` and the native crate's revision/provenance tests. Preserve worker output and `baseOcr`; publish only an immutable review revision plus its atomic pointer. Keep unsaved drafts on sidebar navigation and fail closed on revision conflicts or corrupt evidence.

For OCR audio changes, run `npm run ocr:audio:test` and app Rust tests. Use the existing TTS/FFmpeg managed helpers, but keep the audio-only schema separate from video projects. Require saved reviewed cues and preserve video-zero timing; reject overflowing speech instead of trimming it. Test cache/cancel/restart on isolated data and distinguish synthetic trial evidence from reviewed film evidence.

Full outputs must include every reviewed cue and the captured video duration. Gaps or user-provided end times require explicit partial consent and metadata; never treat missing footage as verified silence. Preserve 4A serialized defaults/cache, old artifacts on errors, global storage locks and per-cue checkpoints. Export the WAV/SRT/metadata as one atomic directory and revalidate source/hash/coverage. Keep full-film and Chrome acceptance gates separate from synthetic long-track QA.

Smart capture retains raw-mode compatibility and uses dense observations for coverage despite sparse PNG evidence. Reuse recognition only inside an OCR-confirmed stable group; disagreements require all retained evidence and review. Do not promote the experimental mode based only on storage savings or fixture scores. Keep replay read-only on source captures and report missing live/ground-truth gates separately.
