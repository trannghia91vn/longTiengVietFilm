export const SMART_CAPABILITY = "smart-observations-v1";
export const SMART_RAM_BYTES = 16 * 1024 * 1024;
export interface Signature { mask: Uint8Array; supported: boolean; width: number; height: number }
export interface SmartDecision { group: number; stable: boolean; save: boolean; role: "start" | "change" | "stable" | "periodic" | "end" | "fallback" }

// Integral dark-pixel counts keep outline detection linear in the ROI's pixel count.
export function glyphSignature(rgba: Uint8ClampedArray, width: number, height: number): Signature {
  if (!Number.isInteger(width) || !Number.isInteger(height) || width < 1 || height < 1 || rgba.length !== width * height * 4 || rgba.byteLength > SMART_RAM_BYTES / 2) throw new Error("ROI vượt bộ đệm detector.");
  const gray = new Uint8Array(width * height);
  const stride = width + 1;
  const dark = new Uint32Array(stride * (height + 1));
  for (let y = 0; y < height; y++) {
    let row = 0;
    for (let x = 0; x < width; x++) {
      const i = y * width + x, p = i * 4;
      gray[i] = (rgba[p] * 77 + rgba[p + 1] * 150 + rgba[p + 2] * 29) >> 8;
      row += gray[i] < 85 ? 1 : 0;
      dark[(y + 1) * stride + x + 1] = dark[y * stride + x + 1] + row;
    }
  }
  const mask = new Uint8Array(width * height);
  let count = 0, top = height, bottom = -1;
  for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
    if (gray[y * width + x] < 130) continue;
    const l = Math.max(0, x - 3), r = Math.min(width, x + 4), t = Math.max(0, y - 3), b = Math.min(height, y + 4);
    if (dark[b * stride + r] - dark[t * stride + r] - dark[b * stride + l] + dark[t * stride + l] > 0) {
      mask[y * width + x] = 1; count++; top = Math.min(top, y); bottom = Math.max(bottom, y);
    }
  }
  const density = count / mask.length;
  if (count < 24 || density < .002 || density > .3) return { mask, width, height, supported: false };
  const band = glyphBand(mask, width, height);
  if (band) {
    count = 0; top = height; bottom = -1;
    for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
      const i = y * width + x;
      if (x < band[0] || x >= band[2] || y < band[1] || y >= band[3]) mask[i] = 0;
      else if (mask[i]) { count++; top = Math.min(top, y); bottom = Math.max(bottom, y); }
    }
  }
  // No usable glyph mask is UNKNOWN, never evidence of an empty subtitle region.
  return { mask, width, height, supported: !!band && count >= 24 && density >= .002 && density <= .3 && bottom - top + 1 <= height * .8 };
}
function glyphBand(mask: Uint8Array, w: number, h: number): [number, number, number, number] | null {
  const seen = new Uint8Array(mask.length), stack = new Int32Array(mask.length);
  const labels = new Int32Array(mask.length);
  const components: { bounds: [number, number, number, number]; count: number; label: number }[] = [];
  const glyphs: [number, number, number, number][] = [];
  for (let seed = 0; seed < mask.length; seed++) {
    if (!mask[seed] || seen[seed]) continue;
    let depth = 1, count = 0, left = seed % w, right = left, top = Math.floor(seed / w), bottom = top;
    const label = components.length + 1;
    stack[0] = seed; seen[seed] = 1;
    while (depth) {
      const i = stack[--depth], x = i % w, y = Math.floor(i / w); count++;
      labels[i] = label;
      left = Math.min(left, x); right = Math.max(right, x); top = Math.min(top, y); bottom = Math.max(bottom, y);
      for (let ny = Math.max(0, y - 1); ny <= Math.min(h - 1, y + 1); ny++) for (let nx = Math.max(0, x - 1); nx <= Math.min(w - 1, x + 1); nx++) {
        const j = ny * w + nx; if (mask[j] && !seen[j]) { seen[j] = 1; stack[depth++] = j; }
      }
    }
    const bounds: [number, number, number, number] = [left, top, right + 1, bottom + 1];
    components.push({ bounds, count, label });
    if (bottom - top + 1 >= 6 && bottom - top + 1 <= 60 && right - left + 1 <= 60 && count >= 8) glyphs.push(bounds);
    if (components.length > 4096 || glyphs.length > 512) return null;
  }
  const bands: { count: number; bounds: [number, number, number, number] }[] = [];
  for (const glyph of glyphs) {
    const center = (glyph[1] + glyph[3]) / 2;
    const aligned = glyphs.filter(g => Math.abs((g[1] + g[3]) / 2 - center) <= 8);
    if (aligned.length < 2) continue;
    const bounds: [number, number, number, number] = [Math.min(...aligned.map(g => g[0])), Math.min(...aligned.map(g => g[1])), Math.max(...aligned.map(g => g[2])), Math.max(...aligned.map(g => g[3]))];
    if (Math.abs((bounds[0] + bounds[2]) / 2 - w / 2) <= w / 4 && bounds[3] - bounds[1] <= 70) bands.push({ count: aligned.length, bounds });
  }
  const best = bands.reduce<typeof bands[number] | null>((best, band) => !best || band.count > best.count ? band : best, null);
  if (!best) return null;
  const region = [...best.bounds] as [number, number, number, number];
  for (const band of bands) if (band.count >= 4 && Math.abs(band.bounds[1] - best.bounds[1]) <= 55) {
    region[0] = Math.min(region[0], band.bounds[0]); region[1] = Math.min(region[1], band.bounds[1]); region[2] = Math.max(region[2], band.bounds[2]); region[3] = Math.max(region[3], band.bounds[3]);
  }
  const bounds: [number, number, number, number] = [Math.max(0, region[0] - 8), Math.max(0, region[1] - 8), Math.min(w, region[2] + 8), Math.min(h, region[3] + 8)];
  const main = components.filter(c => c.count >= 8 && c.bounds[3] - c.bounds[1] >= 6 && c.bounds[3] - c.bounds[1] <= 60 && c.bounds[2] - c.bounds[0] <= 60 && c.bounds[0] >= bounds[0] && c.bounds[1] >= bounds[1] && c.bounds[2] <= bounds[2] && c.bounds[3] <= bounds[3]);
  const keep = new Set(main.map(c => c.label));
  // Detached Vietnamese accents/dots must survive component filtering.
  for (const c of components) if (c.count >= 2 && c.bounds[3] - c.bounds[1] <= 6 && main.some(g => c.bounds[2] >= g.bounds[0] - 3 && c.bounds[0] <= g.bounds[2] + 3 && c.bounds[1] >= g.bounds[1] - 10 && c.bounds[3] <= g.bounds[3] + 3)) keep.add(c.label);
  for (let i = 0; i < mask.length; i++) if (!keep.has(labels[i])) mask[i] = 0;
  return bounds;
}
export function sameGlyphs(a: Signature, b: Signature): boolean {
  if (!a.supported || !b.supported || a.width !== b.width || a.height !== b.height) return false;
  let different = 0, union = 0;
  for (let i = 0; i < a.mask.length; i++) { union += a.mask[i] | b.mask[i]; different += a.mask[i] ^ b.mask[i]; }
  // Fixed anchor, strict mismatch budget: even small word/diacritic changes retain evidence.
  return union > 0 && different <= Math.min(2, union * .005);
}
export class SmartDetector {
  private anchor: Signature | null = null;
  private count = 0;
  private group = 0;
  private lastSaved = -Infinity;
  reset() { this.anchor = null; this.count = 0; this.lastSaved = -Infinity; }
  observe(signature: Signature, timeMs: number): SmartDecision {
    if (!Number.isFinite(timeMs) || timeMs < 0) throw new Error("Clock detector không hợp lệ.");
    if (!signature.supported || !this.anchor || !sameGlyphs(this.anchor, signature)) {
      this.anchor = signature.supported ? signature : null; this.count = 1; this.group++; this.lastSaved = timeMs;
      return { group: this.group, stable: false, save: true, role: signature.supported ? "start" : "fallback" };
    }
    this.count++;
    const role = this.count === 3 ? "stable" : this.count < 3 ? "change" : "periodic";
    const save = this.count <= 3 || timeMs - this.lastSaved >= 2000;
    if (save) this.lastSaved = timeMs;
    return { group: this.group, stable: this.count >= 3, save, role };
  }
}
