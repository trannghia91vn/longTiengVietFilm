import { SelectedPlayer, anchorToPlayer, regionFromAnchor } from "../browser-extension/src/player-geometry";
import type { Clock, Reply, VideoSource } from "../browser-extension/src/types";

const frame = document.querySelector<HTMLIFrameElement>("#player")!;
const ad = document.querySelector<HTMLIFrameElement>("#ad")!;
const result = document.querySelector<HTMLElement>("#result")!;
const handlers: ((message: unknown, sender: unknown, respond: (value: unknown) => void) => void)[] = [];
const responses = new Map<string, (value: Reply<unknown>) => void>();
let clock: Clock | null = null;
let completed = false;
const player = new SelectedPlayer({
  frames: async () => [{ frameId: 0, documentId: "top", parentFrameId: -1 }, { frameId: 1, documentId: "child", parentFrameId: 0, parentDocumentId: "top" }, { frameId: 2, documentId: "ad", parentFrameId: 0 }],
  send: async <T>(_tabId: number, target: { frameId: number }, message: unknown): Promise<Reply<T>> => {
    if (target.frameId === 0) return new Promise(resolve => { for (const handler of handlers) handler(message, { id: "geometry-qa" }, value => resolve(value as Reply<T>)); });
    const id = crypto.randomUUID();
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { responses.delete(id); reject(Error("QA child timeout")); }, 1000);
      responses.set(id, value => { clearTimeout(timer); responses.delete(id); resolve(value as Reply<T>); });
      frame.contentWindow!.postMessage({ qa: "invoke", id, message }, "*");
    });
  },
});
Object.assign(globalThis, { chrome: { runtime: { id: "geometry-qa", getManifest: () => ({manifest_version:3}), sendMessage: async () => undefined, onMessage: { addListener: (handler: typeof handlers[number]) => handlers.push(handler) } } } });
window.addEventListener("message", event => {
  if (event.source !== frame.contentWindow) return;
  if (event.data?.qa === "response") responses.get(event.data.id)?.(event.data.value);
  if (event.data?.qa !== "runtime") return;
  const message = event.data.message;
  if (message.kind === "frame-located") player.acceptLink(1, { frameId: 1, documentId: "child" }, message.nonce, message.elementId);
  if (message.kind === "clock-report") { clock = message.clocks[0] ?? null; if (clock?.width && !completed) result.textContent = "Player đã tải; sẵn sàng kiểm thử"; }
});
await import("../browser-extension/src/content");
frame.src = `http://localhost:${location.port}/tests/capture-geometry-player.html`;
document.querySelector<HTMLButtonElement>("#run")!.onclick = async () => {
  completed = true;
  const lines: string[] = [];
  const check = (ok: boolean, label: string) => { if (!ok) throw Error(label); lines.push(`PASS ${label}`); };
  try {
    if (!clock?.width) throw Error("Chưa tải video");
    const source: VideoSource = { id: `child:${clock.videoId}`, frameId: 1, documentId: "child", origin: "http://localhost", clock, seenAt: Date.now() };
    await player.select(1, source);
    const before = await player.probe(1, source); if (!before.ok || !before.value.geometry) throw Error(JSON.stringify(before));
    check(true, "Liên kết iframe khác origin và đọc hình học từ content thật");
    const g = before.value.geometry;
    const r = { x: (g.rect.x + g.rect.width * .2) / g.viewport.width, y: (g.rect.y + g.rect.height * .8) / g.viewport.height, width: g.rect.width * .6 / g.viewport.width, height: g.rect.height * .1 / g.viewport.height };
    const roundtrip = regionFromAnchor(anchorToPlayer(r, g), g);
    check(Object.keys(r).every(k => Math.abs(r[k as keyof typeof r] - roundtrip[k as keyof typeof r]) < 1e-10), "ROI neo video giữ đúng qua border và letterbox");
    ad.style.width = "200px"; ad.style.height = "80px";
    document.body.insertBefore(ad, frame);
    const changedAd = await player.probe(1, source);
    check(changedAd.ok && changedAd.value.layout === before.value.layout, "Đổi kích thước/thứ tự iframe quảng cáo không đổi layout player");
    frame.style.transform = "translateX(12px)";
    const moved = await player.probe(1, source);
    check(moved.ok && moved.value.layout !== before.value.layout, "Player thật dịch chuyển được phát hiện");
    frame.style.transform = "rotate(2deg)";
    check(!(await player.probe(1, source)).ok, "Phép xoay không hỗ trợ bị chặn");
    frame.style.transform = "";
    const replacement = frame.cloneNode(false) as HTMLIFrameElement;
    frame.replaceWith(replacement);
    check(!(await player.probe(1, source)).ok, "Thay phần tử iframe không tái dùng liên kết cũ");
    result.textContent = lines.join("\n");
  } catch (error) { result.textContent = [...lines, `FAIL ${String(error)}`].join("\n"); }
};
document.querySelector<HTMLButtonElement>("#fullscreen")!.onclick = async () => {
  completed = true;
  const html = document.documentElement, height = html.style.height, overflow = html.style.overflow, bodyHeight = document.body.style.minHeight, previousY = scrollY;
  try {
    if (!clock?.width || !frame.isConnected) throw Error("Reload fixture trước test fullscreen");
    html.style.height = "100vh"; html.style.overflow = "hidden"; document.body.style.minHeight = "2000px";
    window.scrollTo(0, 344);
    await frame.requestFullscreen();
    const source: VideoSource = { id: `child:${clock.videoId}`, frameId: 1, documentId: "child", origin: "http://localhost", clock, seenAt: Date.now() };
    await player.select(1, source);
    const response = await player.probe(1, source);
    if (!response.ok || !response.value.geometry) throw Error(JSON.stringify(response));
    const g = response.value.geometry;
    const roi = { x: (g.rect.x + g.rect.width * .2) / g.viewport.width, y: (g.rect.y + g.rect.height * .88) / g.viewport.height, width: g.rect.width * .6 / g.viewport.width, height: g.rect.height * .08 / g.viewport.height };
    anchorToPlayer(roi, g);
    result.textContent = `PASS Fullscreen iframe bỏ clipping của trang cuộn phía sau\nPASS ROI ở đáy video vẫn hợp lệ\nVideo ${JSON.stringify(g.rect)}\nClip ${JSON.stringify(g.clip)}\nPage scroll ${scrollY}`;
  } catch (error) { result.textContent = `FAIL Fullscreen: ${String(error)}`; }
  finally {
    if (document.fullscreenElement) await document.exitFullscreen();
    html.style.height = height; html.style.overflow = overflow; document.body.style.minHeight = bodyHeight; window.scrollTo(0, previousY);
  }
};
