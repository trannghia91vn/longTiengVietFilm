import { discontinuity, layoutMismatch, matchClock, regionPixels, validRegion } from "./core";
import { anchorToPlayer, regionFromAnchor } from "./player-geometry";
import { captureStalled } from "./capture-status";
import { compactPng } from "./png-encoder";
import { readSession, recoverSessions, saveObservations, saveSample, saveSession } from "./storage";
import { glyphSignature, SMART_RAM_BYTES, SmartDetector, type SmartDecision } from "./smart-capture";
import { emptyNative } from "./native-protocol";
import { epochNow, SAMPLE_INTERVAL_MS, type CaptureSession, type CaptureObservation, type CaptureView, type Clock, type Command, type Region, type Reply, type Sample, type VideoSource } from "./types";

const video = document.querySelector<HTMLVideoElement>("#capture")!;
const previewCanvas = document.createElement("canvas");
const cropCanvas = document.createElement("canvas");
const view: CaptureView = { target: null, connected: false, preview: null, previewSize: null, region: null, source: null, session: null, clock: null, error: null, busy: false };
view.captureMode = "all";
const detector = new SmartDetector();
let observations: CaptureObservation[] = [];
let tail: { pixels: ImageData; metadata: Omit<Sample, "sequence" | "image">; decision: SmartDecision } | null = null;
let savedTime = -Infinity;
let imageSequence = 0;
let batchStarted = 0;
let batchTimer: ReturnType<typeof setTimeout> | null = null;
async function flushObservations() {
  if (batchTimer !== null) { clearTimeout(batchTimer); batchTimer = null; }
  if (!observations.length || !view.session) return;
  view.session = await saveObservations(view.session, observations);
  observations = []; batchStarted = 0; wakeNative();
}
async function storeImage(metadata: Omit<Sample, "sequence" | "image">, pixels?: ImageData) {
  if (pixels) { cropCanvas.width = pixels.width; cropCanvas.height = pixels.height; cropCanvas.getContext("2d")!.putImageData(pixels, 0, 0); }
  const original = await new Promise<Blob>((resolve, reject) => cropCanvas.toBlob(blob => blob ? resolve(blob) : reject(new Error("Khong ma hoa duoc anh crop.")), "image/png"));
  const image = await compactPng(original);
  const current = view.session!;
  const sample: Sample = { ...metadata, sessionId: current.id, sequence: current.sampleCount + 1, image };
  view.session = await saveSample({ ...current, sampleCount: sample.sequence, imageBytes: current.imageBytes + image.size }, sample);
  imageSequence = sample.sequence; savedTime = metadata.estimatedVideoTimeMs; wakeNative();
}
async function closeSmart() {
  if (tail && tail.metadata.estimatedVideoTimeMs !== savedTime) {
    await storeImage({ ...tail.metadata, smart: { ...tail.decision, save: true, role: "end" } }, tail.pixels);
    const last = observations[observations.length - 1];
    if (last && last.smart.group === tail.decision.group) last.imageSequence = imageSequence;
  }
  await flushObservations(); tail = null; detector.reset(); view.detectorState = "observing";
}
let stream: MediaStream | null = null;
let callback: number | null = null;
let fallbackTimer: number | null = null;
let lastCapture = 0;
let lastPreview = 0;
let previousClock: Clock | null = null;
let regionSourceLayout: string | null = null;
let regionAnchor: Region | null = null;
let startedAt = 0;
let sampling: Promise<void> | null = null;
let controls: Promise<unknown> = Promise.resolve();
let pendingControls = 0;
let segment = 0;
let polling = false;
function wakeNative() { void chrome.runtime.sendMessage({ kind: "native-wake" }).catch(() => undefined); }

