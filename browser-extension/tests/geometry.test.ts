import test from "node:test";
import assert from "node:assert/strict";
import { anchorToPlayer, captureSizeForViewport, playerClip, projectPlayer, regionFromAnchor, selectedChain, SelectedPlayer } from "../src/player-geometry";
import { captureStalled, captureStatus } from "../src/capture-status";
import { requestNavigationAccess } from "../src/navigation-permission";
import type { CaptureView, Clock, LocalGeometry, Reply, VideoSource } from "../src/types";

const local: LocalGeometry = { viewport: { width: 800, height: 600 }, rect: { x: 0, y: 75, width: 800, height: 450 }, clip: { x: 0, y: 75, width: 800, height: 450 }, fullscreen: false };
const parent: LocalGeometry = { viewport: { width: 1200, height: 900 }, rect: { x: 100, y: 180, width: 800, height: 600 }, clip: { x: 100, y: 180, width: 800, height: 600 }, fullscreen: false };
const source: VideoSource = { id: "child:video", frameId: 8, documentId: "child", origin: "https://player.test", clock: {} as Clock, seenAt: 1 };
const frames = [{ frameId: 0, documentId: "top", parentFrameId: -1 }, { frameId: 8, documentId: "child", parentFrameId: 0, parentDocumentId: "top" }, { frameId: 12, documentId: "ad", parentFrameId: 0 }];
test("capture constraints preserve source viewport aspect, DPR and native size limits", () => {
  for (const [width, height, dpr] of [[1800, 1000, 2], [1440, 900, 2], [1200, 1200, 2], [2560, 900, 2], [960, 540, 1]]) {
    const size = captureSizeForViewport({ width, height }, dpr);
    assert.ok(size.width <= 1920 && size.height <= 1080);
    assert.ok(Math.abs(size.width / size.height - width / height) < .01);
    assert.equal(size.width % 2, 0); assert.equal(size.height % 2, 0);
  }
  assert.deepEqual(captureSizeForViewport({ width: 1440, height: 900 }, 2), { width: 1728, height: 1080 });
  assert.throws(() => captureSizeForViewport({ width: 0, height: 900 }, 2));
});
test("selected chain excludes advertisements and fails closed on navigation/cycles/missing parents", () => {
  assert.deepEqual(selectedChain(frames, source).map(f => f.frameId), [8, 0]);
  assert.throws(() => selectedChain(frames, { frameId: 8, documentId: "replaced" }));
  assert.throws(() => selectedChain(frames.slice(1), source));
  assert.throws(() => selectedChain([{ ...frames[1], parentFrameId: 8 }], source));
  assert.throws(() => selectedChain([{ ...frames[0] }, { ...frames[1], parentDocumentId: "old-top" }], source));
});
test("nested border/scale/scroll geometry maps ROI to video and never includes letterbox or clipped pixels", () => {
  const geometry = projectPlayer(local, [parent]);
  assert.deepEqual(geometry.rect, { x: 100, y: 255, width: 800, height: 450 });
  const region = { x: 200 / 1200, y: 600 / 900, width: 600 / 1200, height: 60 / 900 };
  const anchor = anchorToPlayer(region, geometry);
  const restored = regionFromAnchor(anchor, geometry);
  for (const key of Object.keys(region) as (keyof typeof region)[]) assert.ok(Math.abs(region[key] - restored[key]) < 1e-12);
  assert.throws(() => anchorToPlayer({ ...region, y: 200 / 900 }, geometry));
  const scaled = projectPlayer(local, [{ ...parent, rect: { ...parent.rect, width: 400, height: 300 }, clip: { ...parent.rect, width: 400, height: 300 } }]);
  assert.deepEqual(scaled.rect, { x: 100, y: 217.5, width: 400, height: 225 });
  const nested = projectPlayer(local, [parent, { ...parent, viewport: { width: 1400, height: 1000 }, rect: { x: 10, y: 20, width: 1200, height: 900 }, clip: { x: 10, y: 20, width: 1200, height: 900 } }]);
  assert.deepEqual(nested.rect, { x: 110, y: 275, width: 800, height: 450 });
  assert.throws(() => projectPlayer({ ...local, clip: { ...local.clip, width: 0 } }, []));
});
test("fullscreen top layer and root viewport ignore scrolled HTML clipping at the subtitle bottom", () => {
  const viewport = { width: 1219, height: 1143 };
  const frame = { rect: { x: 0, y: 0, ...viewport }, clipX: true, clipY: true, rootViewport: false, element: true, fullscreenRoot: true };
  const html = { rect: { x: 0, y: -344, ...viewport }, clipX: true, clipY: true, rootViewport: true, element: false, fullscreenRoot: false };
  assert.deepEqual(playerClip(viewport, [frame, html]), { x: 0, y: 0, ...viewport });
  assert.deepEqual(playerClip(viewport, [{ ...frame, fullscreenRoot: false }, html]), { x: 0, y: 0, ...viewport });
  const outsideLayer = { ...html, rootViewport: false, rect: { x: 56, y: 314, width: 786, height: 435 } };
  assert.deepEqual(playerClip(viewport, [frame, outsideLayer, html]), { x: 0, y: 0, ...viewport });
  const clip = playerClip(viewport, [{ ...frame, fullscreenRoot: false }, outsideLayer, html]);
  assert.deepEqual(clip, outsideLayer.rect);
  const video = { viewport, rect: { x: 0, y: 229, width: 1219, height: 686 }, clip: { x: 0, y: 229, width: 1219, height: 686 }, fullscreen: true };
  assert.doesNotThrow(() => anchorToPlayer({ x: .2, y: 830 / 1143, width: .6, height: 60 / 1143 }, projectPlayer(video, [])));
});
test("only the chosen document can resolve a frame lookup; ad resizing is ignored, selected frame moving is detected", async () => {
  let player: SelectedPlayer;
  let adSize = 10, box = structuredClone(parent), replacement = false;
  player = new SelectedPlayer({
    frames: async () => frames.map(f => ({ ...f, adSize })),
    send: async <T>(_tabId: number, frame: { frameId: number; documentId: string }, message: unknown): Promise<Reply<T>> => {
      const m = message as { kind: string; nonce: string };
      if (m.kind === "frame-locate") {
        assert.equal(player.acceptLink(1, { frameId: 12, documentId: "ad" }, m.nonce, "aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa"), false);
        assert.equal(player.acceptLink(2, source, m.nonce, "aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa"), false);
        assert.equal(player.acceptLink(1, { ...source, documentId: "new-child" }, m.nonce, "aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa"), false);
        player.acceptLink(1, source, m.nonce, "aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa");
        return { ok: true, value: null as T };
      }
      if (replacement) return { ok: false, error: "Document/element replaced" };
      if (m.kind === "frame-geometry") { assert.equal(frame.documentId, "top"); return { ok: true, value: box as T }; }
      return { ok: true, value: { videoId: "video", layout: JSON.stringify(local), geometry: local } as T };
    },
  });
  await player.select(1, source);
  const before = await player.probe(1, source); assert.ok(before.ok);
  adSize = 2000;
  const after = await player.probe(1, source); assert.ok(after.ok); assert.equal(before.value.layout, after.value.layout);
  box = { ...parent, rect: { ...parent.rect, y: 181 } };
  const moved = await player.probe(1, source); assert.ok(moved.ok); assert.notEqual(before.value.layout, moved.value.layout);
  replacement = true; assert.equal((await player.probe(1, source)).ok, false);
  player.clear(); assert.equal((await player.probe(1, source)).ok, false);
});
test("direct video needs no navigation permission or parent lookup", async () => {
  const direct = { ...source, frameId: 0, documentId: "top", id: "top:video" };
  const player = new SelectedPlayer({ frames: async () => { throw Error("Must not request navigation"); }, send: async <T>(): Promise<Reply<T>> => ({ ok: true, value: { layout: "video", geometry: local } as T }) });
  await player.select(1, direct); assert.equal((await player.probe(1, direct)).ok, true);
});
test("an earlier selection cannot overwrite a newer player while navigation lookup is pending", async () => {
  let finish!: (value: typeof frames) => void;
  const player = new SelectedPlayer({ frames: () => new Promise(resolve => { finish = resolve; }), send: async <T>(): Promise<Reply<T>> => ({ ok: true, value: { layout: "video", geometry: local } as T }) });
  const pending = player.select(1, source);
  const rejection = assert.rejects(pending, /Đã đổi lựa chọn/);
  const direct = { ...source, frameId: 0, documentId: "top", id: "top:video" };
  await player.select(1, direct); finish(frames); await rejection;
  assert.equal((await player.probe(1, direct)).ok, true);
});
test("capture status never calls paused, buffering, first-frame or stalled capture recording", () => {
  const view = { connected: true, session: { status: "recording", schemaVersion: 2, sampleCount: 3, observationCount: 20 }, clock: { paused: false, waiting: false, readyState: 4 }, lastObservationAt: 10000 } as CaptureView;
  assert.equal(captureStatus(view, 10100).label, "Đang thu");
  assert.equal(captureStatus({ ...view, lastObservationAt: null }, 10100).label, "Chờ mẫu đầu tiên");
  assert.equal(captureStatus(view, 14000).tone, "warning");
  assert.equal(captureStatus({ ...view, clock: { ...view.clock!, paused: true } }, 14000).label, "Chờ phim phát");
  assert.equal(captureStatus({ ...view, clock: { ...view.clock!, waiting: true } }, 14000).label, "Chờ tải đệm");
  assert.equal(captureStatus({ ...view, session: { ...view.session!, status: "paused" } }).label, "Đã tạm dừng thu");
  assert.equal(captureStatus({ ...view, connected: false }).tone, "warning");
  assert.equal(captureStalled(view, 10000, 12999), false);
  assert.equal(captureStalled(view, 10000, 13000), true);
  assert.equal(captureStalled({ ...view, lastObservationAt: null }, 11000, 13999), false);
  assert.equal(captureStalled({ ...view, clock: { ...view.clock!, paused: true } }, 10000, 14000), false);
  assert.equal(captureStalled({ ...view, clock: { ...view.clock!, readyState: 2 } }, 10000, 14000), false);
});
test("permission request rejects stale manifest before Chrome API, handles denial and reports reload mismatch", async () => {
  let calls = 0;
  await assert.rejects(requestNavigationAccess({ manifest: () => ({ version: "0.3.0" }), request: async () => { calls++; return true; } }), /chưa khai báo/);
  assert.equal(calls, 0);
  await requestNavigationAccess({ manifest: () => ({ version: "0.3.1", optional_permissions: ["webNavigation"] }), request: async () => { calls++; return true; } });
  assert.equal(calls, 1);
  await assert.rejects(requestNavigationAccess({ manifest: () => ({ optional_permissions: ["webNavigation"] }), request: async () => false }), /Chưa cấp quyền/);
  await assert.rejects(requestNavigationAccess({ manifest: () => ({ version: "0.3.1", optional_permissions: ["webNavigation"] }), request: async () => { throw Error("Only permissions specified in the manifest may be requested."); } }), /Chrome chưa cập nhật/);
});
