import { validRegion } from "./core";
import type { Clock, LocalGeometry, PlayerGeometry, Region, Reply, Size, VideoSource } from "./types";

export function captureSizeForViewport(viewport: Size, pixelRatio: number): Size {
  if (![viewport.width, viewport.height, pixelRatio].every(n => Number.isFinite(n) && n > 0)) throw new Error("Không đọc được kích thước tab phim.");
  const scale = Math.min(pixelRatio, 1920 / viewport.width, 1080 / viewport.height);
  const even = (n: number) => Math.max(2, Math.floor(n / 2) * 2);
  return { width: even(viewport.width * scale), height: even(viewport.height * scale) };
}

export interface FrameIdentity { frameId: number; documentId: string }
export interface FrameNode extends FrameIdentity { parentFrameId: number; parentDocumentId?: string; errorOccurred?: boolean; documentLifecycle?: string }
export interface FrameLink { parent: FrameIdentity; child: FrameIdentity; elementId: string }

export function selectedChain(frames: FrameNode[], source: FrameIdentity): FrameIdentity[] {
  const chain: FrameIdentity[] = [];
  let next: FrameIdentity | undefined = source;
  while (next) {
    const frame: FrameNode | undefined = frames.find(f => f.frameId === next!.frameId && f.documentId === next!.documentId);
    if (!frame || frame.errorOccurred || frame.documentLifecycle && frame.documentLifecycle !== "active" || chain.some(f => f.frameId === frame.frameId) || chain.length >= 16) throw new Error("Chuỗi iframe player đã đổi hoặc không truy cập được. Quét lại video.");
    chain.push({ frameId: frame.frameId, documentId: frame.documentId });
    if (frame.frameId === 0 && frame.parentFrameId === -1) break;
    const parent = frames.find(f => f.frameId === frame.parentFrameId);
    if (!parent || frame.parentDocumentId && frame.parentDocumentId !== parent.documentId) throw new Error("Iframe cha đã đổi. Quét lại video.");
    next = parent;
  }
  if (chain.at(-1)?.frameId !== 0) throw new Error("Không xác định được tab chứa player.");
  return chain;
}
function rectangle(r: Region): boolean { return !!r && [r.x, r.y, r.width, r.height].every(Number.isFinite) && r.width > 0 && r.height > 0; }
export function checkedGeometry(g: LocalGeometry): LocalGeometry {
  if (!g || !rectangle(g.rect) || !rectangle(g.clip) || !g.viewport || ![g.viewport.width, g.viewport.height].every(n => Number.isFinite(n) && n > 0) || typeof g.fullscreen !== "boolean") throw new Error("Hình học player không hợp lệ hoặc bị che khuất.");
  return g;
}
export function intersect(a: Region, b: Region): Region {
  const x = Math.max(a.x, b.x), y = Math.max(a.y, b.y);
  return { x, y, width: Math.max(0, Math.min(a.x + a.width, b.x + b.width) - x), height: Math.max(0, Math.min(a.y + a.height, b.y + b.height) - y) };
}
export interface ClipAncestor { rect: Region; clipX: boolean; clipY: boolean; rootViewport: boolean; element: boolean; fullscreenRoot: boolean }
export function playerClip(viewport: Size, ancestors: ClipAncestor[]): Region {
  let clip: Region = { x: 0, y: 0, ...viewport };
  for (const a of ancestors) {
    if (!a.element && !a.rootViewport) {
      if (a.clipX) clip = intersect(clip, { x: a.rect.x, y: clip.y, width: a.rect.width, height: clip.height });
      if (a.clipY) clip = intersect(clip, { x: clip.x, y: a.rect.y, width: clip.width, height: a.rect.height });
    }
    if (a.fullscreenRoot) break;
  }
  return clip;
}
export function projectPlayer(local: LocalGeometry, parents: LocalGeometry[]): PlayerGeometry {
  let { rect, clip, viewport } = checkedGeometry(local);
  for (const input of parents) {
    const parent = checkedGeometry(input);
    const sx = parent.rect.width / viewport.width, sy = parent.rect.height / viewport.height;
    const project = (r: Region): Region => ({ x: parent.rect.x + r.x * sx, y: parent.rect.y + r.y * sy, width: r.width * sx, height: r.height * sy });
    rect = project(rect); clip = intersect(project(clip), parent.clip); viewport = parent.viewport;
  }
  clip = intersect(clip, { x: 0, y: 0, ...viewport });
  if (!rectangle(clip)) throw new Error("Player nằm ngoài vùng đang hiển thị.");
  return { rect, clip, viewport };
}
export function anchorToPlayer(region: Region, geometry: PlayerGeometry): Region {
  if (!validRegion(region)) throw new Error("Vùng phụ đề không hợp lệ.");
  const r = { x: region.x * geometry.viewport.width, y: region.y * geometry.viewport.height, width: region.width * geometry.viewport.width, height: region.height * geometry.viewport.height };
  const allowed = intersect(geometry.rect, geometry.clip);
  if (r.x < allowed.x - .5 || r.y < allowed.y - .5 || r.x + r.width > allowed.x + allowed.width + .5 || r.y + r.height > allowed.y + allowed.height + .5) throw new Error("Chọn vùng phụ đề bên trong hình video đang hiển thị.");
  return { x: (r.x - geometry.rect.x) / geometry.rect.width, y: (r.y - geometry.rect.y) / geometry.rect.height, width: r.width / geometry.rect.width, height: r.height / geometry.rect.height };
}
export function regionFromAnchor(anchor: Region, geometry: PlayerGeometry): Region {
  const region = { x: (geometry.rect.x + anchor.x * geometry.rect.width) / geometry.viewport.width, y: (geometry.rect.y + anchor.y * geometry.rect.height) / geometry.viewport.height, width: anchor.width * geometry.rect.width / geometry.viewport.width, height: anchor.height * geometry.rect.height / geometry.viewport.height };
  anchorToPlayer(region, geometry);
  return region;
}

