import test from "node:test";
import assert from "node:assert/strict";
import { discontinuity, dragRegion, layoutMismatch, matchClock, regionPixels, validRegion } from "../src/core";
import type { Clock } from "../src/types";

const clock = (patch: Partial<Clock> = {}): Clock => ({ videoId: "movie", sourceKey: "episode-1", mediaTime: 42, duration: 100, rate: 1, paused: false, seeking: false, waiting: false, ended: false, readyState: 4, epochMs: 10_000, frameMediaTime: 41.96, frameEpochMs: 9990, layout: "same", width: 1920, height: 1080, ...patch });
test("region selection works in every drag direction and clamps outside image", () => {
  assert.deepEqual(dragRegion({ x: .8, y: .9 }, { x: .2, y: .7 }), { x: .2, y: .7, width: .6000000000000001, height: .20000000000000007 });
  assert.deepEqual(dragRegion({ x: -1, y: -.2 }, { x: 2, y: 4 }), { x: 0, y: 0, width: 1, height: 1 });
});
test("crop is bounded at image edges on high DPI capture", () => {
  assert.deepEqual(regionPixels({ x: .1, y: .8, width: .8, height: .2 }, { width: 3840, height: 2160 }), { x: 384, y: 1728, width: 3072, height: 432 });
  assert.equal(validRegion({ x: .9, y: 0, width: .2, height: 1 }), false);
  assert.equal(validRegion({ x: NaN, y: 0, width: .2, height: 1 }), false);
  assert.throws(() => regionPixels({ x: 0, y: 0, width: 0, height: .2 }, { width: 100, height: 100 }));
});
test("timestamp uses source media timeline, not elapsed capture or completion time", () => {
  const estimate = matchClock(clock(), 9980, 40);
  assert.equal(estimate.estimatedVideoTimeMs, 41980);
  assert.equal(estimate.uncertaintyMs, 140);
  assert.equal(matchClock(clock({ rate: 2 }), 9980, 40).estimatedVideoTimeMs, 41960);
});
test("paused video time is not extrapolated and invalid clocks fail closed", () => {
  assert.equal(matchClock(clock({ paused: true }), 9800, 30).estimatedVideoTimeMs, 42000);
  assert.throws(() => matchClock(clock({ seeking: true }), 9990, 30));
  assert.throws(() => matchClock(clock({ waiting: true }), 9990, 30));
  assert.throws(() => matchClock(clock(), 9000, 30));
  assert.throws(() => matchClock(clock(), 9990, 501));
  assert.throws(() => matchClock(clock({ mediaTime: NaN }), 9990, 30));
});
test("pause, buffering and normal playback do not create false seek", () => {
  assert.equal(discontinuity(clock(), clock({ mediaTime: 42.5, epochMs: 10500 })), null);
  assert.equal(discontinuity(clock({ paused: true }), clock({ mediaTime: 42, epochMs: 11000, paused: true })), null);
  assert.equal(discontinuity(clock({ waiting: true }), clock({ mediaTime: 42, epochMs: 11000 })), null);
});
test("seek, episode change, geometry and rate change are distinguishable", () => {
  assert.equal(discontinuity(clock(), clock({ mediaTime: 5, epochMs: 10500 })), "seek");
  assert.equal(discontinuity(clock(), clock({ sourceKey: "episode-2" })), "source-changed");
  assert.equal(discontinuity(clock(), clock({ layout: "fullscreen" })), "layout-changed");
  assert.equal(discontinuity(clock(), clock({ rate: 1.5 })), "rate-changed");
});
test("layout guard reports bounded source/top differences without relaxing ROI protection", () => {
  assert.equal(layoutMismatch("same", "same", "top", "top"), null);
  assert.equal(layoutMismatch("same", "same", null, "top"), null);
  assert.match(layoutMismatch("old", "new", "top", "top")!, /player old -> new/);
  assert.match(layoutMismatch("same", "same", "old", "new")!, /tab old -> new/);
  assert.ok(layoutMismatch("a".repeat(9000), "b".repeat(9000), "c".repeat(9000), "d".repeat(9000))!.length < 3000);
});
