import { intersect, playerClip, type ClipAncestor } from "./player-geometry";
import type { LocalGeometry, Region } from "./types";

function box(element: HTMLElement): Region {
  const r = element.getBoundingClientRect();
  return { x: r.x, y: r.y, width: r.width, height: r.height };
}
function number(value: string): number { return Number.parseFloat(value) || 0; }
function round(r: Region): Region { return { x: Math.round(r.x), y: Math.round(r.y), width: Math.round(r.width), height: Math.round(r.height) }; }
export function elementGeometry(element: HTMLVideoElement | HTMLIFrameElement): LocalGeometry {
  if (!element.isConnected) throw new Error("Video/iframe đã bị thay thế.");
  const fullscreen = document.fullscreenElement ?? element.closest(":fullscreen");
  if (fullscreen && !fullscreen.contains(element)) throw new Error("Player không nằm trong phần đang fullscreen.");
  const ancestors: ClipAncestor[] = [];
  for (let parent: HTMLElement | null = element; parent; parent = parent.parentElement ?? (parent.getRootNode() instanceof ShadowRoot ? (parent.getRootNode() as ShadowRoot).host as HTMLElement : null)) {
    const style = getComputedStyle(parent);
    if (style.display === "none" || style.visibility !== "visible" || number(style.opacity) === 0) throw new Error("Player đang ẩn.");
    if (style.transform !== "none") {
      const matrix = new DOMMatrixReadOnly(style.transform);
      if (!matrix.is2D || Math.abs(matrix.b) > .00001 || Math.abs(matrix.c) > .00001 || matrix.a <= 0 || matrix.d <= 0) throw new Error("Player có phép xoay/phối cảnh chưa hỗ trợ.");
    }
    ancestors.push({ rect: box(parent), clipX: style.overflowX !== "visible", clipY: style.overflowY !== "visible", rootViewport: parent === document.documentElement, element: parent === element, fullscreenRoot: parent === fullscreen });
    // Fullscreen's top layer is not clipped/transformed by the scrolled page behind it.
    if (parent === fullscreen) break;
  }
  let clip = playerClip({ width: innerWidth, height: innerHeight }, ancestors);
  const style = getComputedStyle(element), outer = box(element);
  const sx = outer.width / element.offsetWidth, sy = outer.height / element.offsetHeight;
  let rect: Region = {
    x: outer.x + (element.clientLeft + number(style.paddingLeft)) * sx,
    y: outer.y + (element.clientTop + number(style.paddingTop)) * sy,
    width: (element.clientWidth - number(style.paddingLeft) - number(style.paddingRight)) * sx,
    height: (element.clientHeight - number(style.paddingTop) - number(style.paddingBottom)) * sy,
  };
  if (element instanceof HTMLVideoElement && ["contain", "scale-down"].includes(style.objectFit)) {
    if (style.objectPosition !== "50% 50%") throw new Error("Vị trí letterbox video chưa hỗ trợ; dùng player căn giữa.");
    let scale = Math.min(rect.width / element.videoWidth, rect.height / element.videoHeight);
    if (style.objectFit === "scale-down") scale = Math.min(scale, 1);
    const width = element.videoWidth * scale, height = element.videoHeight * scale;
    rect = { x: rect.x + (rect.width - width) / 2, y: rect.y + (rect.height - height) / 2, width, height };
  } else if (element instanceof HTMLVideoElement && style.objectFit !== "fill") throw new Error("Kiểu crop video chưa hỗ trợ; chọn player contain/fill.");
  rect = round(rect); clip = round(intersect(clip, rect));
  if (![rect.x, rect.y, rect.width, rect.height, clip.width, clip.height].every(Number.isFinite) || rect.width <= 0 || rect.height <= 0 || clip.width <= 0 || clip.height <= 0) throw new Error("Player nằm ngoài vùng đang hiển thị.");
  return { viewport: { width: innerWidth, height: innerHeight }, rect, clip, fullscreen: !!fullscreen };
}
