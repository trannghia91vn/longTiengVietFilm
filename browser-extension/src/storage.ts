import { emptyNative } from "./native-protocol";
import { MAX_SESSION_BYTES, type CaptureSession, type CaptureObservation, type NativeAck, type OutboxEntry, type Sample } from "./types";

let connection: Promise<IDBDatabase> | undefined;
const CONTROL_RESERVE_BYTES = 1024 * 1024;
function database(): Promise<IDBDatabase> {
  return connection ??= new Promise((resolve, reject) => {
    const request = indexedDB.open("long-tieng-capture", 3);
    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains("sessions")) db.createObjectStore("sessions", { keyPath: "id" });
      if (!db.objectStoreNames.contains("samples")) {
        const samples = db.createObjectStore("samples", { keyPath: ["sessionId", "sequence"] });
        samples.createIndex("sessionId", "sessionId");
      }
      if (!db.objectStoreNames.contains("outbox")) db.createObjectStore("outbox", { keyPath: ["sessionId", "messageSeq"] });
      if (!db.objectStoreNames.contains("nativeSamples")) db.createObjectStore("nativeSamples", { keyPath: ["sessionId", "sequence"] });
      if (!db.objectStoreNames.contains("previews")) db.createObjectStore("previews", { keyPath: "sessionId" });
      if (!db.objectStoreNames.contains("observations")) db.createObjectStore("observations", { keyPath: ["sessionId", "observationSeq"] });
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}
function completed(transaction: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    transaction.oncomplete = () => resolve();
    transaction.onabort = () => reject(transaction.error ?? new Error("Khong luu duoc du lieu."));
    transaction.onerror = () => reject(transaction.error);
  });
}
function result<T>(request: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => { request.onsuccess = () => resolve(request.result); request.onerror = () => reject(request.error); });
}
function payload(session: CaptureSession) {
  const { native: _native, ...capture } = session;
  return capture;
}
function entry(session: CaptureSession, operation: OutboxEntry["operation"], value: unknown, image?: Blob): OutboxEntry {
  const payloadJson = JSON.stringify(value);
  return { sessionId: session.id, messageSeq: session.native!.messageSeq + 1, operation, payloadJson, byteLength: new TextEncoder().encode(payloadJson).length + (image?.size ?? 0), ...(image ? { image } : {}) };
}
export async function readSession(id: string): Promise<CaptureSession | undefined> {
  const db = await database();
  return result(db.transaction("sessions").objectStore("sessions").get(id));
}
export async function saveSession(session: CaptureSession): Promise<CaptureSession> {
  const db = await database();
  const tx = db.transaction(["sessions", "outbox"], "readwrite");
  const done = completed(tx);
  // Capture and ACK writers merge against the latest row inside the same transaction.
  const stored = await result<CaptureSession | undefined>(tx.objectStore("sessions").get(session.id));
  const next = { ...session, ...(session.native ? { native: { ...(stored?.native ?? session.native) } } : {}) };
  if (next.native) {
    if (next.native.finalized) { tx.abort(); await done; throw new Error("Phiên đã lưu xong; tạo phiên mới."); }
    const operation = !stored ? "begin" : ["stopped", "interrupted"].includes(next.status) ? "finalize" : "appendEvent";
    const queued = entry(next, operation, payload(next));
    const sessions = await result<CaptureSession[]>(tx.objectStore("sessions").getAll());
    if (queued.byteLength > CONTROL_RESERVE_BYTES / 4 || sessions.reduce((bytes, current) => bytes + (current.native?.pendingBytes ?? 0), 0) + queued.byteLength > MAX_SESSION_BYTES) { tx.abort(); try { await done; } catch { /* Leave the last committed checkpoint untouched. */ } throw new Error("Bộ đệm metadata đầy; kết nối lại helper trước khi tiếp tục."); }
    next.native.messageSeq = queued.messageSeq;
    next.native.pendingBytes += queued.byteLength;
    tx.objectStore("outbox").add(queued);
  }
  tx.objectStore("sessions").put(next);
  await done;
  return next;
}
export async function saveSample(session: CaptureSession, sample: Sample): Promise<CaptureSession> {
  const db = await database();
  const tx = db.transaction(["sessions", "samples", "outbox", "nativeSamples", "previews"], "readwrite");
  const done = completed(tx);
  const stored = await result<CaptureSession | undefined>(tx.objectStore("sessions").get(session.id));
  const next = { ...session, ...(session.native ? { native: { ...(stored?.native ?? emptyNative()) } } : {}) };
  if (next.native) {
    if (next.native.finalized || !stored || stored.sampleCount + 1 !== sample.sequence) { tx.abort(); await done; throw new Error("Sample sequence không khớp."); }
    const { image, ...metadata } = sample;
    const queued = entry(next, "appendSample", metadata, image);
    const sessions = await result<CaptureSession[]>(tx.objectStore("sessions").getAll());
    if (sessions.reduce((bytes, current) => bytes + (current.native?.pendingBytes ?? 0), 0) + queued.byteLength > MAX_SESSION_BYTES - CONTROL_RESERVE_BYTES) { tx.abort(); try { await done; } catch { /* Abort leaves every previously queued image intact. */ } throw new Error("Bộ đệm chưa lưu đạt 128 MiB. Tạm dừng thu; kết nối lại helper rồi tiếp tục."); }
    next.native.messageSeq = queued.messageSeq;
    next.native.pendingBytes += queued.byteLength;
    tx.objectStore("outbox").add(queued);
    tx.objectStore("nativeSamples").add(metadata);
    tx.objectStore("previews").put(sample);
  } else { tx.objectStore("samples").add(sample); }
  tx.objectStore("sessions").put(next);
  await done;
  return next;
}
export async function listSessions(): Promise<CaptureSession[]> {
  const db = await database();
  const sessions = await result<CaptureSession[]>(db.transaction("sessions").objectStore("sessions").getAll());
  return sessions.sort((a, b) => b.createdAt - a.createdAt);
}
export async function saveObservations(session: CaptureSession, observations: CaptureObservation[]): Promise<CaptureSession> {
  if (!session.native || session.schemaVersion !== 2 || !observations.length || observations.length > 5) throw new Error("Batch observation không hợp lệ.");
  const db = await database();
  const tx = db.transaction(["sessions", "outbox", "observations", "nativeSamples"], "readwrite");
  const done = completed(tx);
  try {
    const stored = await result<CaptureSession>(tx.objectStore("sessions").get(session.id));
    if (!stored?.native || stored.native.finalized) throw new Error("Phiên observation đã kết thúc.");
    let count = stored.observationCount ?? 0;
    for (const observation of observations) {
      if (observation.sessionId !== session.id || observation.observationSeq !== ++count) throw new Error("Observation sequence không liên tục.");
      const image = await result<Omit<Sample, "image"> | undefined>(tx.objectStore("nativeSamples").get([session.id, observation.imageSequence]));
      if (!image || image.segment !== observation.segment || image.smart?.group !== observation.smart.group || JSON.stringify(image.region) !== JSON.stringify(observation.region)) throw new Error("Observation thiếu ảnh cùng nhóm/vùng.");
    }
    const next = { ...stored, ...session, native: { ...stored.native }, observationCount: count, fallbackCount: (stored.fallbackCount ?? 0) + observations.filter(o => o.smart.role === "fallback").length };
    const queued = entry(next, "appendObservations", { sessionId: session.id, observations });
    const sessions = await result<CaptureSession[]>(tx.objectStore("sessions").getAll());
    if (queued.byteLength > CONTROL_RESERVE_BYTES / 4 || sessions.reduce((n, s) => n + (s.native?.pendingBytes ?? 0), 0) + queued.byteLength > MAX_SESSION_BYTES - CONTROL_RESERVE_BYTES) throw new Error("Bộ đệm observation đầy; tạm dừng thu và kết nối lại helper.");
    next.native.messageSeq = queued.messageSeq; next.native.pendingBytes += queued.byteLength;
    tx.objectStore("outbox").add(queued);
    for (const observation of observations) tx.objectStore("observations").add(observation);
    tx.objectStore("sessions").put(next); await done; return next;
  } catch (failure) { try { tx.abort(); } catch { /* Already aborted. */ } try { await done; } catch { /* Preserve committed outbox. */ } throw failure; }
}
export async function loadSamples(sessionId: string): Promise<Sample[]> {
  if ((await readSession(sessionId))?.native) throw new Error("Ảnh phiên native nằm trong app data. Mở thư mục bản ghi để xem.");
  const db = await database();
  const samples = await result<Sample[]>(db.transaction("samples").objectStore("samples").index("sessionId").getAll(sessionId));
  return samples.sort((a, b) => a.sequence - b.sequence);
}
export async function lastSample(sessionId: string): Promise<Sample | null> {
  const db = await database();
  const preview = await result<Sample | undefined>(db.transaction("previews").objectStore("previews").get(sessionId));
  if (preview) return preview;
  const cursor = await result(db.transaction("samples").objectStore("samples").index("sessionId").openCursor(IDBKeyRange.only(sessionId), "prev"));
  return cursor ? cursor.value as Sample : null;
}
export async function recoverSessions(activeId: string | null): Promise<void> {
  for (const session of await listSessions()) {
    if (session.id !== activeId && ["recording", "paused"].includes(session.status)) {
      session.status = "interrupted";
      session.endedAt = Date.now();
      session.events.push({ kind: "interrupted", epochMs: Date.now(), mediaTimeMs: null, detail: "Capture bi ngat; anh da luu van duoc giu." });
      await saveSession(session);
    }
  }
}
export async function nextOutbox(sessionId: string): Promise<OutboxEntry | null> {
  const db = await database();
  const cursor = await result(db.transaction("outbox").objectStore("outbox").openCursor(IDBKeyRange.bound([sessionId, 0], [sessionId, Number.MAX_SAFE_INTEGER])));
  return cursor ? cursor.value as OutboxEntry : null;
}
export async function acknowledge(queued: OutboxEntry, ack: NativeAck): Promise<void> {
  const db = await database();
  const tx = db.transaction(["sessions", "outbox"], "readwrite");
  const done = completed(tx);
  const session = await result<CaptureSession>(tx.objectStore("sessions").get(queued.sessionId));
  const pending = await result<OutboxEntry | undefined>(tx.objectStore("outbox").get([queued.sessionId, queued.messageSeq]));
  if (!pending) { await done; return; }
  const native = session?.native;
  if (!native || ![ack.ackSequence, ack.savedSamples, ack.savedBytes].every(Number.isSafeInteger) || ack.ackSequence < queued.messageSeq || ack.ackSequence > native.messageSeq || ack.savedSamples > session.sampleCount || ack.savedBytes > session.imageBytes || ack.savedSamples < 0 || ack.savedBytes < 0 || typeof ack.finalized !== "boolean" || typeof ack.sessionPath !== "string") { tx.abort(); try { await done; } catch { /* Preserve outbox on invalid ACK. */ } throw new Error("ACK helper không hợp lệ; giữ dữ liệu chưa lưu."); }
  native.ackSequence = Math.max(native.ackSequence, ack.ackSequence);
  native.savedSamples = ack.savedSamples;
  native.savedBytes = ack.savedBytes;
  native.sessionPath = ack.sessionPath;
  native.pendingBytes = Math.max(0, native.pendingBytes - pending.byteLength);
  native.finalized = ack.finalized && native.pendingBytes === 0 && ack.ackSequence === native.messageSeq;
  native.error = null;
  tx.objectStore("outbox").delete([queued.sessionId, queued.messageSeq]);
  tx.objectStore("sessions").put(session);
  await done;
}
export async function setNativeError(id: string, error: string | null): Promise<void> {
  const db = await database();
  const tx = db.transaction("sessions", "readwrite");
  const done = completed(tx);
  const session = await result<CaptureSession | undefined>(tx.objectStore("sessions").get(id));
  if (session?.native) { session.native.error = error; tx.objectStore("sessions").put(session); }
  await done;
}
