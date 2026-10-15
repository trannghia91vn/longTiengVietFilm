import "fake-indexeddb/auto";
import test from "node:test";
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { acknowledge, lastSample, listSessions, loadSamples, nextOutbox, readSession, recoverSessions, saveSample, saveSession } from "../src/storage";
import { emptyNative, nativeRequest } from "../src/native-protocol";
import { MAX_SESSION_BYTES, type CaptureSession, type Sample } from "../src/types";

const ID = "9c4212da-a38b-4452-bb9c-ebfb5d55c66c";
function session(id = ID): CaptureSession { return { schemaVersion: 1, id, title: "native", url: "http://localhost/movie", createdAt: 1, endedAt: null, status: "recording", source: { id: "doc:video", documentId: "doc", frameId: 1, origin: "http://localhost" }, region: { x: 0, y: 0, width: 1, height: 1 }, sampleCount: 0, imageBytes: 0, events: [], native: emptyNative() }; }
function sample(sequence: number, image = new Blob(["png"])): Sample { return { sessionId: ID, sequence, segment: 0, capturedEpochMs: 1, estimatedVideoTimeMs: 1, uncertaintyMs: 100, roundTripMs: 1, captureSize: { width: 2, height: 2 }, region: { x: 0, y: 0, width: 1, height: 1 }, manual: false, image, clock: { videoId: "video", sourceKey: "movie", mediaTime: .001, duration: 20, rate: 1, paused: false, seeking: false, waiting: false, ended: false, readyState: 4, epochMs: 1, frameMediaTime: null, frameEpochMs: null, layout: "same", width: 2, height: 2 } }; }

// Seed the actual v1 shape before storage lazily opens v2.
await new Promise<void>((resolve, reject) => {
  const request = indexedDB.open("long-tieng-capture", 1);
  request.onupgradeneeded = () => {
    request.result.createObjectStore("sessions", { keyPath: "id" });
    request.result.createObjectStore("samples", { keyPath: ["sessionId", "sequence"] }).createIndex("sessionId", "sessionId");
  };
  request.onerror = () => reject(request.error);
  request.onsuccess = () => {
    const db = request.result;
    const tx = db.transaction(["sessions", "samples"], "readwrite");
    const { native: _native, ...legacy } = session("legacy");
    tx.objectStore("sessions").put({ ...legacy, status: "stopped", sampleCount: 1, imageBytes: 3 });
    tx.objectStore("samples").put({ ...sample(1), sessionId: "legacy" });
    tx.oncomplete = () => { db.close(); resolve(); };
    tx.onerror = () => reject(tx.error);
  };
});

