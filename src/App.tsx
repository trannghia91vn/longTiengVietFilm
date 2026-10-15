import { useEffect, useState } from "react";
import { AudioLines, BrainCircuit, FileWarning, FolderKanban, HardDriveDownload, Info, ScanText, Settings2, ShieldCheck, Trash2 } from "lucide-react";
import { OcrGuide } from "./components/OcrGuide";
import { OcrWorkspace } from "./components/OcrWorkspace";
import { Diagnostics } from "./components/Diagnostics";
import { ModelManager } from "./components/ModelManager";
import { ProjectEditor } from "./components/ProjectEditor";
import { api } from "./lib/tauri";
import type { AppView, SystemStatus, TranslationMemoryStatus } from "./types";

export default function App() {
  const [view, setView] = useState<AppView>("project");
  const [system, setSystem] = useState<SystemStatus | null>(null);
  const [modelRevision, setModelRevision] = useState(0);
  const [memory, setMemory] = useState<TranslationMemoryStatus | null>(null);
  const [showOcrGuide, setShowOcrGuide] = useState(false);

  useEffect(() => {
    void api.systemStatus().then(setSystem).catch(() => setSystem(null));
    void api.translationMemoryStatus().then(setMemory).catch(() => setMemory(null));
  }, [modelRevision]);

  const nav = [
    { id: "project" as const, label: "Dự án", icon: FolderKanban },
    { id: "ocr" as const, label: "OCR", icon: ScanText },
    { id: "models" as const, label: "Model", icon: HardDriveDownload },
    { id: "diagnostics" as const, label: "Nhật ký", icon: FileWarning },
    { id: "settings" as const, label: "Cài đặt", icon: Settings2 },
  ];

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand"><span><AudioLines size={20} /></span><strong>Lồng tiếng</strong></div>
        <nav>
          {nav.map((item) => (
            <button key={item.id} className={view === item.id ? "active" : ""} onClick={() => setView(item.id)} title={item.label}>
              <item.icon size={19} /><span>{item.label}</span>
            </button>
          ))}
        </nav>
        <div className="privacy"><ShieldCheck size={17} /><span>Offline trên máy</span></div>
      </aside>

      <main>
        <header className="topbar">
          <div>
            <p className="eyebrow">STUDIO LOCAL</p>
            <h1>{view === "project" ? "Dự án mới" : view === "ocr" ? "OCR" : view === "models" ? "Model Manager" : view === "diagnostics" ? "Nhật ký" : "Cài đặt"}</h1>
          </div>
          <div className="topbar-tools">
            <div className="engine-pills">
              <span className={system?.ffmpeg ? "ready" : "missing"}>FFmpeg</span>
              <span className={system?.whisperCli ? "ready" : "missing"}>Whisper</span>
              <span className={system?.llamaServer ? "ready" : "missing"}>AI dịch</span>
            </div>
            <button type="button" className="icon-button ocr-guide-trigger" title="Hướng dẫn OCR" aria-label="Hướng dẫn OCR" aria-haspopup="dialog" aria-controls="ocr-guide" aria-expanded={showOcrGuide} onClick={event => { event.currentTarget.focus(); setShowOcrGuide(true); }}><Info size={17} /></button>
          </div>
        </header>

        <div className="content-area">
          <div hidden={view !== "ocr"}><OcrWorkspace active={view === "ocr"} /></div>
          {view === "project" && <ProjectEditor modelRevision={modelRevision} onOpenModels={() => setView("models")} onOpenDiagnostics={() => setView("diagnostics")} />}
          {view === "models" && <ModelManager onChanged={() => setModelRevision((value) => value + 1)} />}
          {view === "diagnostics" && <Diagnostics />}
          {view === "settings" && (
            <section className="settings-page">
              <div className="section-heading"><div><p className="eyebrow">HỆ THỐNG</p><h2>Engine cục bộ</h2></div></div>
              <div className="settings-list">
                {system && Object.entries({ FFmpeg: system.ffmpeg, FFprobe: system.ffprobe, "Whisper CLI": system.whisperCli, "Llama CLI": system.llamaCli, "Llama Server": system.llamaServer, "audio.cpp": system.audioCpp }).map(([name, ready]) => (
                  <div key={name}><span>{name}</span><strong className={ready ? "status-ok" : "status-muted"}>{ready ? "Sẵn sàng" : "Chưa tìm thấy"}</strong></div>
                ))}
                <div><span>Thư mục model</span><code>{system?.appDataDir ?? "Đang kiểm tra..."}</code></div>
              </div>
              <div className="settings-section-heading"><BrainCircuit size={17} /><div><strong>Bộ nhớ văn phong</strong><small>{memory?.count ?? 0} câu sửa được lưu local</small></div></div>
              <div className="settings-list">
                <div><span>Học từ câu bạn sửa</span><label className="settings-toggle"><input type="checkbox" checked={memory?.enabled ?? true} onChange={(event) => void api.setTranslationMemoryEnabled(event.target.checked).then(setMemory)} /> Bật</label></div>
                <div><span>Xóa toàn bộ mẫu đã học</span><button className="button secondary" disabled={!memory?.count} onClick={() => void api.clearTranslationMemory().then(setMemory)}><Trash2 size={14} /> Xóa bộ nhớ</button></div>
              </div>
              <ModelManager compact onChanged={() => setModelRevision((value) => value + 1)} />
            </section>
          )}
        </div>
      </main>
      {showOcrGuide && <OcrGuide onClose={() => setShowOcrGuide(false)} />}
    </div>
  );
}
