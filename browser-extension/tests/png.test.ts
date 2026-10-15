import test from "node:test";
import assert from "node:assert/strict";
import { zlibSync, unzlibSync } from "fflate";
import { recompressPng } from "../src/png-compression";
import { compactPng } from "../src/png-encoder";

function crc(bytes: Uint8Array) {
  let value = 0xffffffff;
  for (const byte of bytes) {
    value ^= byte;
    for (let bit = 0; bit < 8; bit++) value = value & 1 ? (value >>> 1) ^ 0xedb88320 : value >>> 1;
  }
  return (value ^ 0xffffffff) >>> 0;
}
function chunk(type: string, data: Uint8Array) {
  const out = new Uint8Array(data.length + 12), view = new DataView(out.buffer);
  view.setUint32(0, data.length); out.set([...type].map(c => c.charCodeAt(0)), 4); out.set(data, 8);
  view.setUint32(data.length + 8, crc(out.subarray(4, data.length + 8)));
  return out;
}
function concat(parts: Uint8Array[]) {
  const result = new Uint8Array(parts.reduce((n, part) => n + part.length, 0));
  let offset = 0; for (const part of parts) { result.set(part, offset); offset += part.length; }
  return result;
}
function fixture(color = 6, level: 0 | 6 = 0) {
  const header = new Uint8Array(13), view = new DataView(header.buffer);
  view.setUint32(0, 96); view.setUint32(4, 32); header[8] = 8; header[9] = color;
  const filtered = new Uint8Array((96 * (color === 6 ? 4 : 3) + 1) * 32);
  // Transparent colored pixels and varied filter bytes are preserved verbatim, not flattened.
  for (let i = 0; i < filtered.length; i++) filtered[i] = i % 13;
  for (let row = 0; row < 32; row++) filtered[row * (96 * (color === 6 ? 4 : 3) + 1)] = 0;
  const data = zlibSync(filtered, { level });
  return { filtered, png: concat([new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10]), chunk("IHDR", header), chunk("tEXt", new TextEncoder().encode("source\0fixture")), chunk("IDAT", data.subarray(0, 10)), chunk("IDAT", data.subarray(10)), chunk("IEND", new Uint8Array())]) };
}
function chunks(png: Uint8Array) {
  const view = new DataView(png.buffer, png.byteOffset, png.byteLength), result = [];
  for (let offset = 8; offset < png.length;) {
    const size = view.getUint32(offset);
    assert.equal(view.getUint32(offset + size + 8), crc(png.subarray(offset + 4, offset + size + 8)));
    result.push({ type: String.fromCharCode(...png.subarray(offset + 4, offset + 8)), data: png.subarray(offset + 8, offset + size + 8) });
    offset += size + 12;
  }
  return result;
}
test("lossless PNG repack preserves all filtered pixels, header and metadata in RGB/RGBA", () => {
  for (const color of [2, 6]) {
    const { png, filtered } = fixture(color), packed = recompressPng(png);
    assert.ok(packed.length < png.length / 2);
    const before = chunks(png), after = chunks(packed);
    assert.deepEqual(unzlibSync(concat(after.filter(c => c.type === "IDAT").map(c => c.data))), filtered);
    assert.deepEqual(after.filter(c => c.type !== "IDAT"), before.filter(c => c.type !== "IDAT"));
    const again = recompressPng(packed);
    assert.equal(again, packed, "never replace a PNG with a larger output");
  }
});
test("PNG repack rejects corrupt, truncated, oversized and unsupported inputs", () => {
  const { png } = fixture();
  assert.throws(() => recompressPng(png.subarray(0, png.length - 1)));
  const damaged = png.slice(); damaged[50] ^= 1;
  assert.throws(() => recompressPng(damaged), /CRC/);
  assert.throws(() => recompressPng(new Uint8Array(16 * 1024 * 1024 + 1)));
  const parts = chunks(png), header = parts[0].data.slice(); new DataView(header.buffer).setUint32(0, 1921);
  assert.throws(() => recompressPng(concat([png.subarray(0, 8), ...parts.map((c, i) => chunk(c.type, i === 0 ? header : c.data))])), /Unsupported/);
});
test("worker failure returns original PNG and encoding is bounded to one pending task", async () => {
  const old = globalThis.Worker;
  class BrokenWorker {
    onmessage: ((event: { data: ArrayBuffer }) => void) | null = null;
    onerror: ((event: { preventDefault(): void }) => void) | null = null;
    postMessage() { queueMicrotask(() => this.onerror?.({ preventDefault() {} })); }
    terminate() {}
  }
  globalThis.Worker = BrokenWorker as unknown as typeof Worker;
  try {
    const source = new Blob([fixture().png], { type: "image/png" });
    const first = compactPng(source), second = compactPng(source);
    assert.equal(await second, source);
    assert.equal(await first, source);
    const small = new Blob(["small"]);
    assert.equal(await compactPng(small), small);
  } finally { globalThis.Worker = old; }
});