test("v1 migration preserves legacy archives and blobs", async () => {
  assert.equal((await loadSamples("legacy")).length, 1);
  assert.equal(await (await lastSample("legacy"))!.image.text(), "png");
  assert.equal((await readSession("legacy"))?.native, undefined);
});
test("native outbox is durable before send and ACK only frees its own blob", async () => {
  let current = await saveSession(session());
  const begin = (await nextOutbox(ID))!;
  assert.equal(begin.operation, "begin");
  current = await saveSample({ ...current, sampleCount: 1, imageBytes: 3 }, sample(1));
  await acknowledge(begin, { ackSequence: 1, savedSamples: 0, savedBytes: 0, finalized: false, sessionPath: "/app-data/session" });
  const frame = (await nextOutbox(ID))!;
  assert.equal(frame.messageSeq, 2);
  assert.equal(await frame.image!.text(), "png");
  const operations = [
    saveSample({ ...current, sampleCount: 2, imageBytes: 6 }, sample(2)),
    acknowledge(frame, { ackSequence: 2, savedSamples: 1, savedBytes: 3, finalized: false, sessionPath: "/app-data/session" }),
  ];
  await Promise.all(operations);
  const stored = (await readSession(ID))!;
  assert.equal(stored.sampleCount, 2);
  assert.equal(stored.native?.savedSamples, 1);
  assert.equal((await nextOutbox(ID))?.messageSeq, 3);
  assert.equal((await lastSample(ID))?.sequence, 2);
  await assert.rejects(loadSamples(ID));
});
test("invalid ACK preserves queue; duplicate sample rolls back metadata", async () => {
  const queued = (await nextOutbox(ID))!;
  await assert.rejects(acknowledge(queued, { ackSequence: 9999, savedSamples: 9999, savedBytes: 9999, finalized: true, sessionPath: "/app-data/session" }));
  const before = (await readSession(ID))!;
  await assert.rejects(saveSample({ ...before, sampleCount: 999 }, sample(2)));
  assert.equal((await readSession(ID))!.sampleCount, 2);
  assert.equal((await nextOutbox(ID))?.messageSeq, 3);
  await acknowledge(queued, { ackSequence: 3, savedSamples: 2, savedBytes: 6, finalized: false, sessionPath: "/app-data/session" });
});
test("128 MiB caps unacknowledged data, not cumulative capture bytes", async () => {
  const current = (await readSession(ID))!;
  const previousBytes = current.imageBytes;
  const overflow = sample(3, new Blob([new Uint8Array(MAX_SESSION_BYTES)]));
  await assert.rejects(saveSample({ ...current, sampleCount: 3, imageBytes: previousBytes + overflow.image.size }, overflow), /128 MiB/);
  assert.equal((await readSession(ID))?.sampleCount, 2);
  const ordinary = sample(3);
  const stored = await saveSample({ ...current, sampleCount: 3, imageBytes: previousBytes + 3 }, ordinary);
  assert.equal(stored.native?.savedSamples, 2);
  assert.ok(stored.native!.pendingBytes < 4096);
});
test("crash recovery queues interruption/finalize and cannot report saved before drain", async () => {
  await recoverSessions(null);
  const current = (await readSession(ID))!;
  assert.equal(current.status, "interrupted");
  assert.equal(current.native?.finalized, false);
  assert.ok(current.events.some(event => event.kind === "interrupted"));
  let queued = (await nextOutbox(ID))!;
  await acknowledge(queued, { ackSequence: 4, savedSamples: 3, savedBytes: 9, finalized: false, sessionPath: "/app-data/session" });
  queued = (await nextOutbox(ID))!;
  assert.equal(queued.operation, "finalize");
  await acknowledge(queued, { ackSequence: queued.messageSeq, savedSamples: 3, savedBytes: 9, finalized: true, sessionPath: "/app-data/session" });
  assert.equal((await readSession(ID))?.native?.finalized, true);
  assert.equal(await nextOutbox(ID), null);
  await recoverSessions(null);
  assert.equal((await listSessions()).find(item => item.id === ID)?.events.filter(event => event.kind === "interrupted").length, 1);
});
test("wire checksum covers operation, identity, sequence, exact JSON and PNG bytes", async () => {
  const payloadJson = '{"text":"Tiếng Việt","number":1.0000001}';
  const entry = { sessionId: ID, messageSeq: 2, operation: "appendSample" as const, payloadJson, image: new Blob([new Uint8Array([1, 2, 3])]), byteLength: 3 };
  const request = await nativeRequest(entry);
  const expected = createHash("sha256").update(`appendSample\n${ID}\n2\n${payloadJson}`).update(new Uint8Array([1, 2, 3])).digest("hex");
  assert.equal(request.checksum, expected);
  assert.equal(request.imageBase64, "AQID");
});
test("a native session can exceed 128 MiB cumulatively while ACK drains the outbox", async () => {
  const id = "268ad9e7-7ff4-4d49-91e7-f6b4d126a285";
  let current = await saveSession(session(id));
  const image = new Blob([new Uint8Array(4 * 1024 * 1024)]);
  let queued = (await nextOutbox(id))!;
  await acknowledge(queued, { ackSequence: 1, savedSamples: 0, savedBytes: 0, finalized: false, sessionPath: "/app-data/session" });
  for (let sequence = 1; sequence <= 33; sequence++) {
    current = await saveSample({ ...current, sampleCount: sequence, imageBytes: sequence * image.size }, { ...sample(sequence, image), sessionId: id });
    queued = (await nextOutbox(id))!;
    await acknowledge(queued, { ackSequence: queued.messageSeq, savedSamples: sequence, savedBytes: sequence * image.size, finalized: false, sessionPath: "/app-data/session" });
  }
  const stored = (await readSession(id))!;
  assert.ok(stored.imageBytes > MAX_SESSION_BYTES);
  assert.equal(stored.native?.pendingBytes, 0);
  assert.equal(stored.native?.savedSamples, 33);
});
