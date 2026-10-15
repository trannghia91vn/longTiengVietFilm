import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  OcrSession, OcrDocument, OcrCue, OcrEvidence, OcrCandidate,
  AudioTrialSettings, AudioTrialJob, OcrAudioProject, AudioFullPreflight,
  Cue,
  DiagnosticEntry,
  DownloadProgress,
  DubProgress,
  DubResult,
  MediaInfo,
  ModelInfo,
  SubtitleDocument,
  SyncResult,
  SystemStatus,
  TranslationProgress,
  TranslationQuality,
  TranslationResult,
  VideoPreviewProgress,
  DubbingProject,
  ProjectProfile,
  ProjectProgress,
  ProjectSummary,
  RenderResult,
  SynthesizeResult,
  TranslationMemoryStatus,
} from "../types";

const isTauri = "__TAURI_INTERNALS__" in window;

const errorMessage = (reason: unknown) => reason instanceof Error ? reason.message : String(reason);

const invokeLogged = async <T,>(command: string, args?: Record<string, unknown>): Promise<T> => {
  try {
    return await invoke<T>(command, args);
  } catch (reason) {
    const message = errorMessage(reason);
    void invoke("record_diagnostic", {
      level: "error",
      source: `command.${command}`,
      message,
      details: args ? JSON.stringify(args, null, 2) : null,
    }).catch(() => undefined);
    throw reason;
  }
};

const previewModels: ModelInfo[] = [
  { id: "ocr-vie-fast", name: "OCR Vietnamese Fast", description: "Model vie của Tesseract 5, fallback OCR tiếng Việt local.", provider: "ocr", sizeBytes: 531275, license: "Apache-2.0", state: "missing", installedBytes: 0 },
  { id: "whisper-large-v3-turbo-q5", name: "Whisper large-v3-turbo Q5", description: "Nhận dạng đa ngôn ngữ với Silero VAD, tối ưu cho Apple Silicon.", provider: "asr", sizeBytes: 574926293, license: "MIT", state: "missing", installedBytes: 0 },
  { id: "hy-mt2-7b-q4", name: "Hy-MT2 7B Q4_K_M", description: "Model dịch Nhật/Anh sang Việt, ưu tiên đúng nghĩa và thuật ngữ.", provider: "translation", sizeBytes: 4624648896, license: "Apache-2.0", state: "missing", installedBytes: 0 },
  { id: "qwen3-14b-q4", name: "Qwen3 14B Q4_K_M", description: "Biên tập lời thoại theo nhân vật, bối cảnh và phong cách của Project Editor.", provider: "translation", sizeBytes: 9001752960, license: "Apache-2.0", state: "missing", installedBytes: 0 },
  { id: "vieneu-v3-turbo-q8", name: "VieNeu-TTS v3 Turbo Q8", description: "Giọng Việt 48 kHz với bảy preset nam và nữ.", provider: "tts", sizeBytes: 187963791, license: "Apache-2.0", state: "missing", installedBytes: 0 },
  { id: "mel-band-roformer-q8", name: "Mel-Band RoFormer Q8", description: "Tách vocal khỏi nhạc và hiệu ứng trước khi mix.", provider: "audio", sizeBytes: 251748928, license: "MIT", state: "missing", installedBytes: 0 },
];

