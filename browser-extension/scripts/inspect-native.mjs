import assert from "node:assert/strict";
import { readFile, readdir, stat, mkdir, writeFile, copyFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import path from "node:path";

const directory = process.argv[2];
if (!directory) throw new Error("Usage: node browser-extension/scripts/inspect-native.mjs <session-directory> [evidence-directory]");
const manifest = JSON.parse(await readFile(path.join(directory, "session.json"), "utf8"));
const lines = (await readFile(path.join(directory, "journal.ndjson"), "utf8")).split("\n");
assert.equal(lines.pop(), "", "Uncommitted journal tail");
const records = lines.map(line => JSON.parse(line));
const samples = [];
const timeline = new Map();
let bytes = 0;
for (const [index, record] of records.entries()) {
  assert.equal(record.messageSeq, index + 1);
  const payload = JSON.parse(record.payloadJson);
  let image = Buffer.alloc(0);
  if (record.operation === "appendSample") {
    assert.equal(payload.sequence, samples.length + 1);
    assert.equal(payload.sessionId, manifest.id);
    assert.ok(Number.isFinite(payload.estimatedVideoTimeMs) && payload.estimatedVideoTimeMs >= 0);
    if (!payload.manual) {
      const previous = timeline.get(payload.segment);
      if (previous !== undefined) assert.ok(payload.estimatedVideoTimeMs >= previous - 50);
      timeline.set(payload.segment, payload.estimatedVideoTimeMs);
    }
    image = await readFile(path.join(directory, "frames", `${String(payload.sequence).padStart(8, "0")}.png`));
    assert.equal(image.length, record.imageBytes);
    assert.equal(createHash("sha256").update(image).digest("hex"), record.imageHash);
    bytes += image.length;
    samples.push(payload);
  }
  const digest = createHash("sha256").update(`${record.operation}\n${manifest.id}\n${record.messageSeq}\n${record.payloadJson}`).update(image).digest("hex");
  assert.equal(digest, record.checksum);
}
assert.equal(samples.length, manifest.sampleCount);
assert.equal(bytes, manifest.imageBytes);
assert.equal((await readdir(path.join(directory, "frames"))).filter(name => name.endsWith(".png")).length, samples.length);
assert.equal(records.length, manifest.nativeStore.ackSequence);
if (manifest.nativeStore.status === "captured") assert.equal(records.at(-1)?.operation, "finalize");
const latencies = samples.map(sample => sample.roundTripMs).sort((a, b) => a - b);
const summary = {
  sessionId: manifest.id, status: manifest.nativeStore.status, captureStatus: manifest.status,
  source: manifest.url, directory: path.resolve(directory), samples: samples.length, messageCount: records.length,
  sizeMiB: Number((bytes / 1048576).toFixed(2)), firstVideoTimeMs: samples[0]?.estimatedVideoTimeMs, lastVideoTimeMs: samples.at(-1)?.estimatedVideoTimeMs,
  firstCaptureEpochMs: samples[0]?.capturedEpochMs, lastCaptureEpochMs: samples.at(-1)?.capturedEpochMs,
  rttMedianMs: latencies[Math.floor(latencies.length / 2)], rttP95Ms: latencies[Math.floor(latencies.length * .95)],
  captureSize: samples[0]?.captureSize, region: manifest.region, segments: [...timeline.keys()], events: manifest.events,
  journalBytes: (await stat(path.join(directory, "journal.ndjson"))).size,
};
console.log(JSON.stringify(summary, null, 2));
if (process.argv[3]) {
  await mkdir(process.argv[3], { recursive: true });
  await writeFile(path.join(process.argv[3], "summary.json"), JSON.stringify(summary, null, 2));
  for (const sequence of new Set([1, Math.floor(samples.length / 2), samples.length])) {
    if (sequence > 0) await copyFile(path.join(directory, "frames", `${String(sequence).padStart(8, "0")}.png`), path.join(process.argv[3], `crop-${sequence}.png`));
  }
}
