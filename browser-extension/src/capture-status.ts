import type { CaptureView } from "./types";

export function captureStalled(view: CaptureView, startedAt: number, now = Date.now()): boolean {
  const clock = view.clock;
  return !!(view.connected && view.session?.status === "recording" && clock && !clock.paused && !clock.waiting && !clock.seeking && !clock.ended && clock.readyState >= 3 && now - (view.lastObservationAt ?? startedAt) >= 3000);
}

export function captureStatus(view: CaptureView, now = Date.now()): { label: string; detail: string; tone: "ok" | "warning" | "idle" } {
  if (view.session?.status === "paused" || view.session?.status === "interrupted") return { label: "Đã tạm dừng thu", detail: "Không thu thêm ảnh hoặc timestamp.", tone: "warning" };
  if (view.session?.status === "recording") {
    if (!view.connected) return { label: "Mất capture", detail: "Không thu thêm ảnh hoặc timestamp.", tone: "warning" };
    if (view.clock?.ended) return { label: "Phim đã hết", detail: "Phiên thu chưa kết thúc lưu.", tone: "idle" };
    if (view.clock?.paused) return { label: "Chờ phim phát", detail: "Capture đang sẵn sàng; video đang pause.", tone: "idle" };
    if (view.clock?.waiting || view.clock?.seeking || view.clock && view.clock.readyState < 3) return { label: view.clock.seeking ? "Chờ tua xong" : "Chờ tải đệm", detail: "Chưa thu mẫu mới từ player.", tone: "idle" };
    if (view.lastObservationAt && now - view.lastObservationAt > 3000) return { label: "Không nhận mẫu mới", detail: "Phim có thể vẫn phát; capture chưa ghi nhận mẫu mới.", tone: "warning" };
    if (!view.lastObservationAt) return { label: "Chờ mẫu đầu tiên", detail: "Chưa ghi nhận ảnh hoặc observation playback.", tone: "idle" };
    return { label: "Đang thu", detail: `${view.session.sampleCount} ảnh${view.session.schemaVersion === 2 ? ` · ${view.session.observationCount ?? 0} lần quan sát đã ghi bộ đệm` : ""}`, tone: "ok" };
  }
  if (view.session?.status === "stopped") return { label: view.session.native?.finalized ? "Đã lưu ảnh và timestamp" : "Chờ lưu", detail: view.session.native?.finalized ? "Phiên thu đã kết thúc." : "Chưa nhận xác nhận lưu cuối từ helper.", tone: "idle" };
  return { label: view.connected ? "Sẵn sàng" : "Chưa kết nối capture", detail: view.connected ? "Chưa bắt đầu thu." : "Không thu ảnh hoặc timestamp.", tone: "idle" };
}
