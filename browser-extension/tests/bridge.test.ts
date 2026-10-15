import "fake-indexeddb/auto";
import test from "node:test";
import assert from "node:assert/strict";
import { NativeBridge } from "../src/native-bridge";
import { NATIVE_HOST } from "../src/native-protocol";

class Event<T extends (...args: any[]) => void> {
  listeners: T[] = [];
  addListener(listener: T) { this.listeners.push(listener); }
  emit(...args: Parameters<T>) { for (const listener of this.listeners) listener(...args); }
}
class Port {
  onDisconnect = new Event<() => void>();
  onMessage = new Event<(message: unknown) => void>();
  constructor(private valid: boolean) {}
  postMessage(message: { requestId: string; operation: string; sessionId?: string }) {
    queueMicrotask(() => this.onMessage.emit({ version: 1, requestId: message.requestId, ok: true, value: message.operation.startsWith("ocr") ? { sessionId: message.sessionId, ...ocrResponse } : this.valid ? { protocolVersion: 1, appData: "/app-data", hostVersion: "test" } : { protocolVersion: 99 } }));
  }
  disconnect() { this.onDisconnect.emit(); }
}
let ports: Port[] = [];
let valid = true;
let ocrResponse: Record<string, unknown> = { state: "processing", committed: false, processedSamples: 10, totalSamples: 20 };
globalThis.chrome = { runtime: { connectNative: (name: string) => { assert.equal(name, NATIVE_HOST); const port = new Port(valid); ports.push(port); return port; } } } as unknown as typeof chrome;

test("parallel handshake uses one native port and validates helper metadata", async () => {
  ports = []; valid = true;
  const bridge = new NativeBridge(async () => {});
  await Promise.all([bridge.connect(), bridge.connect()]);
  assert.equal(ports.length, 1);
  assert.equal(bridge.state.connected, true);
  assert.equal(bridge.state.appData, "/app-data");
});
test("disconnect updates state and user retry restores the native connection", async () => {
  ports = []; valid = true;
  const bridge = new NativeBridge(async () => {});
  await bridge.connect(); ports[0].disconnect();
  assert.equal(bridge.state.connected, false);
  assert.ok(bridge.state.error);
  await bridge.retry();
  assert.equal(bridge.state.connected, true);
  assert.equal(ports.length, 2);
});
test("incompatible hello fails closed then reconnects after helper correction", async () => {
  ports = []; valid = false;
  const bridge = new NativeBridge(async () => {});
  await assert.rejects(bridge.connect(), /Handshake/);
  assert.equal(bridge.state.connected, false);
  valid = true;
  await bridge.retry();
  assert.equal(bridge.state.connected, true);
});
test("OCR status is scoped to its session and malformed completion fails closed", async () => {
  valid=true;
  const bridge=new NativeBridge(async()=>{});
  const id="fixture-session";
  ocrResponse={state:"processing",committed:false,processedSamples:10,totalSamples:20};
  assert.equal((await bridge.ocr("ocrStatus",id)).sessionId,id);
  ocrResponse={state:"processing",committed:true};await assert.rejects(bridge.ocr("ocrStatus",id),/không hợp lệ/);
  ocrResponse={state:"partial",committed:true,cueCount:-1};await assert.rejects(bridge.ocr("ocrStatus",id),/không hợp lệ/);
  ocrResponse={state:"partial",committed:true,cueCount:5,reviewCount:0};assert.equal((await bridge.ocr("ocrStart",id)).cueCount,5);
});