function enqueue<T>(task: () => Promise<T>): Promise<T> {
  pendingControls++;
  const operation = controls.catch(() => undefined).then(async () => { await sampling; return task(); });
  controls = operation;
  return operation.finally(() => { pendingControls--; });
}
async function event(kind: string, detail: string, clock = view.clock) {
  if (!view.session) return;
  await flushObservations();
  const next = structuredClone(view.session);
  next.events.push({ kind, detail, epochMs: epochNow(), mediaTimeMs: clock ? Math.round(clock.mediaTime * 1000) : null });
  view.session = await saveSession(next);
  wakeNative();
}
async function pause(detail: string, clearRegion = false) {
  try { await closeSmart(); } catch (failure) { detail += ` ${String(failure)}`; }
  view.error = detail;
  if (clearRegion) { view.region = null; regionAnchor = null; }
  if (view.session?.status === "recording") {
    const next = structuredClone(view.session);
    next.status = "paused";
    next.events.push({ kind: "capture-paused", detail, epochMs: epochNow(), mediaTimeMs: view.clock ? Math.round(view.clock.mediaTime * 1000) : null });
    view.session = next;
    try { view.session = await saveSession(next); wakeNative(); }
    catch { view.error = `${detail} Không ghi được checkpoint; ảnh đã lưu trước đó vẫn được giữ.`; }
  }
}
async function probe(source: VideoSource): Promise<{ clock: Clock; roundTripMs: number }> {
  const started = epochNow();
  let timer: ReturnType<typeof setTimeout>;
  const reply: Reply<Clock> = await Promise.race([
    chrome.runtime.sendMessage({ kind: "probe-source", source }),
    new Promise<never>((_, reject) => { timer = setTimeout(() => reject(new Error("Player phản hồi chậm; tạm dừng thu để giữ đúng timestamp.")), 500); }),
  ]).finally(() => clearTimeout(timer));
  if (!reply.ok) throw new Error(reply.error);
  if (reply.value.videoId !== source.clock.videoId) throw new Error("Clock sai video.");
  view.playerGeometry = reply.value.geometry ?? null;
  return { clock: reply.value, roundTripMs: epochNow() - started };
}
async function newSession(status: CaptureSession["status"]) {
  if (!view.source || !view.target || !view.region) throw new Error("Chon video va vung subtitle truoc.");
  const { id, frameId, documentId, origin } = view.source;
  const session: CaptureSession = {
    schemaVersion: view.captureMode === "smart" ? 2 : 1, ...(view.captureMode === "smart" ? { captureMode: "smart" as const, observationCount: 0, fallbackCount: 0 } : {}), id: crypto.randomUUID(), title: view.target.title, url: view.target.url, createdAt: Date.now(), endedAt: null,
    status, source: { id, frameId, documentId, origin }, region: view.region, sampleCount: 0, imageBytes: 0, events: [], native: emptyNative(),
  };
  view.session = await saveSession(session);
  wakeNative();
  segment = 0;
  previousClock = null;
  detector.reset(); tail = null; observations = []; savedTime = -Infinity; imageSequence = 0;
  await event("session-started", "Timestamp la uoc luong; do tre compositor chua duoc hieu chinh.");
}
async function record(manual: boolean) {
  if (manual && view.session?.status === "recording") throw new Error("Tạm dừng thu trước khi lưu ảnh mẫu.");
  if (!view.connected || !view.source || !view.region || !view.previewSize) throw new Error("Chua ket noi hoac chua chon vung.");
  if (!view.session || view.session.status === "stopped") await newSession("paused");
  const size = { width: video.videoWidth, height: video.videoHeight };
  if (size.width !== view.previewSize.width || size.height !== view.previewSize.height) { await pause("Kich thuoc tab da doi. Chon lai vung subtitle.", true); return; }
  const pixels = regionPixels(view.region, size);
  cropCanvas.width = pixels.width;
  cropCanvas.height = pixels.height;
  const capturedEpochMs = epochNow();
  cropCanvas.getContext("2d")!.drawImage(video, pixels.x, pixels.y, pixels.width, pixels.height, 0, 0, pixels.width, pixels.height);
  const { clock, roundTripMs } = await probe(view.source);
  view.clock = clock;
  const change = discontinuity(previousClock, clock);
  if (change === "source-changed") { await pause("Player doi video. Ket thuc phien va ket noi lai.", true); return; }
  const changedLayout = layoutMismatch(regionSourceLayout, clock.layout, null, null);
  if (changedLayout) { await pause(changedLayout, true); return; }
  if (!clock.geometry || !regionAnchor) { await pause("Mất vùng neo video. Chọn lại vùng phụ đề.", true); return; }
  const anchoredRegion = regionFromAnchor(regionAnchor, clock.geometry);
  if (Object.keys(anchoredRegion).some(key => Math.abs(anchoredRegion[key as keyof Region] - view.region![key as keyof Region]) > .000001)) { await pause("Vùng phụ đề đã lệch khỏi video. Chọn lại vùng.", true); return; }
  if (change === "seek" || change === "rate-changed") { await closeSmart(); segment++; await event(change, "Timeline tao segment moi; doan bo qua khong duoc coi la da thu.", clock); previousClock = clock; return; }
  previousClock = clock;
  if (!manual && (clock.paused || clock.waiting || clock.seeking || clock.ended || clock.readyState < 3)) { await closeSmart(); return; }
  const timing = matchClock(clock, capturedEpochMs, roundTripMs);
  const current = view.session!;
  const metadata: Omit<Sample, "sequence" | "image"> = { sessionId: current.id, segment, capturedEpochMs, ...timing, roundTripMs, captureSize: size, region: { ...view.region }, clock, manual };
  if (current.schemaVersion !== 2 || manual) { await storeImage(metadata); }
  else {
    let decision: SmartDecision;
    const pixels = cropCanvas.getContext("2d", { willReadFrequently: true })!.getImageData(0, 0, cropCanvas.width, cropCanvas.height);
    if (pixels.data.byteLength <= SMART_RAM_BYTES / 2) decision = detector.observe(glyphSignature(pixels.data, pixels.width, pixels.height), timing.estimatedVideoTimeMs);
    else { detector.reset(); decision = { group: (tail?.decision.group ?? 0) + 1, stable: false, save: true, role: "fallback" }; }
    if (tail && decision.group !== tail.decision.group) {
      if (tail.metadata.estimatedVideoTimeMs !== savedTime) await storeImage({ ...tail.metadata, smart: { ...tail.decision, save: true, role: "end" } }, tail.pixels);
      await flushObservations();
    }
    if (decision.save) await storeImage({ ...metadata, smart: decision }, pixels);
    const observation: CaptureObservation = { ...metadata, observationSeq: (view.session!.observationCount ?? 0) + observations.length + 1, imageSequence, smart: decision };
    if (!observations.length) {
      batchStarted = epochNow();
      batchTimer = setTimeout(() => {
        batchTimer = null;
        void enqueue(flushObservations).catch(failure => enqueue(() => pause(String(failure))).catch(() => undefined));
      }, 500);
    }
    observations.push(observation);
    tail = pixels.data.byteLength <= SMART_RAM_BYTES / 2 ? { pixels, metadata, decision } : null;
    view.detectorState = decision.role === "fallback" ? "fallback" : decision.stable ? "stable" : "observing";
    if (observations.length >= 5 || epochNow() - batchStarted >= 500) await flushObservations();
  }
  view.error = null;
  view.lastObservationAt = Date.now();
}
function frames() {
  if (!view.connected) return;
  const now = epochNow();
  const size = { width: video.videoWidth, height: video.videoHeight };
  if (size.width > 0 && size.height > 0 && now - lastPreview >= 500) {
    if (view.previewSize && (view.previewSize.width !== size.width || view.previewSize.height !== size.height)) {
      void enqueue(() => pause("Tab doi kich thuoc. Chon lai vung subtitle.", true));
    }
    view.previewSize = size;
    previewCanvas.width = Math.min(size.width, 1280);
    previewCanvas.height = Math.round(size.height * previewCanvas.width / size.width);
    previewCanvas.getContext("2d")!.drawImage(video, 0, 0, previewCanvas.width, previewCanvas.height);
    view.preview = previewCanvas.toDataURL("image/jpeg", 0.8);
    lastPreview = now;
  }
  if (view.session?.status === "recording" && view.region && !sampling && !pendingControls && now - lastCapture >= SAMPLE_INTERVAL_MS) {
    if (lastCapture && now - lastCapture > 600) void enqueue(async () => { await closeSmart(); await event("sampling-gap", `${Math.round(now - lastCapture)} ms giua cac frame; can doi chieu.`); });
    lastCapture = now;
    sampling = record(false).catch(async failure => { await pause(failure instanceof Error ? failure.message : String(failure)); }).finally(() => { sampling = null; });
  }
}
async function stop(reason?: string) {
  if (callback !== null) video.cancelVideoFrameCallback(callback);
  if (fallbackTimer !== null) clearInterval(fallbackTimer);
  callback = null;
  fallbackTimer = null;
  stream?.getTracks().forEach(track => track.stop());
  stream = null;
  video.srcObject = null;
  view.connected = false;
  await closeSmart();
  if (view.session && view.session.status !== "stopped") {
    const next = structuredClone(view.session);
    next.status = "stopped";
    next.endedAt = Date.now();
    next.events.push({ kind: "capture-stopped", detail: reason ?? "Nguoi dung ket thuc thu.", epochMs: epochNow(), mediaTimeMs: view.clock ? Math.round(view.clock.mediaTime * 1000) : null });
    view.session = next;
    try { view.session = await saveSession(next); wakeNative(); }
    catch { next.status = "interrupted"; view.error = "Đã dừng capture nhưng không ghi được checkpoint cuối. Có thể xuất những ảnh đã lưu."; }
  }
  if (reason) view.error = reason;
}
async function command(command: Command) {
  switch (command.action) {
    case "capture-mode": {
      if (view.session && !["stopped", "interrupted"].includes(view.session.status)) throw new Error("Kết thúc phiên trước khi đổi chế độ thu.");
      if (!["smart", "all"].includes(command.mode)) throw new Error("Chế độ thu không hợp lệ.");
      view.captureMode = command.mode; return null;
    }
    case "state": {
      const session = view.session ? await readSession(view.session.id) : undefined;
      return { ...view, session: session ?? view.session, busy: pendingControls > 0 };
    }
    case "open": {
      await stop();
      Object.assign(view, { target: command.target, source: null, session: null, region: null, clock: null, preview: null, previewSize: null, error: null });
      regionAnchor = null; regionSourceLayout = null; view.lastObservationAt = null;
      view.playerGeometry = null;
      await recoverSessions(null);
      const { width, height } = command.captureSize;
      if (![width, height].every(Number.isInteger) || width < 2 || width > 1920 || height < 2 || height > 1080) throw new Error("Kích thước capture không hợp lệ.");
      stream = await navigator.mediaDevices.getUserMedia({ audio: false, video: { mandatory: { chromeMediaSource: "tab", chromeMediaSourceId: command.streamId, minWidth: width, maxWidth: width, minHeight: height, maxHeight: height, maxFrameRate: 15 } } } as unknown as MediaStreamConstraints);
      video.srcObject = stream;
      await video.play();
      view.connected = true;
      stream.getVideoTracks()[0].addEventListener("ended", () => { void enqueue(() => stop("Chrome da dung chia se tab.")); }, { once: true });
      lastPreview = 0;
      lastCapture = 0;
      // Hidden offscreen video is not always submitted to the compositor, so rVFC alone can stall.
      const presented = () => { frames(); if (view.connected) callback = video.requestVideoFrameCallback(presented); };
      callback = video.requestVideoFrameCallback(presented);
      // Poll below the sample interval so a busy tick does not cost another full 100 ms.
      fallbackTimer = window.setInterval(() => { if (video.readyState >= 2) frames(); }, Math.max(16, Math.floor(SAMPLE_INTERVAL_MS / 4)));
      return null;
    }
    case "source": {
      if (view.session && view.session.status !== "stopped" && view.session.sampleCount) throw new Error("Ket thuc phien truoc khi doi video.");
      const checked = await probe(command.source);
      view.source = command.source;
      view.clock = checked.clock;
      view.region = null;
      regionAnchor = null; regionSourceLayout = null;
      previousClock = null;
      view.error = null;
      return null;
    }
    case "region": {
      if (!validRegion(command.region)) throw new Error("Vung chon qua nho hoac nam ngoai anh.");
      if (view.session?.status === "recording") throw new Error("Tam dung thu truoc khi doi vung.");
      if (!view.source) throw new Error("Chon video truoc khi chon vung.");
      await closeSmart();
      const { clock } = await probe(view.source);
      if (!clock.geometry || !view.previewSize) throw new Error("Chưa xác minh được hình video. Quét và chọn lại player.");
      if (Math.abs(clock.geometry.viewport.width / clock.geometry.viewport.height - view.previewSize.width / view.previewSize.height) > .01) throw new Error(`Tỷ lệ ảnh tab và viewport khác nhau: ảnh ${view.previewSize.width}×${view.previewSize.height}, viewport ${clock.geometry.viewport.width}×${clock.geometry.viewport.height}. Kết thúc/kết nối lại capture; không dùng vùng có thể lệch.`);
      regionAnchor = anchorToPlayer(command.region, clock.geometry);
      view.region = regionFromAnchor(regionAnchor, clock.geometry);
      regionSourceLayout = clock.layout;
      previousClock = null;
      await event("region-changed", JSON.stringify(command.region), clock);
      view.error = null;
      return null;
    }
    case "start":
    case "resume": {
      if (!view.connected || !view.region || !view.source) throw new Error("Ket noi video va chon vung truoc.");
      const { clock } = await probe(view.source);
      if (clock.readyState < 2) throw new Error("Video chua tai xong.");
      const changedLayout = layoutMismatch(regionSourceLayout, clock.layout, null, null);
      if (changedLayout) { view.region = null; regionAnchor = null; throw new Error(changedLayout); }
      if (!clock.geometry || !regionAnchor) throw new Error("Chọn lại vùng neo video trước khi thu.");
      await closeSmart();
      if (!view.session || view.session.status === "stopped") await newSession("recording");
      else {
        const stored = await readSession(view.session.id);
        if (stored?.native?.error) throw new Error("Helper từ chối lưu. Bấm Kết nối lại trước khi tiếp tục.");
        const next = { ...view.session, status: "recording" as const }; view.session = await saveSession(next); wakeNative();
        if (view.session.sampleCount > 0) segment++;
      }
      lastCapture = 0;
      startedAt = Date.now(); view.lastObservationAt = null;
      previousClock = null;
      await event("capture-resumed", "Bat dau lay mau; phim pause se khong sinh anh lap.", clock);
      view.error = null;
      return null;
    }
    case "pause": await pause("Da tam dung thu."); return null;
    case "stop": await stop(command.reason); return null;
    case "snapshot": await record(true); return null;
    default: throw new Error("Lenh khong ho tro.");
  }
}
chrome.runtime.onMessage.addListener((message, sender, respond) => {
  if (message.target !== "offscreen" || sender.id !== chrome.runtime.id) return;
  const work = async () => {
    if (message.uploadError) {
      if (view.connected && view.session?.id === message.uploadError.sessionId) await pause(message.uploadError.error);
      return null;
    }
    if (message.event) {
      if (view.connected && view.source && message.frameId === view.source.frameId && message.documentId === view.source.documentId && message.clock?.videoId === view.source.clock.videoId) {
        view.clock = message.clock;
        if (["pause", "waiting", "ended", "seeking", "seeked", "ratechange"].includes(message.event)) await closeSmart();
        if (["seeking", "seeked", "ratechange"].includes(message.event)) { segment++; previousClock = null; }
        await event(message.event, "Su kien tu player nguon.", message.clock);
      }
      return null;
    }
    return command(message.command);
  };
  const operation = message.command?.action === "state" ? work() : enqueue(work);
  void operation.then(value => respond({ ok: true, value })).catch(failure => {
    view.error = failure instanceof Error ? failure.message : String(failure);
    respond({ ok: false, error: view.error });
  });
  return true;
});
setInterval(() => {
  if (!view.connected || !view.source || polling) return;
  const recording = view.session?.status === "recording";
  if (recording && Date.now() - (view.lastObservationAt ?? startedAt) < 3000) return;
  polling = true;
  void enqueue(async () => {
    if (!view.source || !view.connected) return;
    const { clock } = await probe(view.source); view.clock = clock;
    if (captureStalled(view, startedAt)) await pause("Phim đang phát nhưng capture không nhận mẫu trong 3 giây. Kiểm tra tab và bấm tiếp tục thu.");
  }).catch(failure => {
    if (view.session?.status === "recording") return enqueue(() => pause(String(failure))).catch(() => undefined);
    view.error = String(failure);
  }).finally(() => { polling = false; });
}, 500);
