import { unzlibSync, zlibSync } from "fflate";

const MAGIC = [137, 80, 78, 71, 13, 10, 26, 10];
const MAX_BYTES = 16 * 1024 * 1024;
const crcTable = Uint32Array.from({ length: 256 }, (_, n) => {
  for (let i = 0; i < 8; i++) n = n & 1 ? 0xedb88320 ^ (n >>> 1) : n >>> 1;
  return n >>> 0;
});
function crc(bytes: Uint8Array) {
  let value = 0xffffffff;
  for (const byte of bytes) value = crcTable[(value ^ byte) & 255] ^ (value >>> 8);
  return (value ^ 0xffffffff) >>> 0;
}
function adler(bytes: Uint8Array) {
  let a = 1, b = 0;
  for (let start = 0; start < bytes.length; start += 5552) {
    for (let i = start; i < Math.min(start + 5552, bytes.length); i++) { a += bytes[i]; b += a; }
    a %= 65521; b %= 65521;
  }
  return ((b << 16) | a) >>> 0;
}
function chunk(data: Uint8Array) {
  const bytes = new Uint8Array(data.length + 12), view = new DataView(bytes.buffer);
  view.setUint32(0, data.length);
  bytes.set([73, 68, 65, 84], 4); bytes.set(data, 8);
  view.setUint32(data.length + 8, crc(bytes.subarray(4, data.length + 8)));
  return bytes;
}

// Repack only IDAT's zlib stream; every filtered pixel byte and other PNG chunk stays intact.
export function recompressPng(input: Uint8Array): Uint8Array {
  if (input.length < 45 || input.length > MAX_BYTES || MAGIC.some((byte, i) => input[i] !== byte)) throw new Error("Invalid PNG");
  const view = new DataView(input.buffer, input.byteOffset, input.byteLength);
  const chunks: { type: string; bytes: Uint8Array; data: Uint8Array }[] = [];
  let ended = false, expected = 0, idatClosed = false;
  for (let offset = 8; offset < input.length;) {
    if (ended || offset + 12 > input.length) throw new Error("Invalid PNG tail");
    const length = view.getUint32(offset), end = offset + length + 12;
    if (end > input.length) throw new Error("Invalid PNG chunk");
    const type = String.fromCharCode(...input.subarray(offset + 4, offset + 8));
    if (crc(input.subarray(offset + 4, end - 4)) !== view.getUint32(end - 4)) throw new Error("Invalid PNG CRC");
    if (!chunks.length) {
      if (type !== "IHDR" || length !== 13) throw new Error("Invalid PNG header");
      const width = view.getUint32(offset + 8), height = view.getUint32(offset + 12);
      const depth = input[offset + 16], color = input[offset + 17];
      if (!width || !height || width > 1920 || height > 1080 || depth !== 8 || ![2, 6].includes(color) || input[offset + 18] || input[offset + 19] || input[offset + 20]) throw new Error("Unsupported PNG format");
      expected = (width * (color === 6 ? 4 : 3) + 1) * height;
    } else if (type === "IHDR") throw new Error("Duplicate PNG header");
    if (type === "IDAT" && idatClosed) throw new Error("Non-contiguous PNG data");
    if (type !== "IDAT" && chunks.some(c => c.type === "IDAT")) idatClosed = true;
    if (type === "IEND") { if (length !== 0) throw new Error("Invalid PNG end"); ended = true; }
    chunks.push({ type, bytes: input.subarray(offset, end), data: input.subarray(offset + 8, end - 4) });
    offset = end;
  }
  const idats = chunks.filter(c => c.type === "IDAT");
  if (!ended || !idats.length) throw new Error("Incomplete PNG");
  const compressed = new Uint8Array(idats.reduce((sum, c) => sum + c.data.length, 0));
  let offset = 0;
  for (const c of idats) { compressed.set(c.data, offset); offset += c.data.length; }
  if (compressed.length < 6) throw new Error("Invalid PNG stream");
  const filtered = unzlibSync(compressed, { out: new Uint8Array(expected) });
  if (adler(filtered) !== new DataView(compressed.buffer).getUint32(compressed.length - 4)) throw new Error("Invalid PNG pixels");
  const packed = chunk(zlibSync(filtered, { level: 6 }));
  const size = input.length - idats.reduce((sum, c) => sum + c.bytes.length, 0) + packed.length;
  if (size >= input.length) return input;
  const output = new Uint8Array(size);
  output.set(MAGIC); offset = 8;
  let written = false;
  for (const c of chunks) {
    if (c.type === "IDAT") {
      if (written) continue;
      output.set(packed, offset); offset += packed.length; written = true;
    } else { output.set(c.bytes, offset); offset += c.bytes.length; }
  }
  return output;
}
