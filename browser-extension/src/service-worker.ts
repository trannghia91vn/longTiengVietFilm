import type { Command, NativeCommand, RecorderView, Reply, TargetTab, VideoSource } from "./types";
import { SMART_CAPABILITY } from "./smart-capture";
import { NativeBridge } from "./native-bridge";
import { recoverSessions } from "./storage";
import { captureSizeForViewport, SelectedPlayer } from "./player-geometry";

const native = new NativeBridge(async (sessionId, error) => {
  if (await exists()) await chrome.runtime.sendMessage({ target: "offscreen", uploadError: { sessionId, error } });
});

const sources = new Map<string, VideoSource>();
const player = new SelectedPlayer({
  frames: async tabId => {
    if (!await chrome.permissions.contains({ permissions: ["webNavigation"] })) throw new Error("Cấp quyền Liên kết iframe trước khi chọn player trong iframe.");
    return await chrome.webNavigation.getAllFrames({ tabId }) ?? [];
  },
  send: (tabId, frame, message) => chrome.tabs.sendMessage(tabId, message, { frameId: frame.frameId, documentId: frame.documentId }),
});
const origins = new Set<string>();
let target: TargetTab | null = null;
let recorderWindowId: number | null = null;
let opening = false;
let error: string | null = null;
const ready = chrome.storage.session.get(["target", "recorderWindowId"]).then(saved => {
  target = saved.target as TargetTab | undefined ?? null;
  recorderWindowId = typeof saved.recorderWindowId === "number" ? saved.recorderWindowId : null;
}).then(async () => {
  const state = await exists() ? await offscreen({ action: "state" }) : null;
  const current = state?.ok ? state.value as RecorderView : null;
  await recoverSessions(current?.connected ? current.session?.id ?? null : null);
  if (current?.connected && current.source && target) {
    try { await player.select(target.id, current.source); }
    catch (failure) { await offscreen({ action: "pause" }); error = `Mất liên kết player: ${String(failure)}`; }
  }
});
async function offscreen(command: Command): Promise<Reply> {
  return chrome.runtime.sendMessage({ target: "offscreen", command });
}
async function exists(): Promise<boolean> {
  return (await chrome.runtime.getContexts({ contextTypes: [chrome.runtime.ContextType.OFFSCREEN_DOCUMENT] })).length > 0;
}
async function scan(): Promise<void> {
  if (!target) return;
  error = null;
  await chrome.scripting.executeScript({ target: { tabId: target.id, frameIds: [0] }, files: ["content.js"] });
  try { await chrome.scripting.executeScript({ target: { tabId: target.id, allFrames: true }, files: ["content.js"] }); }
  catch { error = "Chua truy cap duoc mot frame player. Cap quyen cho mien player roi quet lai."; }
}
async function openRecorder() {
  if (recorderWindowId !== null) {
    try { await chrome.windows.update(recorderWindowId, { focused: true }); return; } catch { recorderWindowId = null; }
  }
  const window = await chrome.windows.create({ type: "popup", url: chrome.runtime.getURL("recorder.html"), width: 1120, height: 820 });
  recorderWindowId = window?.id ?? null;
  await chrome.storage.session.set({ recorderWindowId });
}
chrome.action.onClicked.addListener(tab => {
  void (async () => {
    await ready;
    if (opening) return;
    opening = true;
    try {
      if (await exists()) {
        const current = await offscreen({ action: "state" });
        if (current.ok && (current.value as RecorderView).connected) { await openRecorder(); return; }
      }
      if (!tab.id || !tab.url || !/^https?:/.test(tab.url)) throw new Error("Mo tab phim HTTP/HTTPS roi bam extension.");
      target = { id: tab.id, title: tab.title ?? "Phim", url: tab.url };
      sources.clear();
      origins.clear();
      player.clear();
      error = null;
      await chrome.storage.session.set({ target });
      if (!await exists()) {
        await chrome.offscreen.createDocument({ url: "offscreen.html", reasons: [chrome.offscreen.Reason.USER_MEDIA, chrome.offscreen.Reason.BLOBS], justification: "Capture selected subtitle region with video timestamps." });
      }
      const viewport = await chrome.scripting.executeScript({ target: { tabId: target.id, frameIds: [0] }, func: () => ({ width: innerWidth, height: innerHeight, pixelRatio: devicePixelRatio }) });
      const measured = viewport[0]?.result;
      if (!measured) throw new Error("Không đọc được viewport của tab phim.");
      const captureSize = captureSizeForViewport(measured, measured.pixelRatio);
      const streamId = await chrome.tabCapture.getMediaStreamId({ targetTabId: target.id });
      const result = await offscreen({ action: "open", target, streamId, captureSize });
      if (!result.ok) throw new Error(result.error);
      const saved = await chrome.storage.local.get("captureMode");
      if (saved.captureMode === "smart") {
        try {
          await native.connect();
          if (native.state.capabilities?.includes(SMART_CAPABILITY)) await offscreen({ action: "capture-mode", mode: "smart" });
        } catch { /* Opening capture still works in raw mode if the helper is unavailable. */ }
      }
      await scan();
    } catch (failure) { error = failure instanceof Error ? failure.message : String(failure); }
    finally { opening = false; await openRecorder(); }
  })();
});