export const api = {
  newOcrAudioJobId: () => invokeLogged<string>("new_ocr_audio_job_id"),
  createOcrAudioProject: (sessionId: string, expectedRevision: string) => invokeLogged<OcrAudioProject>("create_ocr_audio_project", {sessionId,expectedRevision}),
  loadOcrAudioProject: (projectId: string) => invokeLogged<OcrAudioProject>("load_ocr_audio_project", {projectId}),
  getOcrAudioJob:(projectId:string)=>invokeLogged<AudioTrialJob|null>("get_ocr_audio_job",{projectId}),
  renderOcrAudioPreview: (projectId: string, jobId: string, settings: AudioTrialSettings) => invokeLogged<OcrAudioProject>("render_ocr_audio_preview", {projectId,jobId,settings}),
  cancelOcrAudioPreview: (projectId: string, jobId: string) => invokeLogged<void>("cancel_ocr_audio_preview", {projectId,jobId}),
  exportOcrAudioPreview: (projectId: string, previewKey: string, path: string) => invokeLogged<void>("export_ocr_audio_preview", {projectId,previewKey,path}),
  preflightOcrAudioFull: (projectId:string,settings:AudioTrialSettings)=>invokeLogged<AudioFullPreflight>("preflight_ocr_audio_full",{projectId,settings}),
  exportOcrAudioFull: (projectId:string,artifactKey:string,directory:string)=>invokeLogged<string>("export_ocr_audio_full",{projectId,artifactKey,directory}),
  openOcrAudioFolder:(projectId:string)=>invokeLogged<void>("open_ocr_audio_folder",{projectId}),
  listOcrSessions: () => isTauri ? invokeLogged<OcrSession[]>("list_ocr_sessions") : Promise.resolve([]),
  loadOcrSession: (sessionId: string) => invokeLogged<OcrDocument>("load_ocr_session", { sessionId }),
  saveOcrReview: (sessionId: string, expectedRevision: string, cues: OcrCue[]) =>
    invokeLogged<OcrDocument>("save_ocr_review", { sessionId, expectedRevision, cues: cues.map(({startMs,endMs,text,reviewed,evidenceSequences}) => ({startMs,endMs,text,reviewed,evidenceSequences})) }),
  ocrEvidence: (sessionId: string, sequence: number) => invokeLogged<OcrEvidence>("get_ocr_evidence", { sessionId, sequence }),
  processOcrSession: (sessionId: string, reprocess = false) => invokeLogged<void>("process_ocr_session", { sessionId, reprocess }),
  cancelOcrSession: (sessionId: string) => invokeLogged<void>("cancel_ocr_session", { sessionId }),
  recheckOcrEvidence: (sessionId: string, sequence: number) => invokeLogged<OcrCandidate[]>("recheck_ocr_evidence", { sessionId, sequence }),
  exportOcrRevision: (sessionId: string, expectedRevision: string, format: "srt" | "txt", path: string) =>
    invokeLogged<void>("export_ocr_revision", {sessionId,expectedRevision,format,path}),
  openOcrFolder: (sessionId: string) => invokeLogged<void>("open_ocr_folder", { sessionId }),
  systemStatus: () => isTauri
    ? invokeLogged<SystemStatus>("get_system_status")
    : Promise.resolve({ ffmpeg: true, ffprobe: true, whisperCli: false, llamaCli: false, llamaServer: false, vieneuCli: false, audioCpp: false, appDataDir: "~/Library/Application Support/com.basushi.longtieng" }),
  models: () => isTauri ? invokeLogged<ModelInfo[]>("list_models") : Promise.resolve(previewModels),
  downloadModel: (modelId: string) => isTauri
    ? invokeLogged<void>("download_model", { modelId })
    : Promise.reject(new Error("Chỉ tải model trong ứng dụng Tauri.")),
  removeModel: (modelId: string) => isTauri
    ? invokeLogged<void>("remove_model", { modelId })
    : Promise.reject(new Error("Chỉ xóa model trong ứng dụng Tauri.")),
  inspectMedia: (path: string) => invokeLogged<MediaInfo>("inspect_media", { path }),
  prepareVideoPreview: (path: string, forceProxy = false) =>
    invokeLogged<string>("prepare_video_preview", { path, forceProxy }),
  readSrt: (path: string, language: string) =>
    invokeLogged<SubtitleDocument>("read_srt", { path, language }),
  transcribePreview: (videoPath: string, sourceLanguage: string, seconds = 300) =>
    invokeLogged<SubtitleDocument>("transcribe_preview", { videoPath, seconds, sourceLanguage }),
  translateSrt: (inputPath: string, sourceLanguage: string, quality: TranslationQuality = "natural", style = "cinematic-neutral") =>
    invokeLogged<TranslationResult>("translate_srt", { inputPath, sourceLanguage, quality, style }),
  syncSubtitles: (referencePath: string, candidatePath: string) =>
    invokeLogged<SyncResult>("sync_subtitles", { referencePath, candidatePath }),
  renderDubbedVideo: (videoPath: string, subtitlePath: string, outputPath: string | null, maxSeconds: number | null) =>
    invokeLogged<DubResult>("render_dubbed_video", { videoPath, subtitlePath, outputPath, maxSeconds }),
  writeSrt: (path: string, cues: Cue[]) => invokeLogged<void>("write_srt", { path, cues }),
  listProjects: () => isTauri ? invokeLogged<ProjectSummary[]>("list_projects") : Promise.resolve([]),
  createProject: (videoPath: string, sourceLanguage: "ja" | "en", profile: ProjectProfile) =>
    invokeLogged<DubbingProject>("create_project", { videoPath, sourceLanguage, profile }),
  loadProject: (projectId: string) => invokeLogged<DubbingProject>("load_project", { projectId }),
  updateProject: (project: DubbingProject) => invokeLogged<DubbingProject>("update_project", { project }),
  importProjectSrt: (projectId: string, path: string, language: "ja" | "en" | "vi") =>
    invokeLogged<DubbingProject>("import_project_srt", { projectId, path, language }),
  analyzeProject: (projectId: string, sampleSeconds: number | null) =>
    invokeLogged<DubbingProject>("analyze_project", { projectId, sampleSeconds }),
  translateProject: (projectId: string) => invokeLogged<DubbingProject>("translate_project", { projectId }),
  translationMemoryStatus: () => invokeLogged<TranslationMemoryStatus>("get_translation_memory_status"),
  setTranslationMemoryEnabled: (enabled: boolean) =>
    invokeLogged<TranslationMemoryStatus>("set_translation_memory_enabled", { enabled }),
  saveTranslationCorrection: (projectId: string, cueId: string, correctedText: string) =>
    invokeLogged<TranslationMemoryStatus>("save_translation_correction", { projectId, cueId, correctedText }),
  clearTranslationMemory: () => invokeLogged<TranslationMemoryStatus>("clear_translation_memory"),
  previewVoice: (voice: string) => invokeLogged<string>("preview_voice", { voice }),
  synthesizeCues: (projectId: string, cueIds: string[], maxSeconds: number | null) =>
    invokeLogged<SynthesizeResult>("synthesize_cues", { request: { projectId, cueIds, maxSeconds } }),
  renderProject: (projectId: string, outputPath: string | null, maxSeconds: number | null) =>
    invokeLogged<RenderResult>("render_project", { projectId, outputPath, maxSeconds }),
  cancelJob: () => invokeLogged<void>("cancel_job"),
  diagnostics: () => isTauri ? invoke<DiagnosticEntry[]>("get_diagnostics") : Promise.resolve([]),
  clearDiagnostics: () => isTauri ? invoke<void>("clear_diagnostics") : Promise.resolve(),
  writeTextFile: (path: string, content: string) => invokeLogged<void>("write_text_file", { path, content }),
  onDownloadProgress: (handler: (event: DownloadProgress) => void): Promise<UnlistenFn> => isTauri
    ? listen<DownloadProgress>("model-download-progress", (event) => handler(event.payload))
    : Promise.resolve(() => undefined),
  onPreviewProgress: (handler: (event: VideoPreviewProgress) => void): Promise<UnlistenFn> => isTauri
    ? listen<VideoPreviewProgress>("video-preview-progress", (event) => handler(event.payload))
    : Promise.resolve(() => undefined),
  onDubProgress: (handler: (event: DubProgress) => void): Promise<UnlistenFn> => isTauri
    ? listen<DubProgress>("dub-progress", (event) => handler(event.payload))
    : Promise.resolve(() => undefined),
  onTranslationProgress: (handler: (event: TranslationProgress) => void): Promise<UnlistenFn> => isTauri
    ? listen<TranslationProgress>("translation-progress", (event) => handler(event.payload))
    : Promise.resolve(() => undefined),
  onDiagnostic: (handler: (entry: DiagnosticEntry) => void): Promise<UnlistenFn> => isTauri
    ? listen<DiagnosticEntry>("diagnostic-entry", (event) => handler(event.payload))
    : Promise.resolve(() => undefined),
  onProjectProgress: (handler: (event: ProjectProgress) => void): Promise<UnlistenFn> => isTauri
    ? listen<ProjectProgress>("project-progress", (event) => handler(event.payload))
    : Promise.resolve(() => undefined),
};
