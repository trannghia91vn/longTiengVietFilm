import assert from "node:assert/strict";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import { unzipSync, strFromU8 } from "fflate";
import path from "node:path";

const files = unzipSync(await readFile(process.argv[2]));
const capture = JSON.parse(strFromU8(files["capture.json"]));
assert.equal(capture.samples.length, capture.sampleCount);
const segments = new Map();
for (const [index, sample] of capture.samples.entries()) {
  assert.equal(sample.sequence, index + 1, "Non-contiguous sample sequence");
  assert.ok(files[sample.imagePath], `Missing crop ${sample.sequence}`);
  assert.ok(Number.isFinite(sample.estimatedVideoTimeMs) && sample.estimatedVideoTimeMs >= 0);
  assert.ok(sample.roundTripMs <= 500);
  assert.equal(sample.clock.videoId, capture.samples[0].clock.videoId);
  const previous = segments.get(sample.segment);
  if (previous !== undefined) assert.ok(sample.estimatedVideoTimeMs >= previous - 50, `Clock moved backward in segment ${sample.segment}`);
  segments.set(sample.segment, sample.estimatedVideoTimeMs);
}
const latencies = capture.samples.map(sample => sample.roundTripMs).sort((a, b) => a - b);
const summary = {
  sessionId: capture.id, status: capture.status, source: capture.url, samples: capture.sampleCount,
  sizeMiB: Number((capture.imageBytes / 1048576).toFixed(2)),
  firstVideoTimeMs: capture.samples[0]?.estimatedVideoTimeMs,
  lastVideoTimeMs: capture.samples.at(-1)?.estimatedVideoTimeMs,
  rttMedianMs: latencies[Math.floor(latencies.length / 2)],
  rttP95Ms: latencies[Math.floor(latencies.length * .95)],
  captureSize: capture.samples[0]?.captureSize, region: capture.region,
  events: capture.events,
};
console.log(JSON.stringify(summary, null, 2));
if (process.argv[3]) {
  await mkdir(process.argv[3], { recursive: true });
  await writeFile(path.join(process.argv[3], "summary.json"), JSON.stringify(summary, null, 2));
  const requested = process.argv[4]?.split(",").map(Number) ?? [];
  assert.ok(requested.every(sequence => Number.isInteger(sequence) && sequence > 0 && sequence <= capture.sampleCount), "Invalid crop selection");
  for (const sequence of new Set([1, Math.floor(capture.sampleCount / 2), capture.sampleCount, ...requested])) {
    const sample = capture.samples.find(sample => sample.sequence === sequence);
    if (sample) await writeFile(path.join(process.argv[3], `crop-${sequence}.png`), files[sample.imagePath]);
  }
}
