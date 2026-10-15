import { recompressPng } from "./png-compression";

self.onmessage = (event: MessageEvent<ArrayBuffer>) => {
  let bytes = new Uint8Array(event.data);
  try { bytes = recompressPng(bytes); } catch { /* Keep the original canvas PNG on any encoder failure. */ }
  self.postMessage(bytes.buffer, { transfer: [bytes.buffer] });
};
