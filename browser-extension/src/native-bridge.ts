import { NATIVE_HOST, nativeRequest, PROTOCOL_VERSION } from "./native-protocol";
import { SMART_CAPABILITY } from "./smart-capture";
import { acknowledge, listSessions, nextOutbox, setNativeError } from "./storage";
import type { NativeAck, NativeConnection, OcrStatus } from "./types";

export class NativeBridge {
  readonly state: NativeConnection = { connected: false, error: null, appData: null, hostVersion: null };
  private port: chrome.runtime.Port | null = null;
  private connecting: Promise<void> | null = null;
  private requests = new Map<string, { resolve: (value: unknown) => void; reject: (error: Error) => void; timer: ReturnType<typeof setTimeout> }>();
  private retryAttempt = 0;
  private retryAt = 0;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private pumping = false;
  constructor(private pause: (sessionId: string, error: string) => Promise<void>) {}

  private lost(port: chrome.runtime.Port, detail: string) {
    if (this.port !== port) return;
    this.port = null;
    this.state.connected = false;
    this.state.capabilities = [];
    this.state.error = detail;
    this.retryAt = Date.now() + Math.min(10_000, 1000 * 2 ** this.retryAttempt++);
    for (const request of this.requests.values()) { clearTimeout(request.timer); request.reject(new Error(detail)); }
    this.requests.clear();
    this.wake(this.retryAt - Date.now());
  }
  private async send(message: Record<string, unknown>): Promise<unknown> {
    const port = this.port;
    if (!port) throw new Error("Chưa kết nối helper.");
    const requestId = typeof message.requestId === "string" ? message.requestId : crypto.randomUUID();
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { this.lost(port, "Helper không phản hồi; đang kết nối lại."); port.disconnect(); }, 15_000);
      this.requests.set(requestId, { resolve, reject, timer });
      try { port.postMessage({ ...message, version: PROTOCOL_VERSION, requestId }); }
      catch { this.lost(port, "Mất kết nối helper; dữ liệu chưa ACK vẫn ở bộ đệm."); }
    });
  }
  async connect(): Promise<void> {
    if (this.state.connected) return;
    if (this.connecting) return this.connecting;
    if (Date.now() < this.retryAt) throw new Error(this.state.error ?? "Đang đợi kết nối lại helper.");
    this.connecting = (async () => {
      const port = chrome.runtime.connectNative(NATIVE_HOST);
      this.port = port;
      port.onDisconnect.addListener(() => { this.lost(port, chrome.runtime.lastError?.message ?? "Helper đã ngắt kết nối."); });
      port.onMessage.addListener((message: { version?: number; requestId?: string; ok?: boolean; value?: unknown; error?: string }) => {
        if (this.port !== port || typeof message.requestId !== "string") return;
        const pending = this.requests.get(message.requestId);
        if (!pending) return;
        clearTimeout(pending.timer);
        this.requests.delete(message.requestId);
        if (message.version !== PROTOCOL_VERSION || typeof message.ok !== "boolean") { pending.reject(new Error("Protocol helper không tương thích.")); return; }
        if (message.ok) pending.resolve(message.value);
        else pending.reject(new Error(message.error ?? "Helper từ chối dữ liệu."));
      });
      try {
        const hello = await this.send({ operation: "hello" }) as { protocolVersion: number; appData: string; hostVersion: string; capabilities?: unknown };
        if (hello?.protocolVersion !== PROTOCOL_VERSION || typeof hello.appData !== "string" || typeof hello.hostVersion !== "string") throw new Error("Handshake helper không hợp lệ.");
        const capabilities = Array.isArray(hello.capabilities) ? hello.capabilities.filter((v): v is string => typeof v === "string") : [];
        Object.assign(this.state, { connected: true, error: null, appData: hello.appData, hostVersion: hello.hostVersion, capabilities });
        this.retryAttempt = 0;
        this.retryAt = 0;
      } catch (failure) {
        this.lost(port, failure instanceof Error ? failure.message : String(failure));
        port.disconnect();
        throw failure;
      }
    })().finally(() => { this.connecting = null; });
    return this.connecting;
  }
  wake(delay = 0) {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = setTimeout(() => { this.timer = null; void this.pump(); }, Math.max(delay, this.retryAt - Date.now(), 0));
  }
  private async pump() {
    if (this.pumping) { this.wake(100); return; }
    this.pumping = true;
    try {
      await this.connect();
      for (const session of await listSessions()) {
        if (!session.native || session.native.error) continue;
        let queued = await nextOutbox(session.id);
        while (queued) {
          try {
            if (session.schemaVersion === 2 && !this.state.capabilities?.includes(SMART_CAPABILITY)) throw new Error("Helper chưa hỗ trợ thu thông minh; cập nhật helper để lưu tiếp. Outbox vẫn được giữ.");
            const ack = await this.send(await nativeRequest(queued)) as NativeAck;
            await acknowledge(queued, ack);
          } catch (failure) {
            if (!this.state.connected) throw failure;
            const detail = failure instanceof Error ? failure.message : String(failure);
            await setNativeError(session.id, detail);
            await this.pause(session.id, detail);
            break;
          }
          queued = await nextOutbox(session.id);
        }
      }
    } catch (failure) {
      this.state.error = failure instanceof Error ? failure.message : String(failure);
      this.wake(Math.max(1000, this.retryAt - Date.now()));
    } finally { this.pumping = false; }
  }
  async retry() {
    this.retryAt = 0;
    this.retryAttempt = 0;
    for (const session of await listSessions()) if (session.native?.error) await setNativeError(session.id, null);
    await this.connect();
    this.wake();
  }
  async openFolder(sessionId: string) {
    await this.connect();
    await this.send({ operation: "openFolder", sessionId });
  }
  async ocr(operation: "ocrStart" | "ocrStatus" | "ocrCancel", sessionId: string): Promise<OcrStatus> {
    await this.connect();
    const value = await this.send({ operation, sessionId }) as OcrStatus;
    if (value?.sessionId !== sessionId || !["not-started", "queued", "processing", "interrupted", "cancelled", "error", "partial", "no-text"].includes(value.state) || typeof value.committed !== "boolean" || [value.processedSamples, value.totalSamples, value.cueCount, value.reviewCount].some(count => count !== undefined && (!Number.isSafeInteger(count) || count < 0)) || (value.committed && !["partial", "no-text"].includes(value.state))) throw new Error("OCR response không hợp lệ.");
    return value;
  }
}
