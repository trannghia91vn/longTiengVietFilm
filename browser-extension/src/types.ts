export interface Region { x: number; y: number; width: number; height: number }
export interface Size { width: number; height: number }
export interface PlayerGeometry { viewport: Size; rect: Region; clip: Region }
export interface LocalGeometry extends PlayerGeometry { fullscreen: boolean }
export interface TargetTab { id: number; title: string; url: string }
export interface Clock {
  videoId: string;
  sourceKey: string;
  mediaTime: number;
  duration: number | null;
  rate: number;
  paused: boolean;
  seeking: boolean;
  waiting: boolean;
  ended: boolean;
  readyState: number;
  epochMs: number;
  frameMediaTime: number | null;
  frameEpochMs: number | null;
  layout: string;
  width: number;
  height: number;
  geometry?: PlayerGeometry;
}
export interface VideoSource {
  id: string;
  frameId: number;
  documentId: string;
  origin: string;
  clock: Clock;
  seenAt: number;
}
export interface Sample {
  sessionId: string;
  sequence: number;
  segment: number;
  capturedEpochMs: number;
  estimatedVideoTimeMs: number;
  uncertaintyMs: number;
  roundTripMs: number;
  captureSize: Size;
  region: Region;
  clock: Clock;
  manual: boolean;
  image: Blob;
  smart?: import("./smart-capture").SmartDecision;
}
export interface CaptureObservation extends Omit<Sample, "image" | "sequence" | "smart"> {
  observationSeq: number;
  imageSequence: number;
  smart: import("./smart-capture").SmartDecision;
}
export interface SessionEvent { kind: string; epochMs: number; mediaTimeMs: number | null; detail: string }
export interface CaptureSession {
  schemaVersion: 1 | 2;
  captureMode?: "smart" | "all";
  observationCount?: number;
  fallbackCount?: number;
  id: string;
  title: string;
  url: string;
  createdAt: number;
  endedAt: number | null;
  status: "recording" | "paused" | "stopped" | "interrupted";
  source: Pick<VideoSource, "id" | "frameId" | "documentId" | "origin">;
  region: Region;
  sampleCount: number;
  imageBytes: number;
  events: SessionEvent[];
  native?: NativeSessionState;
}
export interface NativeSessionState {
  messageSeq: number;
  ackSequence: number;
  savedSamples: number;
  savedBytes: number;
  pendingBytes: number;
  finalized: boolean;
  sessionPath: string | null;
  error: string | null;
}
export interface NativeConnection { connected: boolean; error: string | null; appData: string | null; hostVersion: string | null; capabilities?: string[] }
export interface NativeAck { ackSequence: number; savedSamples: number; savedBytes: number; finalized: boolean; sessionPath: string }
export interface OutboxEntry {
  sessionId: string;
  messageSeq: number;
  operation: "begin" | "appendSample" | "appendEvent" | "appendObservations" | "finalize";
  payloadJson: string;
  image?: Blob;
  byteLength: number;
}
export interface CaptureView {
  target: TargetTab | null;
  connected: boolean;
  preview: string | null;
  previewSize: Size | null;
  region: Region | null;
  source: VideoSource | null;
  session: CaptureSession | null;
  clock: Clock | null;
  error: string | null;
  busy: boolean;
  captureMode?: "smart" | "all";
  detectorState?: "stable" | "fallback" | "observing";
  lastObservationAt?: number | null;
  playerGeometry?: PlayerGeometry | null;
}
export interface RecorderView extends CaptureView {
  sources: VideoSource[];
  frameOrigins: string[];
  topLayout: string | null;
  native?: NativeConnection;
  navigationAllowed?: boolean;
}
export type Command =
  | { action: "state" }
  | { action: "source"; source: VideoSource }
  | { action: "region"; region: Region }
  | { action: "capture-mode"; mode: "smart" | "all" }
  | { action: "start" }
  | { action: "pause" }
  | { action: "resume" }
  | { action: "stop"; reason?: string }
  | { action: "snapshot" }
  | { action: "scan" }
  | { action: "open"; target: TargetTab; streamId: string; captureSize: Size };
export interface OcrStatus {
  sessionId: string;
  state: "not-started" | "queued" | "processing" | "interrupted" | "cancelled" | "error" | "partial" | "no-text";
  committed: boolean;
  processedSamples?: number;
  totalSamples?: number;
  cueCount?: number;
  reviewCount?: number;
  error?: string | null;
  preview?: string;
}
export type NativeCommand = { action: "native-retry" } | { action: "open-folder"; sessionId: string } | { action: "ocr-start"; sessionId: string } | { action: "ocr-status"; sessionId: string } | { action: "ocr-cancel"; sessionId: string };
export type Reply<T = unknown> = { ok: true; value: T } | { ok: false; error: string };
export const MAX_SESSION_BYTES = 128 * 1024 * 1024;
export const SAMPLE_INTERVAL_MS = 100;
export const epochNow = () => performance.timeOrigin + performance.now();