chrome.runtime.onMessage.addListener((message, sender, respond) => {
  if (message.target === "offscreen") return;
  const process = async (): Promise<Reply> => {
    if (message.kind === "frame-located") {
      if (sender.id !== chrome.runtime.id || sender.tab?.id !== target?.id || sender.frameId === undefined || !sender.documentId) throw new Error("Sai frame liên kết.");
      player.acceptLink(target!.id, { frameId: sender.frameId, documentId: sender.documentId }, message.nonce, message.elementId);
      return { ok: true, value: null };
    }
    await ready;
    if (message.kind === "clock-report" || message.kind === "player-event") {
      if (sender.tab?.id !== target?.id || sender.frameId === undefined || !sender.documentId) throw new Error("Sai tab nguon.");
      if (message.kind === "clock-report") {
        for (const source of sources.values()) {
          if (source.frameId === sender.frameId && (source.documentId !== sender.documentId || !message.clocks.some((clock: { videoId: string }) => clock.videoId === source.clock.videoId))) sources.delete(source.id);
        }
        for (const clock of message.clocks) {
          const id = `${sender.documentId}:${clock.videoId}`;
          sources.set(id, { id, frameId: sender.frameId, documentId: sender.documentId, origin: sender.origin ?? new URL(sender.url!).origin, clock, seenAt: Date.now() });
        }
        for (const origin of message.origins) origins.add(origin);
      } else if (await exists()) {
        await chrome.runtime.sendMessage({ target: "offscreen", event: message.event, clock: message.clock, frameId: sender.frameId, documentId: sender.documentId });
      }
      return { ok: true, value: null };
    }
    if (sender.id !== chrome.runtime.id || sender.tab?.id === target?.id || !sender.url?.startsWith(chrome.runtime.getURL(""))) throw new Error("Lenh khong den tu extension.");
    if (message.kind === "probe-source") {
      const source: VideoSource = message.source;
      return player.probe(target!.id, source);
    }
    if (message.kind === "native-wake") { native.wake(); return { ok: true, value: null }; }
    if (message.kind !== "command") throw new Error("Lenh khong hop le.");
    const command: Command | NativeCommand = message.command;
    if (command.action === "native-retry") { await native.retry(); return { ok: true, value: null }; }
    if (command.action === "open-folder") { await native.openFolder(command.sessionId); return { ok: true, value: null }; }
    if (command.action === "ocr-start" || command.action === "ocr-status" || command.action === "ocr-cancel") {
      const operation = command.action === "ocr-start" ? "ocrStart" : command.action === "ocr-status" ? "ocrStatus" : "ocrCancel";
      return { ok: true, value: await native.ocr(operation, command.sessionId) };
    }
    if (["start", "resume", "snapshot", "capture-mode"].includes(command.action)) {
      try { await native.connect(); }
      catch { throw new Error("Chưa kết nối helper native. Chạy bước đăng ký host rồi bấm Kết nối lại."); }
      const state = await offscreen({ action: "state" });
      const smart = command.action === "capture-mode" ? command.mode === "smart" : state.ok && (state.value as RecorderView)?.captureMode === "smart";
      if (smart && !native.state.capabilities?.includes(SMART_CAPABILITY)) throw new Error("Helper chưa hỗ trợ Thông minh. Cập nhật helper hoặc chọn Lưu mọi ảnh.");
    }
    if (command.action === "scan") { await scan(); return { ok: true, value: null }; }
    if (command.action === "state") {
      native.wake();
      let state: Record<string, unknown> = {};
      if (await exists()) {
        const response = await offscreen(command);
        if (response.ok) state = response.value as Record<string, unknown>;
      }
      return { ok: true, value: { target, connected: false, ...state, error: error ?? state.error ?? null, sources: Array.from(sources.values()).filter(source => Date.now() - source.seenAt < 3500), frameOrigins: Array.from(origins), topLayout: null, native: native.state, navigationAllowed: await chrome.permissions.contains({ permissions: ["webNavigation"] }) } };
    }
    if (command.action === "source" && !sources.has(command.source.id)) throw new Error("Nguon video da mat; quet lai.");
    if (!await exists()) throw new Error("Bam extension tren tab phim de ket noi lai.");
    if (command.action === "source") {
      const current = await offscreen({ action: "state" });
      const session = current.ok ? (current.value as RecorderView).session : null;
      if (session && session.status !== "stopped" && session.sampleCount) throw new Error("Kết thúc phiên trước khi đổi video.");
      await player.select(target!.id, sources.get(command.source.id)!);
    }
    const response = await offscreen(command);
    if (response.ok && command.action === "capture-mode") await chrome.storage.local.set({ captureMode: command.mode });
    return response;
  };
  void process().then(respond).catch(failure => respond({ ok: false, error: failure instanceof Error ? failure.message : String(failure) }));
  return true;
});
chrome.tabs.onRemoved.addListener(tabId => {
  if (target?.id === tabId) void exists().then(async active => { if (active) await offscreen({ action: "stop", reason: "Tab phim da dong." }); }).catch(() => undefined);
});
chrome.tabs.onUpdated.addListener((tabId, change) => {
  if (target?.id === tabId && change.url && change.url !== target.url) void exists().then(async active => { if (active) await offscreen({ action: "stop", reason: "Tab doi trang; tao phien moi cho tap moi." }); }).catch(() => undefined);
});
chrome.windows.onRemoved.addListener(windowId => {
  if (windowId === recorderWindowId) { recorderWindowId = null; void chrome.storage.session.set({ recorderWindowId }); }
});