export interface GeometryTransport {
  frames(tabId: number): Promise<FrameNode[]>;
  send<T>(tabId: number, frame: FrameIdentity, message: unknown): Promise<Reply<T>>;
}
interface PendingLink { tabId: number; child: FrameIdentity; parent: FrameIdentity; finish(elementId?: string, error?: Error): void }

// Window messages identify an iframe element once; geometry itself is read via extension messaging.
export class SelectedPlayer {
  private selection: { tabId: number; source: VideoSource; links: FrameLink[] } | null = null;
  private pending = new Map<string, PendingLink>();
  private generation = 0;
  constructor(private transport: GeometryTransport) {}
  clear() { this.generation++; this.selection = null; for (const p of this.pending.values()) p.finish(undefined, new Error("Đã đổi tab/player.")); }
  acceptLink(tabId: number, frame: FrameIdentity, nonce: string, elementId: string): boolean {
    const request = this.pending.get(nonce);
    if (!request || tabId !== request.tabId || frame.frameId !== request.child.frameId || frame.documentId !== request.child.documentId || typeof elementId !== "string" || !/^[a-f0-9-]{36}$/.test(elementId)) return false;
    request.finish(elementId); return true;
  }
  private locate(tabId: number, parent: FrameIdentity, child: FrameIdentity): Promise<string> {
    const nonce = crypto.randomUUID();
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => finish(undefined, new Error("Không liên kết được iframe player. Cấp quyền miền iframe cha rồi quét lại.")), 1500);
      const finish = (elementId?: string, error?: Error) => {
        if (!this.pending.delete(nonce)) return;
        clearTimeout(timer); if (error) reject(error); else resolve(elementId!);
      };
      this.pending.set(nonce, { tabId, child, parent, finish });
      void this.transport.send(tabId, parent, { kind: "frame-locate", nonce }).then(reply => {
        if (!reply.ok) finish(undefined, new Error(reply.error));
      }).catch(error => finish(undefined, new Error(String(error))));
    });
  }
  async select(tabId: number, source: VideoSource): Promise<void> {
    this.clear();
    const generation = this.generation;
    const chain = source.frameId === 0 ? [source] : selectedChain(await this.transport.frames(tabId), source);
    if (generation !== this.generation) throw new Error("Đã đổi lựa chọn player.");
    const links: FrameLink[] = [];
    for (let i = 0; i + 1 < chain.length; i++) {
      links.push({ child: chain[i], parent: chain[i + 1], elementId: await this.locate(tabId, chain[i + 1], chain[i]) });
      if (generation !== this.generation) throw new Error("Đã đổi lựa chọn player.");
    }
    this.selection = { tabId, source, links };
  }
  async probe(tabId: number, source: VideoSource): Promise<Reply<Clock>> {
    const selection = this.selection;
    if (!selection || selection.tabId !== tabId || selection.source.id !== source.id) return { ok: false, error: "Liên kết player đã mất. Quét và chọn lại video." };
    try {
      const [reply, ...parents] = await Promise.all([
        this.transport.send<Clock>(tabId, source, { kind: "clock-probe", videoId: source.clock.videoId }),
        ...selection.links.map(link => this.transport.send<LocalGeometry>(tabId, link.parent, { kind: "frame-geometry", elementId: link.elementId })),
      ]);
      if (!reply.ok) return reply;
      const clock = reply.value as Clock;
      const local = checkedGeometry(clock.geometry as LocalGeometry);
      const boxes = parents.map(p => { if (!p.ok) throw new Error(p.error); return checkedGeometry(p.value as LocalGeometry); });
      return { ok: true, value: { ...clock, geometry: projectPlayer(local, boxes), layout: JSON.stringify([clock.layout, selection.links.map((link, i) => [link.parent.documentId, link.elementId, boxes[i]])]) } };
    } catch (error) { return { ok: false, error: `Không xác minh được vùng player: ${String(error)}` }; }
  }
}
