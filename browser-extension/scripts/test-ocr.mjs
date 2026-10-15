import assert from "node:assert/strict";
import { spawn, spawnSync } from "node:child_process";
import { once } from "node:events";
import { mkdir, mkdtemp, readFile, writeFile, copyFile, realpath, stat, appendFile } from "node:fs/promises";
import { createHash, randomUUID } from "node:crypto";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("../../", import.meta.url));
const temp = await realpath(await mkdtemp(path.join(os.tmpdir(), "long-tieng-ocr-test-")));
const home = path.join(temp, "home");
const data = path.join(home, "Library/Application Support/com.basushi.longtieng");
const installation = path.join(temp, "host");
const frames = path.join(temp, "frames");
const origin = `chrome-extension://${"a".repeat(32)}/`;
const id = randomUUID();
await mkdir(installation, { recursive: true });
await mkdir(frames);
await mkdir(path.join(data, "models/ocr-vie-fast"), { recursive: true });
const binary = path.join(installation, "long-tieng-capture-host");
await copyFile(path.join(root, "browser-extension/native-host/target/release/long-tieng-capture-host"), binary);
await copyFile(path.join(root, "src-tauri/engines/tesseract"), path.join(installation, "tesseract"));
await copyFile(path.join(root, "src-tauri/engines/vision-ocr"), path.join(installation, "vision-ocr"));
await copyFile(path.join(root, "browser-extension/native-host/runtime/vie.traineddata"), path.join(data, "models/ocr-vie-fast/vie.traineddata"));
await writeFile(path.join(installation, "host-config.json"), JSON.stringify({ allowedOrigin: origin, autoOcr: true }));
const encode = spawnSync("ffmpeg", ["-hide_banner", "-loglevel", "error", "-i", path.join(root, "browser-extension/tests/media/clock.webm"), "-vf", "crop=960:110:0:400", "-fps_mode", "passthrough", path.join(frames, "%08d.png")], { encoding: "utf8" });
assert.equal(encode.status, 0, encode.stderr);
let incoming = Buffer.alloc(0);
const pending = new Map();
let stderr = "";
function connectHost() {
  const child = spawn(binary, [origin], { env: { ...process.env, HOME: home }, stdio: ["pipe", "pipe", "pipe"] });
  child.stderr.on("data", bytes => { stderr += bytes; });
  child.stdout.on("data", bytes => {
  incoming = Buffer.concat([incoming, bytes]);
  while (incoming.length >= 4 && incoming.length >= incoming.readUInt32LE(0) + 4) {
    const size = incoming.readUInt32LE(0);
    assert.ok(size < 1048576);
    const response = JSON.parse(incoming.subarray(4, size + 4));
    incoming = incoming.subarray(size + 4);
    const resolve = pending.get(response.requestId); pending.delete(response.requestId); resolve?.(response);
  }
});
  return child;
}
let host = connectHost();
function request(operation, payload, image) {
  const requestId = randomUUID();
  const message = { version: 1, requestId, operation, sessionId: id };
  if (payload) {
    message.messageSeq = ++sequence;
    message.payloadJson = JSON.stringify(payload);
    message.checksum = createHash("sha256").update(`${operation}\n${id}\n${sequence}\n${message.payloadJson}`).update(image ?? Buffer.alloc(0)).digest("hex");
    if (image) message.imageBase64 = image.toString("base64");
  }
  const bytes = Buffer.from(JSON.stringify(message)); const size = Buffer.alloc(4); size.writeUInt32LE(bytes.length);
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => { pending.delete(requestId); reject(new Error(`Native timeout: ${operation}`)); }, 20000);
    pending.set(requestId, response => { clearTimeout(timer); response.ok ? resolve(response.value) : reject(new Error(response.error)); });
    host.stdin.write(Buffer.concat([size, bytes]));
  });
}
async function until(test, timeoutMs = 90000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) { const value = await test(); if (value) return value; await new Promise(resolve => setTimeout(resolve, 200)); }
  throw new Error(`Timed out waiting for OCR; ${stderr}`);
}
let sequence = 0;
const epoch = Date.now();
const region = { x: 0, y: 400 / 540, width: 1, height: 110 / 540 };
const metadata = { schemaVersion: 1, id, title: "OCR native integration fixture", url: "http://localhost/fixture", createdAt: epoch, endedAt: null, status: "recording", source: { id: "doc:video", frameId: 1, documentId: "doc", origin: "http://localhost" }, region, sampleCount: 0, imageBytes: 0, events: [] };
const session = path.join(data, "ocr-sessions", id);
try {
  await request("begin", metadata);
  for (let i = 1; i <= 200; i++) {
    const image = await readFile(path.join(frames, `${String(i).padStart(8, "0")}.png`));
    const time = (i - 1) * 100;
    const clock = { videoId: "video", sourceKey: "fixture", mediaTime: time / 1000, duration: 20, rate: 1, paused: false, seeking: false, waiting: false, ended: false, readyState: 4, epochMs: epoch + time, frameMediaTime: null, frameEpochMs: null, layout: "fixed", width: 960, height: 540 };
    await request("appendSample", { sessionId: id, sequence: i, segment: 0, capturedEpochMs: epoch + time, estimatedVideoTimeMs: time, uncertaintyMs: 0, roundTripMs: 0, captureSize: { width: 960, height: 540 }, region, clock, manual: false }, image);
    metadata.sampleCount++; metadata.imageBytes += image.length;
  }
  const started = Date.now();
  const finalized = await request("finalize", { ...metadata, endedAt: epoch + 20000, status: "stopped" });
  assert.equal(finalized.finalized, true);
  await until(async () => { const s = await request("ocrStatus"); return s.processedSamples >= 10 && s.state === "processing" ? s : null; });
  await request("ocrCancel");
  const stopped = await until(async () => { const s = await request("ocrStatus"); return s.state === "cancelled" ? s : null; });
  assert.equal(stopped.committed, false);
  const before = stopped.processedSamples;
  const resumed = await request("ocrStart"); assert.ok(["queued", "processing"].includes(resumed.state));
  const running = await until(async () => { const s = await request("ocrStatus"); return s.state === "processing" && s.processedSamples >= 20 ? s : null; });
  assert.ok(Number.isInteger(running.workerPid) && running.workerPid > 0 && running.workerPid !== host.pid);
  process.kill(-running.workerPid, "SIGKILL");
  await until(async () => { const s = await request("ocrStatus"); return s.state === "interrupted" ? s : null; });
  const observationPath=path.join(session,"ocr-work",running.observationRevision,"observations.ndjson");
  await appendFile(observationPath,'{"partial-crash-tail"');
  await request("ocrStart");
  await until(async () => { const s=await request("ocrStatus");return s.state === "processing" && s.processedSamples >= 20 ? s : null; });
  const hostExit = once(host, "exit"); host.stdin.end(); await hostExit;
  const detached = await until(async () => { const s = JSON.parse(await readFile(path.join(session, "ocr.json"), "utf8")); if (s.state === "error") throw new Error(s.error); return s.committed ? s : null; });
  host = connectHost();
  const complete = await request("ocrStatus"); assert.equal(complete.revision, detached.revision);
  assert.equal(complete.processedSamples, 200);
  assert.equal(complete.wholeEpisodeVerified, false);
  const again = await request("ocrStart"); assert.equal(again.revision, complete.revision);
  const committed = JSON.parse(await readFile(path.join(session, "ocr.json"), "utf8"));
  const cues = JSON.parse(await readFile(path.join(session, "cues.json"), "utf8")).cues;
  const srt = await readFile(path.join(session, "subtitles.vi.srt"), "utf8");
  assert.equal(createHash("sha256").update(srt).digest("hex"), committed.srtHash);
  const rerun = spawnSync(binary, ["--ocr-reprocess", id], {env:{...process.env,HOME:home},encoding:"utf8",timeout:90000});
  assert.equal(rerun.status,0,rerun.stderr);
  assert.equal(JSON.parse(rerun.stdout).revision,committed.revision);
  assert.equal(await readFile(path.join(session,"ocr-revisions",committed.revision,"subtitles.vi.srt"),"utf8"),srt);
  assert.ok(cues.length > 0);
  const checkpoint=(await readFile(observationPath,"utf8")).trim().split("\n").map(JSON.parse);
  assert.equal(checkpoint.length,200);checkpoint.forEach((row,i)=>assert.equal(row.observation.sequence,i+1));
  for (const cue of cues) { assert.ok(cue.endMs > cue.startMs); assert.equal(cue.reviewed, false); }
  const truth = [
    { start: 1000, end: 4000, text: "Xin chào, đây là phụ đề tiếng Việt." },
    { start: 5000, end: 8000, text: "Một câu có dấu: ă â ê ô ơ ư đ.\nDòng thứ hai giữ đúng thời gian." },
    { start: 9000, end: 12000, text: "Câu mới sau khoảng lặng." },
    { start: 13000, end: 16000, text: "Xin chào, đây là phụ đề tiếng Việt." },
    { start: 17000, end: 19000, text: "Kết thúc đoạn kiểm thử." },
  ];
  function distance(a, b) {
    a = [...a.normalize("NFC")]; b = [...b.normalize("NFC")]; let row = b.map((_, i) => i + 1); row.unshift(0);
    for (let i = 0; i < a.length; i++) { const next = [i + 1]; for (let j = 0; j < b.length; j++) next.push(Math.min(next[j] + 1, row[j + 1] + 1, row[j] + (a[i] === b[j] ? 0 : 1))); row = next; }
    return row.at(-1);
  }
  const measures = truth.map(t => {
    const overlapping = cues.filter(c => c.startMs < t.end && c.endMs > t.start);
    const best = overlapping.sort((a, b) => distance(a.text, t.text) - distance(b.text, t.text))[0];
    return { truth: t.text, recognized: best?.text ?? "", overlappingCues: overlapping.length, cer: distance(best?.text ?? "", t.text) / [...t.text].length, startErrorMs: best ? Math.abs(best.startMs - t.start) : null, endErrorMs: best ? Math.abs(best.endMs - t.end) : null };
  });
  const report = { session: id, directory: session, resumedAfterSamples: before, workerKilledAfterSamples: running.processedSamples, partialJournalTailRecovered:true, completedAfterNativeHostExit: true, reprocessPreservedCanonical:true, totalSamples: 200, elapsedMs: Date.now() - started, cueCount: cues.length, reviewCount: complete.reviewCount, sourceBytes: metadata.imageBytes, measures, captureUnchanged: (await stat(path.join(session, "frames/00000001.png"))).size > 0, revision: complete.revision };
  const evidence = path.join(root, "browser-extension/tests/results/ocr-integration"); await mkdir(evidence, { recursive: true });
  await writeFile(path.join(evidence, "report.json"), JSON.stringify(report, null, 2));
  await copyFile(path.join(session, "subtitles.vi.srt"), path.join(evidence, "subtitles.vi.srt"));
  await copyFile(path.join(frames, "00000190.png"), path.join(evidence, "crop-190.png"));
  console.log(JSON.stringify(report, null, 2));
  assert.equal(cues.length,truth.length,"Clean fixture must not invent or split cues");
  assert.ok(measures.every(m=>m.cer===0 && m.startErrorMs<=200 && m.endErrorMs<=200),"Clean fixture OCR/text/timing quality failed; inspect report.json");
} finally {
  host.stdin.end();
  const state=await readFile(path.join(session,"ocr.json"),"utf8").then(JSON.parse).catch(()=>null);
  if(state && !state.committed && ["queued","processing"].includes(state.state)) {
    await writeFile(path.join(session,"ocr-cancel.json"),JSON.stringify({jobId:state.jobId}));
    await until(async()=>{const s=JSON.parse(await readFile(path.join(session,"ocr.json"),"utf8"));return !["queued","processing"].includes(s.state)?s:null;},25000).catch(()=>{});
  }
}
