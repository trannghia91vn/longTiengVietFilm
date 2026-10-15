import "fake-indexeddb/auto";
import test from "node:test";
import assert from "node:assert/strict";
import { lastSample, listSessions, loadSamples, recoverSessions, saveSample, saveSession } from "../src/storage";
import type { CaptureSession, Sample } from "../src/types";

function session(id: string, patch: Partial<CaptureSession> = {}): CaptureSession {
  return { schemaVersion: 1, id, title: id, url: "https://example.test/movie", createdAt: 1000, endedAt: null, status: "paused", source: { id: "video", frameId: 1, documentId: "document", origin: "https://example.test" }, region: { x: 0, y: .8, width: 1, height: .2 }, sampleCount: 0, imageBytes: 0, events: [], ...patch };
}
function sample(sessionId: string, sequence: number): Sample {
  return { sessionId, sequence, segment: 0, capturedEpochMs: 1000, estimatedVideoTimeMs: 2000, uncertaintyMs: 100, roundTripMs: 1, captureSize: { width: 100, height: 100 }, region: { x: 0, y: .8, width: 1, height: .2 }, manual: false, image: new Blob(["image"], { type: "image/png" }), clock: { videoId: "video", sourceKey: "movie", mediaTime: 2, duration: 20, rate: 1, paused: false, seeking: false, waiting: false, ended: false, readyState: 4, epochMs: 1000, frameMediaTime: 2, frameEpochMs: 1000, layout: "same", width: 100, height: 100 } };
}
test("sample and metadata commit together; duplicate sequence rolls both back", async () => {
  const initial = session("atomic");
  await saveSession(initial);
  const first = sample(initial.id, 1);
  await saveSample({ ...initial, sampleCount: 1, imageBytes: first.image.size }, first);
  await assert.rejects(saveSample({ ...initial, sampleCount: 999, imageBytes: 999 }, first));
  assert.equal((await listSessions()).find(item => item.id === initial.id)?.sampleCount, 1);
  const stored = await loadSamples(initial.id);
  assert.equal(stored.length, 1);
  assert.equal(await stored[0].image.text(), "image");
  assert.equal((await lastSample(initial.id))?.sequence, 1);
});
test("archive reads stay isolated and ordered by session and sequence", async () => {
  const initial = session("ordered", { createdAt: 2000 });
  await saveSession(initial);
  await saveSample({ ...initial, sampleCount: 1 }, sample(initial.id, 2));
  await saveSample({ ...initial, sampleCount: 2 }, sample(initial.id, 1));
  assert.deepEqual((await loadSamples(initial.id)).map(item => item.sequence), [1, 2]);
  assert.equal((await lastSample(initial.id))?.sequence, 2);
  assert.equal(await lastSample("missing"), null);
  assert.equal((await listSessions())[0].id, initial.id);
});
test("recovery marks inactive captures interrupted without losing saved images", async () => {
  await saveSession(session("active", { status: "recording" }));
  await saveSession(session("finished", { status: "stopped" }));
  await recoverSessions("active");
  const sessions = await listSessions();
  assert.equal(sessions.find(item => item.id === "active")?.status, "recording");
  assert.equal(sessions.find(item => item.id === "finished")?.status, "stopped");
  assert.equal(sessions.find(item => item.id === "atomic")?.status, "interrupted");
  assert.equal((await loadSamples("atomic")).length, 1);
  await recoverSessions(null);
  assert.equal((await listSessions()).find(item => item.id === "atomic")?.events.filter(event => event.kind === "interrupted").length, 1);
});
