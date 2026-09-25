export type AppView = "project" | "models" | "diagnostics" | "settings";
export type ModelState = "missing" | "partial" | "installed" | "downloading" | "invalid";

export interface ModelInfo {
  id: string;
  name: string;
  description: string;
  provider: "asr" | "translation" | "tts" | "audio";
  sizeBytes: number;
  license: string;
  state: ModelState;
  installedBytes: number;
}

export interface DownloadProgress {
  modelId: string;
  fileName: string;
  downloadedBytes: number;
  totalBytes: number;
  percent: number;
  phasePercent: number | null;
  phase: "connecting" | "resuming" | "downloading" | "verifying" | "installing" | "complete";
}

export interface DiagnosticEntry {
  timestampMs: number;
  level: "info" | "warning" | "error";
  source: string;
  message: string;
  details: string | null;
}

export interface SystemStatus {
  ffmpeg: boolean;
  ffprobe: boolean;
  whisperCli: boolean;
  llamaCli: boolean;
  llamaServer: boolean;
  vieneuCli: boolean;
  audioCpp: boolean;
  appDataDir: string;
}

export interface MediaInfo {
  path: string;
  fileName: string;
  durationMs: number;
  width: number | null;
  height: number | null;
  audioCodec: string | null;
  videoCodec: string | null;
  audioChannels: number | null;
}

export interface Cue {
  index: number;
  startMs: number;
  endMs: number;
  text: string;
}

export interface SubtitleDocument {
  path: string;
  language: string;
  cues: Cue[];
}

export type TranslationQuality = "natural" | "fast";

export interface TranslationWarning {
  cueIndex: number | null;
  code: string;
  message: string;
}

export interface TranslationResult {
  output: SubtitleDocument;
  warnings: TranslationWarning[];
  editedCues: number;
  fallbackCues: number;
  quality: TranslationQuality;
}

export interface TranslationProgress {
  phase: "loading-translator" | "translating" | "loading-editor" | "editing" | "validating" | "complete";
  current: number;
  total: number;
  percent: number;
  message: string;
}

export interface SyncResult {
  output: SubtitleDocument;
  offsetMs: number;
  speedRatio: number;
  matchedCues: number;
  confidence: number;
}

export interface VideoPreviewProgress {
  percent: number;
}

export interface DubResult {
  path: string;
  renderedCues: number;
  durationMs: number;
  voice: string;
}

export interface DubProgress {
  phase: string;
  current: number;
  total: number;
  percent: number;
  message: string;
}

export type ProjectProfile = "auto" | "modern" | "anime" | "historical" | "documentary";

export interface GlossaryEntry {
  source: string;
  target: string;
  note: string;
}

export interface SpeakerProfile {
  id: string;
  displayName: string;
  voicePreset: string;
  color: string;
  notes: string;
}

export interface DialogueCue {
  id: string;
  index: number;
  startMs: number;
  endMs: number;
  sourceText: string;
  translatedText: string;
  machineTranslatedText: string;
  sceneId: string;
  utteranceGroupId: string;
  speakerId: string;
  confidence: number;
  audioPath: string | null;
  audioDurationMs: number | null;
  speed: number;
  volume: number;
  status: "source" | "translated" | "voiced" | "warning" | string;
  warnings: string[];
}

export interface AudioSettings {
  separateBackground: boolean;
  backgroundVolume: number;
  dialogueVolume: number;
  minTempo: number;
  maxTempo: number;
}

export interface ProjectAssets {
  sourceSrt: string | null;
  translatedSrt: string | null;
  backgroundStem: string | null;
  vocalStem: string | null;
  previewOutput: string | null;
  finalOutput: string | null;
}

export interface CharacterBible {
  summary: string;
  relationships: string[];
  addressRules: string[];
}

export interface SceneContext {
  id: string;
  startIndex: number;
  endIndex: number;
  summary: string;
  tone: string;
  status: string;
}

export interface TranslationGuide {
  revision: number;
  synopsis: string;
  tone: string;
  userNotes: string;
  addressRules: string[];
  scenes: SceneContext[];
  preparedAtMs: number;
}

export interface TranslationMemoryStatus {
  enabled: boolean;
  count: number;
}

export interface DubbingProject {
  id: string;
  title: string;
  videoPath: string;
  previewPath: string | null;
  sourceLanguage: "ja" | "en";
  targetLanguage: "vi";
  profile: ProjectProfile;
  status: string;
  createdAtMs: number;
  updatedAtMs: number;
  glossary: GlossaryEntry[];
  speakers: SpeakerProfile[];
  cues: DialogueCue[];
  audioSettings: AudioSettings;
  assets: ProjectAssets;
  characterBible: CharacterBible;
  translationGuide: TranslationGuide;
  warnings: string[];
  ttsCacheVersion: number;
  lastJob: LongJobState | null;
}

export interface JobError {
  code: string;
  stage: string;
  message: string;
  cueId: string | null;
  cueIndex: number | null;
  voice: string | null;
  batch: number | null;
  exitCode: number | null;
  timedOut: boolean;
  elapsedMs: number;
  details: string | null;
}

export interface LongJobState {
  id: string;
  kind: "synthesize" | "render" | string;
  status: "running" | "completed" | "failed" | "cancelled" | "interrupted" | string;
  stage: string;
  total: number;
  processed: number;
  generated: number;
  reused: number;
  percent: number;
  message: string;
  currentCueIndex: number | null;
  startedAtMs: number;
  updatedAtMs: number;
  elapsedSeconds: number;
  etaSeconds: number | null;
  error: JobError | null;
}

export interface ProjectSummary {
  id: string;
  title: string;
  videoPath: string;
  sourceLanguage: "ja" | "en";
  profile: ProjectProfile;
  cueCount: number;
  updatedAtMs: number;
}

export interface ProjectProgress {
  jobId: string | null;
  kind: string | null;
  status: string;
  stage: string;
  current: number;
  total: number;
  processed: number;
  generated: number;
  reused: number;
  percent: number;
  message: string;
  currentCueIndex: number | null;
  elapsedSeconds: number;
  etaSeconds: number | null;
}

export interface SynthesizeResult {
  project: DubbingProject;
  generated: number;
  reused: number;
  skipped: number;
  warningCount: number;
  elapsedSeconds: number;
  job: LongJobState;
}

export interface RenderResult {
  project: DubbingProject;
  path: string;
  renderedCues: number;
  usedSeparation: boolean;
}
