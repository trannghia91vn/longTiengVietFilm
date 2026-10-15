import type { NativeSessionState, OutboxEntry } from "./types";

export const NATIVE_HOST = "com.basushi.longtieng.capture";
export const PROTOCOL_VERSION = 1;
export const MAX_NATIVE_MESSAGE_BYTES = 16 * 1024 * 1024;
export const emptyNative = (): NativeSessionState => ({ messageSeq: 0, ackSequence: 0, savedSamples: 0, savedBytes: 0, pendingBytes: 0, finalized: false, sessionPath: null, error: null });
export async function nativeRequest(entry: OutboxEntry) {
  const image = entry.image ? new Uint8Array(await entry.image.arrayBuffer()) : new Uint8Array();
  const prefix = new TextEncoder().encode(`${entry.operation}\n${entry.sessionId}\n${entry.messageSeq}\n${entry.payloadJson}`);
  const input = new Uint8Array(prefix.length + image.length);
  input.set(prefix);
  input.set(image, prefix.length);
  const checksum = [...new Uint8Array(await crypto.subtle.digest("SHA-256", input))].map(byte => byte.toString(16).padStart(2, "0")).join("");
  let encoded = "";
  if (image.length) {
    const chunks: string[] = [];
    for (let offset = 0; offset < image.length; offset += 0x8000) chunks.push(String.fromCharCode(...image.subarray(offset, offset + 0x8000)));
    encoded = btoa(chunks.join(""));
  }
  const request = { version: PROTOCOL_VERSION, requestId: crypto.randomUUID(), operation: entry.operation, sessionId: entry.sessionId, messageSeq: entry.messageSeq, payloadJson: entry.payloadJson, checksum, ...(entry.image ? { imageBase64: encoded } : {}) };
  if (new TextEncoder().encode(JSON.stringify(request)).length > MAX_NATIVE_MESSAGE_BYTES) throw new Error("Ảnh quá lớn cho Native Messaging.");
  return request;
}
