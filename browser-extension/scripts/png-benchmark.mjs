import { build } from "esbuild";
import { readFile, readdir, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { pathToFileURL, fileURLToPath } from "node:url";
import { performance } from "node:perf_hooks";
import path from "node:path";
import assert from "node:assert/strict";
import { unzlibSync } from "fflate";

const source = process.argv[2];
if (!source) throw new Error("Usage: node browser-extension/scripts/png-benchmark.mjs <finalized-session-directory>");
const manifest = JSON.parse(await readFile(path.join(source, "session.json"), "utf8"));
if (manifest.nativeStore?.status !== "captured") throw new Error("Benchmark needs a finalized capture; original files are read-only.");
const temporary = await mkdtemp(path.join(tmpdir(), "long-tieng-png-benchmark-"));
function pixels(png) {
  const parts = [];
  for (let offset = 8; offset < png.length;) {
    const length = png.readUInt32BE(offset);
    if (png.toString("ascii", offset + 4, offset + 8) === "IDAT") parts.push(png.subarray(offset + 8, offset + 8 + length));
    offset += length + 12;
  }
  return unzlibSync(Buffer.concat(parts));
}
try {
  const modulePath = path.join(temporary, "compression.mjs");
  await build({ entryPoints: [fileURLToPath(new URL("../src/png-compression.ts", import.meta.url))], bundle: true, platform: "node", format: "esm", outfile: modulePath });
  const { recompressPng } = await import(pathToFileURL(modulePath).href);
  const names = (await readdir(path.join(source, "frames"))).filter(name => /^\d{8}\.png$/.test(name)).sort();
  let before = 0, after = 0;
  const times = [];
  for (const name of names) {
    const original = await readFile(path.join(source, "frames", name));
    const started = performance.now();
    const result = original.length < 4096 ? original : recompressPng(original);
    times.push(performance.now() - started);
    assert.deepEqual(pixels(Buffer.from(result)), pixels(original), `${name}: filtered pixel bytes changed`);
    assert.deepEqual(Buffer.from(result).subarray(16, 29), original.subarray(16, 29), `${name}: PNG format changed`);
    before += original.length; after += result.length;
  }
  assert.equal(names.length, manifest.sampleCount);
  assert.equal(before, manifest.imageBytes);
  times.sort((a, b) => a - b);
  console.log(JSON.stringify({ sessionId: manifest.id, images: names.length, beforeBytes: before, afterBytes: after, reductionPercent: Number(((1 - after / before) * 100).toFixed(2)), compressionMs: { median: Number(times[Math.floor(times.length * .5)]?.toFixed(2)), p95: Number(times[Math.floor(times.length * .95)]?.toFixed(2)), total: Number(times.reduce((a, b) => a + b, 0).toFixed(2)) }, identicalFilteredPixels: true, sourceModified: false, note: "Node replay, not live Chrome sampling/RAM measurement; OCR call count unchanged by PNG repack." }, null, 2));
} finally { await rm(temporary, { recursive: true, force: true }); }
