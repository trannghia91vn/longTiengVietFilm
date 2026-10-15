import type { Clock, Region, Size } from "./types";

export function validRegion(region: Region): boolean {
  return Object.values(region).every(Number.isFinite) && region.x >= 0 && region.y >= 0 && region.width >= 0.01 && region.height >= 0.01 && region.x + region.width <= 1.000001 && region.y + region.height <= 1.000001;
}
export function regionPixels(region: Region, size: Size): Region {
  if (!validRegion(region) || size.width < 1 || size.height < 1) throw new Error("Vung chon khong hop le.");
  const x = Math.floor(region.x * size.width);
  const y = Math.floor(region.y * size.height);
  return { x, y, width: Math.min(size.width - x, Math.max(1, Math.round(region.width * size.width))), height: Math.min(size.height - y, Math.max(1, Math.round(region.height * size.height))) };
}
export function dragRegion(from: { x: number; y: number }, to: { x: number; y: number }): Region {
  const clamp = (value: number) => Math.max(0, Math.min(1, value));
  const a = { x: clamp(from.x), y: clamp(from.y) };
  const b = { x: clamp(to.x), y: clamp(to.y) };
  return { x: Math.min(a.x, b.x), y: Math.min(a.y, b.y), width: Math.abs(a.x - b.x), height: Math.abs(a.y - b.y) };
}
export function matchClock(clock: Clock, capturedEpochMs: number, roundTripMs: number) {
  if (![clock.mediaTime, clock.epochMs, clock.rate, capturedEpochMs, roundTripMs].every(Number.isFinite) || clock.mediaTime < 0 || clock.rate <= 0 || roundTripMs < 0 || (clock.duration !== null && (!Number.isFinite(clock.duration) || clock.duration <= 0))) throw new Error("Clock video khong hop le.");
  const ageMs = clock.epochMs - capturedEpochMs;
  if (Math.abs(ageMs) > 600 || roundTripMs > 500) throw new Error("Clock cham; hay thu lai khi player on dinh.");
  if (clock.seeking || clock.waiting || clock.readyState < 2) throw new Error("Player dang tua hoac tai dem.");
  const moving = !clock.paused && !clock.ended;
  const estimate = clock.mediaTime * 1000 - (moving ? ageMs * clock.rate : 0);
  const durationMs = clock.duration === null ? Infinity : clock.duration * 1000;
  return {
    estimatedVideoTimeMs: Math.round(Math.min(durationMs, Math.max(0, estimate))),
    // Tab capture has an unmeasured compositor delay; this is a lower bound, not frame-exact timing.
    uncertaintyMs: Math.ceil(100 + roundTripMs / 2 + Math.abs(ageMs)),
  };
}
export function discontinuity(previous: Clock | null, current: Clock): string | null {
  if (!previous) return null;
  if (previous.videoId !== current.videoId || previous.sourceKey !== current.sourceKey) return "source-changed";
  if (previous.layout !== current.layout) return "layout-changed";
  if (current.seeking || previous.seeking) return "seek";
  if (previous.rate !== current.rate) return "rate-changed";
  const elapsed = Math.max(0, current.epochMs - previous.epochMs) / 1000;
  const expected = previous.mediaTime + (!previous.paused && !previous.waiting ? elapsed * previous.rate : 0);
  if (Math.abs(current.mediaTime - expected) > 0.75) return "seek";
  return null;
}
export function layoutMismatch(sourceBefore: string | null, sourceNow: string, topBefore: string | null, topNow: string | null): string | null {
  const changes: string[] = [];
  if (sourceBefore !== sourceNow) changes.push(`player ${String(sourceBefore).slice(0, 700)} -> ${sourceNow.slice(0, 700)}`);
  if (topBefore !== null && topBefore !== topNow) changes.push(`tab ${topBefore.slice(0, 700)} -> ${String(topNow).slice(0, 700)}`);
  return changes.length ? `Player, scroll hoac fullscreen da doi. Chon lai vung subtitle. [${changes.join("; ")}]` : null;
}
export function formatTime(ms: number | null): string {
  if (ms === null || !Number.isFinite(ms)) return "--:--.---";
  const time = Math.max(0, Math.round(ms));
  return `${String(Math.floor(time / 60000)).padStart(2, "0")}:${String(Math.floor(time / 1000) % 60).padStart(2, "0")}.${String(time % 1000).padStart(3, "0")}`;
}
