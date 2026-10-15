import { useEffect, useRef, useState, type PointerEvent } from "react";
import { createRoot } from "react-dom/client";
import { Camera, Check, Crop, Download, ExternalLink, FolderOpen, History, Pause, Play, PlugZap, RefreshCw, ScanLine, ScanText, Square, Wifi, WifiOff } from "lucide-react";
import { dragRegion, formatTime, validRegion } from "./core";
import { captureStatus } from "./capture-status";
import { requestNavigationAccess } from "./navigation-permission";
import { exportSession } from "./export";
import { lastSample, listSessions } from "./storage";
import type { CaptureSession, Command, NativeCommand, OcrStatus, RecorderView, Region, Reply, Sample } from "./types";
import "./recorder.css";

const empty: RecorderView = { target: null, connected: false, preview: null, previewSize: null, region: null, source: null, session: null, clock: null, error: null, busy: false, sources: [], frameOrigins: [], topLayout: null };
const previewOnly = !globalThis.chrome?.runtime?.id;
const extensionVersion = previewOnly ? "Preview" : chrome.runtime.getManifest?.().version ?? "QA";
async function request<T = unknown>(command: Command | NativeCommand): Promise<T> {
  if (previewOnly) throw new Error("Chua ket noi Chrome extension.");
  const reply: Reply<T> = await chrome.runtime.sendMessage({ kind: "command", command });
  if (!reply.ok) throw new Error(reply.error);
  return reply.value;
}
function Recorder() {
  const [view, setView] = useState(empty);
  const [sessions, setSessions] = useState<CaptureSession[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [sample, setSample] = useState<Sample | null>(null);
  const [imageUrl, setImageUrl] = useState<string | null>(null);
  const [draft, setDraft] = useState<Region | null>(null);
  const [selecting, setSelecting] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [ocr, setOcr] = useState<OcrStatus | null>(null);
  const [ocrError, setOcrError] = useState<string | null>(null);
  const [missingOrigins, setMissingOrigins] = useState<string[]>([]);
  const anchor = useRef<{ x: number; y: number } | null>(null);
  const autoSource = useRef<string | null>(null);
  const refreshed = useRef(0);
  const active = view.session?.status === "recording";
  const selected = sessions.find(session => session.id === selectedId) ?? view.session;
  const displayed = selected ?? view.session;
  const ocrRunning = ocr?.state === "queued" || ocr?.state === "processing";
  const ocrLabels: Record<OcrStatus["state"], string> = { "not-started": "Chưa xử lý", queued: "Chờ OCR", processing: "Đang OCR", interrupted: "OCR bị ngắt", cancelled: "Đã dừng OCR", error: "Lỗi OCR", partial: "Đã lưu SRT · Chưa review", "no-text": "Không nhận được chữ" };
  const nativeStatus = (session: CaptureSession) => session.native?.error ? "Lỗi lưu" : session.native?.finalized ? "Đã lưu ảnh và timestamp" : session.native && ["stopped", "interrupted"].includes(session.status) ? "Chờ lưu" : session.status === "recording" ? "Đang thu" : session.status === "paused" ? "Tạm dừng" : session.status === "interrupted" ? "Bị ngắt" : "Đã kết thúc";
  const refresh = async () => {
    try {
      const next = previewOnly ? empty : await request<RecorderView>({ action: "state" });
      setView(next);
      if (Date.now() - refreshed.current > 1500) {
        refreshed.current = Date.now();
        setSessions(await listSessions());
        const pending: string[] = [];
        if (!previewOnly) for (const origin of next.frameOrigins) {
          if (!await chrome.permissions.contains({ origins: [`${origin}/*`] })) pending.push(origin);
        }
        setMissingOrigins(pending);
      }
    } catch (failure) { setError(failure instanceof Error ? failure.message : String(failure)); }
  };
  useEffect(() => {
    void refresh();
    const timer = setInterval(() => { void refresh(); }, 600);
    return () => clearInterval(timer);
  }, []);
  useEffect(() => {
    if (!view.connected) { setSelecting(false); anchor.current = null; setDraft(null); }
  }, [view.connected]);
  useEffect(() => {
    const candidates = view.sources.filter(source => source.clock.width > 0 && source.clock.height > 0);
    if (view.connected && !view.source && candidates.length === 1 && autoSource.current !== candidates[0].id) {
      autoSource.current = candidates[0].id;
      void request({ action: "source", source: candidates[0] }).then(refresh).catch(failure => setError(String(failure)));
    }
  }, [view.connected, view.source, view.sources]);
  useEffect(() => {
    let cancelled = false;
    if (!selected) { setSample(null); return; }
    void lastSample(selected.id).then(value => { if (!cancelled) setSample(value); });
    return () => { cancelled = true; };
  }, [selected?.id, selected?.sampleCount]);
  useEffect(() => {
    let disposed = false;
    let fetching = false;
    setOcr(null); setOcrError(null);
    const fetchOcr = async () => {
      if (fetching || !displayed?.native?.finalized || previewOnly) return;
      fetching = true;
      try { const result = await request<OcrStatus>({ action: "ocr-status", sessionId: displayed.id }); if (!disposed) { setOcr(result); setOcrError(null); } }
      catch (failure) { if (!disposed) setOcrError(String(failure)); }
      finally { fetching = false; }
    };
    void fetchOcr();
    const timer = setInterval(() => { void fetchOcr(); }, 2500);
    return () => { disposed = true; clearInterval(timer); };
  }, [displayed?.id, displayed?.native?.finalized]);
  useEffect(() => {
    if (!sample) { setImageUrl(null); return; }
    const url = URL.createObjectURL(sample.image);
    setImageUrl(url);
    return () => URL.revokeObjectURL(url);
  }, [sample]);
  const run = async (task: () => Promise<unknown>) => {
    setBusy(true);
    setError(null);
    try { await task(); await refresh(); } catch (failure) { setError(failure instanceof Error ? failure.message : String(failure)); }
    finally { setBusy(false); }
  };
  const point = (event: PointerEvent<HTMLDivElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    return { x: (event.clientX - rect.left) / rect.width, y: (event.clientY - rect.top) / rect.height };
  };
  const finishRegion = (event: PointerEvent<HTMLDivElement>) => {
    if (!anchor.current) return;
    const region = dragRegion(anchor.current, point(event));
    anchor.current = null;
    setDraft(null);
    if (validRegion(region)) void run(async () => { await request({ action: "region", region }); setSelecting(false); });
  };
  const region = draft ?? view.region;
  const health = captureStatus(view);
  const needsNavigation = view.connected && !view.navigationAllowed && view.sources.some(source => source.frameId !== 0);
  return <div className="recorder-shell">
    <header className="recorder-header">
      <div className="brand"><ScanLine size={24} /><div><strong>Long Tieng</strong><span>Thu subtitle · v{extensionVersion}</span></div></div>
      <span className={`connection ${health.tone === "ok" ? "online" : health.tone === "warning" ? "warning" : ""}`}>{view.connected ? <Wifi size={15} /> : <WifiOff size={15} />}{health.label}</span>
    </header>
    <div className="recorder-layout">
      <aside className="history"><div className="history-title"><History size={17} /><strong>Phiên thu</strong><span>{sessions.length}</span></div>
        {sessions.length === 0 && <p className="empty-history">Chưa có bản ghi</p>}
        {sessions.map(saved => { const session = saved.id === view.session?.id ? view.session : saved; return <button key={session.id} className={`session-item ${selected?.id === session.id ? "selected" : ""}`} onClick={() => setSelectedId(session.id)}>
          <strong>{session.title}</strong><span>{new Date(session.createdAt).toLocaleString("vi-VN")}<br />{session.sampleCount} ảnh · {(session.imageBytes / 1048576).toFixed(1)} MB</span><small>{nativeStatus(session)}</small>
        </button>; })}
      </aside>
      <main className="workspace">
        <div className="source-row"><div><h1>{!view.connected && displayed ? displayed.title : view.target?.title ?? "Tab phim"}</h1><p>{view.source?.origin ?? displayed?.source.origin ?? view.target?.url ?? "Chưa kết nối"}</p></div>
          {view.target && <button className="icon-button" title="Mở tab phim" aria-label="Mở tab phim" onClick={() => void run(async () => { const tab = await chrome.tabs.update(view.target!.id, { active: true }); if (tab) await chrome.windows.update(tab.windowId, { focused: true }); })}><ExternalLink size={17} /></button>}
        </div>
        {(error || view.error) && <div role="alert" className="error-banner">{error ?? view.error}</div>}
        <div className="native-row"><span className={`connection ${view.native?.connected ? "online" : ""}`}><PlugZap size={16} />{view.native?.connected ? `Helper ${view.native.hostVersion ?? ""}` : "Chưa kết nối helper"}</span><button className="icon-button" title="Kết nối lại helper / thử lưu lại" aria-label="Kết nối lại helper" disabled={busy || previewOnly} onClick={() => void run(() => request({ action: "native-retry" }))}><RefreshCw size={16} /></button></div>
        {(displayed?.native?.error || view.native?.error) && <div role="alert" className="error-banner">{displayed?.native?.error ?? view.native?.error}</div>}
        {needsNavigation && <div className="permission-row"><span>Chưa có quyền liên kết iframe player</span><button disabled={busy || previewOnly || active} onClick={() => void run(async () => {
          await requestNavigationAccess({ manifest: () => chrome.runtime.getManifest(), request: () => chrome.permissions.request({ permissions: ["webNavigation"] }) });
          autoSource.current = null; await request({ action: "scan" });
        })}><PlugZap size={16} />Liên kết iframe</button></div>}
        {missingOrigins.map(origin => <div className="permission-row" key={origin}><span>{origin}</span><button onClick={() => void run(async () => { const accepted = await chrome.permissions.request({ origins: [`${origin}/*`] }); if (!accepted) throw new Error("Chưa cấp quyền cho frame player."); await request({ action: "scan" }); })}>Kết nối player</button></div>)}
        <div className="controls">
          <fieldset className="capture-mode" disabled={busy || !view.native?.connected || !!view.session && !["stopped", "interrupted"].includes(view.session.status)}><legend>Chế độ thu</legend><div className="capture-mode-options" role="group" aria-label="Chế độ thu"><button aria-pressed={(view.captureMode ?? "all") === "all"} onClick={() => void run(() => request({ action: "capture-mode", mode: "all" }))}>Lưu mọi ảnh</button><button aria-pressed={view.captureMode === "smart"} title="Thông minh đang thử nghiệm; không chắc thì lưu mọi mẫu" disabled={!view.native?.capabilities?.includes("smart-observations-v1")} onClick={() => void run(() => request({ action: "capture-mode", mode: "smart" }))}>Thông minh · thử nghiệm</button></div></fieldset>
          <label className="source-select"><span>Video</span><select disabled={busy || active || !view.connected} value={view.source?.id ?? ""} onChange={event => { const source = view.sources.find(source => source.id === event.target.value); if (source) void run(() => request({ action: "source", source })); }}><option value="">Chọn video</option>{view.sources.map((source, index) => <option key={source.id} value={source.id}>{index + 1}. {source.origin} · {source.clock.width} × {source.clock.height}</option>)}</select></label>
          <button className="icon-button" title="Quét lại video" aria-label="Quét lại video" disabled={busy || !view.connected} onClick={() => void run(() => request({ action: "scan" }))}><RefreshCw size={17} /></button>
          <span className="divider" />
          <button className={`icon-button ${selecting ? "chosen" : ""}`} title="Chọn vùng subtitle" aria-label="Chọn vùng subtitle" disabled={active || !view.source || busy || !view.connected} onClick={() => setSelecting(!selecting)}><Crop size={18} /></button>
          <button className="icon-button" title="Lưu một ảnh mẫu" aria-label="Lưu một ảnh mẫu" disabled={!view.region || busy || active || !view.connected} onClick={() => void run(() => request({ action: "snapshot" }))}><Camera size={18} /></button>
          <button className="primary" disabled={busy || !view.region || !view.connected || (!active && !view.native?.connected)} onClick={() => void run(async () => { await request({ action: active ? "pause" : "start" }); setSelectedId(null); })}>{active ? <Pause size={16} /> : <Play size={16} />}{active ? "Tạm dừng" : "Thu"}</button>
          <button className="icon-button" title="Kết thúc phiên" aria-label="Kết thúc phiên" disabled={busy || !view.connected && (!view.session || !!view.session.native?.finalized)} onClick={() => void run(() => request({ action: "stop" }))}><Square size={16} /></button>
        </div>
        <div className={`capture-health ${health.tone}`} role="status" aria-live="polite"><strong>{health.label}</strong><span>{health.detail}</span></div>
        {(view.connected || !displayed?.native?.finalized) && <div className={`preview ${selecting ? "selecting" : ""}`} style={{ aspectRatio: view.previewSize ? `${view.previewSize.width}/${view.previewSize.height}` : "16/9" }}
          onPointerDown={event => { if (!selecting || !view.connected) return; event.currentTarget.setPointerCapture(event.pointerId); anchor.current = point(event); setDraft(dragRegion(anchor.current, anchor.current)); }}
          onPointerMove={event => { if (anchor.current) setDraft(dragRegion(anchor.current, point(event))); }} onPointerUp={finishRegion} onPointerCancel={() => { anchor.current = null; setDraft(null); }}>
          {view.preview ? <img src={view.preview} alt="Tab phim đang thu" draggable={false} /> : <div className="empty-preview"><ScanLine size={36} /><span>Chưa có hình</span></div>}
          {view.playerGeometry && <div className="player-bound" aria-label="Khung video đã liên kết" style={{left:`${view.playerGeometry.rect.x / view.playerGeometry.viewport.width * 100}%`,top:`${view.playerGeometry.rect.y / view.playerGeometry.viewport.height * 100}%`,width:`${view.playerGeometry.rect.width / view.playerGeometry.viewport.width * 100}%`,height:`${view.playerGeometry.rect.height / view.playerGeometry.viewport.height * 100}%`}} />}
          {region && <div className="region" style={{ left: `${region.x * 100}%`, top: `${region.y * 100}%`, width: `${region.width * 100}%`, height: `${region.height * 100}%` }}><span /><span /><span /><span /></div>}
        </div>}
        <div className="metrics"><div><span>Thời gian video</span><strong>{formatTime(view.connected && view.clock ? view.clock.mediaTime * 1000 : sample?.estimatedVideoTimeMs ?? null)}</strong></div><div><span>Player</span><strong>{view.clock?.seeking ? "Đang tua" : view.clock?.waiting ? "Tải đệm" : view.clock?.paused ? "Pause" : view.clock ? `${view.clock.rate}×` : "--"}</strong></div><div><span>Ảnh đã lưu</span><strong>{displayed?.native?.savedSamples ?? displayed?.sampleCount ?? 0}</strong></div><div><span>Dung lượng ảnh</span><strong>{((displayed?.native?.savedBytes ?? displayed?.imageBytes ?? 0) / 1048576).toFixed(1)} MiB</strong></div></div>
        {displayed?.native && <div className="upload-metrics"><span>Đang chờ: {Math.max(0, displayed.sampleCount - displayed.native.savedSamples)} ảnh · {(displayed.native.pendingBytes / 1048576).toFixed(1)} MiB / 128 MiB</span><strong>{nativeStatus(displayed)}</strong></div>}
        {displayed?.schemaVersion === 2 && <div className="upload-metrics"><span>{displayed.observationCount ?? 0} lần quan sát · {displayed.sampleCount} ảnh · {displayed.fallbackCount ?? 0} mẫu fallback</span><strong>{view.detectorState === "stable" ? "Chữ ổn định" : view.detectorState === "fallback" ? "Không chắc · Lưu mọi mẫu" : "Đang quan sát"}</strong></div>}
        {view.native?.connected && !view.native.capabilities?.includes("smart-observations-v1") && <p className="error-banner">Helper hiện tại chưa hỗ trợ Thông minh. Cập nhật helper để dùng; Lưu mọi ảnh vẫn hoạt động.</p>}
        <section className="sample-section"><div className="sample-heading"><h2>Ảnh mẫu</h2>{selected?.native ? <button disabled={busy || !selected.native.sessionPath} onClick={() => void run(() => request({ action: "open-folder", sessionId: selected.id }))}><FolderOpen size={16} />Mở thư mục</button> : <button disabled={busy || !selected?.sampleCount || (active && selected.id === view.session?.id)} onClick={() => void run(() => exportSession(selected!))}><Download size={16} />Xuất ZIP</button>}</div>
          {imageUrl && sample ? <><div className="sample-image"><img src={imageUrl} alt="Ảnh vùng subtitle đã lưu" /></div><div className="sample-details"><span><Check size={14} />#{sample.sequence} · {formatTime(sample.estimatedVideoTimeMs)}</span><span>RTT {Math.round(sample.roundTripMs)} ms · segment {sample.segment}</span></div></> : <p className="empty-sample">Chưa có ảnh mẫu</p>}
        </section>
        {displayed?.native?.finalized && <section className="ocr-section">
          <div className="sample-heading"><h2>OCR / SRT</h2><div className="ocr-actions">
            <button disabled={busy || ocrRunning || ocr?.committed} onClick={() => void run(async () => { const result = await request<OcrStatus>({ action: "ocr-start", sessionId: displayed.id }); setOcr(result); })}><ScanText size={16} />{["error", "interrupted", "cancelled"].includes(ocr?.state ?? "") ? "Tiếp tục OCR" : "Chạy OCR"}</button>
            <button className="icon-button" title="Dừng OCR" aria-label="Dừng OCR" disabled={busy || !ocrRunning} onClick={() => void run(() => request({ action: "ocr-cancel", sessionId: displayed.id }))}><Square size={16} /></button>
          </div></div>
          {(ocrError || ocr?.error) && <div role="alert" className="error-banner">{ocrError ?? ocr?.error}</div>}
          <div className="upload-metrics"><strong>{ocr ? ocrLabels[ocr.state] : "Đang đọc trạng thái"}</strong><span>{ocr?.committed ? `${ocr.cueCount ?? 0} cue · ${ocr.reviewCount ?? 0} nghi vấn · Chưa kiểm tra toàn tập` : `${ocr?.processedSamples ?? 0} / ${ocr?.totalSamples ?? displayed.sampleCount} ảnh`}</span></div>
          {ocrRunning && <progress aria-label="Tiến độ OCR" max={ocr?.totalSamples || 1} value={ocr?.processedSamples ?? 0} />}
          {ocr?.preview && <pre className="srt-preview">{ocr.preview}</pre>}
        </section>}
      </main>
    </div>
  </div>;
}
createRoot(document.querySelector("#root")!).render(<Recorder />);
