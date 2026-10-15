import assert from "node:assert/strict";
import { readFile, mkdir, writeFile, copyFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import path from "node:path";

const dir = path.resolve(process.argv[2] ?? "");
if (!process.argv[2]) throw new Error("Usage: node browser-extension/scripts/inspect-ocr.mjs <session-directory> [evidence-directory]");
const hash = bytes => createHash("sha256").update(bytes).digest("hex");
const manifest = JSON.parse(await readFile(path.join(dir, "ocr.json"), "utf8"));
assert.equal(manifest.committed, true);
assert.match(manifest.revision, /^[0-9a-f]{64}$/);
const revision = path.join(dir, "ocr-revisions", manifest.revision);
for (const [file, field] of [["subtitles.vi.srt", "srtHash"], ["subtitles.vi.txt", "txtHash"], ["cues.json", "cuesHash"]]) {
  const bytes = await readFile(path.join(revision, file));
  assert.equal(hash(bytes), manifest[field]);
  assert.equal(hash(await readFile(path.join(dir, file))), manifest[field]);
}
const document = JSON.parse(await readFile(path.join(revision, "cues.json"), "utf8"));
assert.equal(document.sessionId, manifest.sessionId);
assert.equal(document.revision, manifest.revision);
assert.equal(document.cues.length, manifest.cueCount);
assert.equal(document.coverage.wholeEpisodeVerified, false);
document.cues.forEach((cue, index) => {
  assert.equal(cue.index, index + 1); assert.ok(cue.endMs > cue.startMs);
  assert.equal(cue.text, cue.text.normalize("NFC")); assert.ok(cue.text.trim()); assert.equal(cue.reviewed, false);
  if (index) assert.ok(cue.startMs >= document.cues[index - 1].startMs);
});
const observationRevision = manifest.observationRevision ?? manifest.revision;
assert.match(observationRevision, /^[0-9a-f]{64}$/);
const rows = (await readFile(path.join(dir, "ocr-work", observationRevision, "observations.ndjson"), "utf8")).trim().split("\n").filter(Boolean).map(JSON.parse);
assert.equal(rows.length, manifest.totalSamples);
for (const [index, row] of rows.entries()) {
  assert.equal(row.observation.sequence, index + 1);
  const frame = await readFile(path.join(dir, "frames", `${String(index + 1).padStart(8, "0")}.png`));
  assert.equal(hash(frame), row.observation.imageHash);
}
const summary = { sessionId: manifest.sessionId, state: manifest.state, samples: rows.length, cueCount: document.cues.length, reviewCount: manifest.reviewCount, engine: manifest.engine, visionSystem: manifest.visionSystem, modelHash: manifest.modelHash, wholeEpisodeVerified: false, revision: manifest.revision, sourceJournalHash: manifest.sourceJournalHash, ranges: document.coverage.ranges, gaps: document.coverage.gaps, elapsedMs: manifest.completedAt - manifest.startedAt, directory: dir };
console.log(JSON.stringify(summary, null, 2));
if (process.argv[3]) {
  const evidence = path.resolve(process.argv[3]); await mkdir(evidence, { recursive: true });
  await writeFile(path.join(evidence, "summary.json"), JSON.stringify(summary, null, 2));
  for (const file of ["subtitles.vi.srt", "subtitles.vi.txt", "cues.json"]) await copyFile(path.join(revision, file), path.join(evidence, file));
}
