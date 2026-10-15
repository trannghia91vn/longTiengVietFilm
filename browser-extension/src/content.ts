import { epochNow, type Clock } from "./types";
import { elementGeometry } from "./dom-geometry";

const VERSION = "selected-player-geometry-2";
const scope = window as Window & { __longTiengClockInstalled?: boolean; __longTiengClockVersion?: string; __longTiengClockDispose?: () => void; __longTiengClockAlive?: () => boolean };
let installed = false;
try { installed = scope.__longTiengClockVersion === VERSION && !!scope.__longTiengClockAlive?.(); } catch { /* A reloaded extension leaves an invalid old content context. */ }
if (!installed) {
  try { scope.__longTiengClockDispose?.(); } catch { /* The abandoned context cannot own the new hooks. */ }
  scope.__longTiengClockInstalled = true;
  scope.__longTiengClockVersion = VERSION;
  const videos = new Map<string, { element: HTMLVideoElement; waiting: boolean; mediaTime: number | null; frameEpochMs: number | null; callback: number | null }>();
  const ids = new WeakMap<HTMLVideoElement, string>();
  const listeners = new AbortController();
  const frames = new Map<string, HTMLIFrameElement>();
  const frameIds = new WeakMap<HTMLIFrameElement, string>();
  let active = true;
  let timer: ReturnType<typeof setInterval> | null = null;
  function detach() {
    active = false;
    if (scope.__longTiengClockDispose === detach) { scope.__longTiengClockInstalled = false; scope.__longTiengClockVersion = undefined; scope.__longTiengClockDispose = undefined; scope.__longTiengClockAlive = undefined; }
    listeners.abort();
    if (timer !== null) clearInterval(timer);
    for (const entry of videos.values()) {
      if (entry.callback !== null) entry.element.cancelVideoFrameCallback(entry.callback);
    }
  }
  scope.__longTiengClockDispose = detach;
  scope.__longTiengClockAlive = () => active && !!chrome.runtime.id && chrome.runtime.getManifest().manifest_version === 3;
  const send = (message: unknown) => {
    if (!active) return;
    try {
      if (!chrome.runtime.id) { detach(); return; }
      void chrome.runtime.sendMessage(message).catch(() => undefined);
    } catch { detach(); }
  };
  const clock = (id: string): Clock | null => {
    const entry = videos.get(id);
    if (!entry?.element.isConnected) return null;
    const video = entry.element;
    let geometry;
    try { geometry = elementGeometry(video); } catch { /* Hidden/unloaded players remain discoverable, but cannot capture. */ }
    return {
      videoId: id,
      sourceKey: video.currentSrc ? new URL(video.currentSrc, location.href).origin + new URL(video.currentSrc, location.href).pathname : "pending",
      mediaTime: video.currentTime,
      duration: Number.isFinite(video.duration) ? video.duration : null,
      rate: video.playbackRate,
      paused: video.paused,
      seeking: video.seeking,
      waiting: entry.waiting && !video.paused,
      ended: video.ended,
      readyState: video.readyState,
      epochMs: epochNow(),
      frameMediaTime: entry.mediaTime,
      frameEpochMs: entry.frameEpochMs,
      layout: JSON.stringify([geometry ?? null, video.videoWidth, video.videoHeight]),
      geometry,
      width: video.videoWidth,
      height: video.videoHeight,
    };
  };
  function discover(root: Document | ShadowRoot = document): HTMLVideoElement[] {
    const found = Array.from(root.querySelectorAll("video"));
    for (const element of root.querySelectorAll("*")) {
      if (element.shadowRoot) found.push(...discover(element.shadowRoot));
    }
    return found;
  }
  function scan() {
    for (const video of discover()) {
      if (ids.has(video)) continue;
      const id = crypto.randomUUID();
      ids.set(video, id);
      const entry = { element: video, waiting: false, mediaTime: null as number | null, frameEpochMs: null as number | null, callback: null as number | null };
      videos.set(id, entry);
      if (video.requestVideoFrameCallback) {
        const frame = (_now: number, metadata: VideoFrameCallbackMetadata) => {
          entry.mediaTime = metadata.mediaTime;
          entry.frameEpochMs = performance.timeOrigin + metadata.expectedDisplayTime;
          if (active && video.isConnected) entry.callback = video.requestVideoFrameCallback(frame);
        };
        entry.callback = video.requestVideoFrameCallback(frame);
      }
      for (const kind of ["play", "pause", "playing", "waiting", "stalled", "seeking", "seeked", "ratechange", "ended", "emptied", "loadedmetadata"]) {
        video.addEventListener(kind, () => {
          if (["waiting", "stalled"].includes(kind)) entry.waiting = true;
          if (["playing", "pause", "seeked", "loadedmetadata"].includes(kind)) entry.waiting = false;
          send({ kind: "player-event", event: kind, clock: clock(id) });
          report();
        }, { signal: listeners.signal });
      }
    }
    for (const [id, entry] of videos) {
      if (!entry.element.isConnected) {
        if (entry.callback !== null) entry.element.cancelVideoFrameCallback(entry.callback);
        videos.delete(id);
      }
    }
  }
  function report() {
    const origins = Array.from(document.querySelectorAll("iframe")).flatMap(frame => {
      try { const url = new URL(frame.src); return url.protocol === "https:" || url.protocol === "http:" ? [url.origin] : []; } catch { return []; }
    });
    send({ kind: "clock-report", clocks: Array.from(videos.keys()).map(clock).filter(Boolean), origins });
  }
  // Page messages carry only an opaque lookup token; Chrome authenticates the replying document.
  window.addEventListener("message", event => {
    if (!active || event.source !== window.parent || event.data?.kind !== "long-tieng-frame-locate" || !/^[a-f0-9-]{36}$/.test(event.data.nonce) || !/^[a-f0-9-]{36}$/.test(event.data.elementId)) return;
    send({ kind: "frame-located", nonce: event.data.nonce, elementId: event.data.elementId });
  }, { signal: listeners.signal });
  chrome.runtime.onMessage.addListener((message, sender, respond) => {
    if (!active || sender.id !== chrome.runtime.id) return;
    if (message.kind === "frame-locate") {
      for (const [id, frame] of frames) if (!frame.isConnected) frames.delete(id);
      for (const frame of document.querySelectorAll("iframe")) {
        let id = frameIds.get(frame);
        if (!id) { id = crypto.randomUUID(); frameIds.set(frame, id); frames.set(id, frame); }
        frame.contentWindow?.postMessage({ kind: "long-tieng-frame-locate", nonce: message.nonce, elementId: id }, "*");
      }
      respond({ ok: true, value: null }); return;
    }
    if (message.kind === "frame-geometry") {
      try {
        const frame = frames.get(message.elementId);
        if (!frame?.isConnected) throw new Error("Iframe player đã bị thay thế. Quét lại video.");
        respond({ ok: true, value: elementGeometry(frame) });
      } catch (error) { respond({ ok: false, error: String(error) }); }
      return;
    }
    if (message.kind !== "clock-probe") return;
    scan();
    const value = clock(message.videoId);
    respond(value ? { ok: true, value } : { ok: false, error: "Video da doi hoac frame khong con truy cap duoc." });
  });
  scan();
  report();
  if (active) timer = setInterval(() => { scan(); report(); }, 1000);
}
