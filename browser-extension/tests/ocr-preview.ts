import { saveSession } from "../src/storage";
import type { CaptureSession, OcrStatus, RecorderView } from "../src/types";

const id = "8c882a56-11fb-4996-827b-c9fb2d57ce88";
const srt = await fetch("/tests/results/ocr-integration/subtitles.vi.srt").then(response => response.text());
const session: CaptureSession = { schemaVersion: 1, id, title: "OCR native integration fixture", url: "http://localhost/fixture", createdAt: 1791964800000, endedAt: 1791964820000, status: "stopped", source: { id: "doc:video", frameId: 1, documentId: "doc", origin: "http://localhost" }, region: { x: 0, y: .74, width: 1, height: .2 }, sampleCount: 200, imageBytes: 3205540, events: [] };
await saveSession(session);
session.native = { messageSeq: 202, ackSequence: 202, savedSamples: 200, savedBytes: 3205540, pendingBytes: 0, finalized: true, sessionPath: "/test-fixture/ocr-sessions", error: null };
const db = await new Promise<IDBDatabase>((resolve, reject) => { const request = indexedDB.open("long-tieng-capture", 2); request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); });
await new Promise<void>((resolve, reject) => { const tx = db.transaction("sessions", "readwrite"); tx.objectStore("sessions").put(session); tx.oncomplete = () => resolve(); tx.onerror = () => reject(tx.error); });
db.close();
const preview = await fetch("/tests/results/ocr-integration/crop-190.png").then(response => response.blob());
const previewDb = await new Promise<IDBDatabase>((resolve) => { const request=indexedDB.open("long-tieng-capture",2);request.onsuccess=()=>resolve(request.result); });
await new Promise<void>((resolve)=>{ const tx=previewDb.transaction("previews","readwrite");tx.objectStore("previews").put({sessionId:id,sequence:190,segment:0,estimatedVideoTimeMs:18900,roundTripMs:0,image:preview});tx.oncomplete=()=>resolve(); });
previewDb.close();
const view: RecorderView = { target: null, connected: false, preview: null, previewSize: null, region: null, source: null, session, clock: null, error: null, busy: false, sources: [], frameOrigins: [], topLayout: null, native: { connected: true, appData: "/test-fixture", hostVersion: "0.3.0", error: null } };
let ocr: OcrStatus = { sessionId: id, state: "partial", committed: true, cueCount: 5, reviewCount: 0, processedSamples: 200, totalSamples: 200, preview: srt };
const initial = new URL(location.href).searchParams.get("state");
if (initial === "processing") ocr = { ...ocr, state: "processing", committed: false, processedSamples: 90, preview: undefined };
if (initial === "error") ocr = { ...ocr, state: "error", committed: false, processedSamples: 90, preview: undefined, error: "OCR engine timeout; checkpoint còn nguyên." };
Object.defineProperty(window, "chrome", { configurable: true, value: { runtime: { id: "recorder-test-fixture", sendMessage: async ({ command }: { command: { action: string } }) => {
  if (command.action === "state") return { ok: true, value: view };
  if (command.action === "ocr-status") return { ok: true, value: ocr };
  if (command.action === "ocr-cancel") { ocr = { ...ocr, state: "cancelled", committed: false }; return { ok: true, value: null }; }
  if (command.action === "ocr-start") { ocr = { ...ocr, state: "processing", committed: false, error: null }; return { ok: true, value: ocr }; }
  return { ok: true, value: null };
} }, permissions: { contains: async () => true } } });
await import("../src/recorder");
