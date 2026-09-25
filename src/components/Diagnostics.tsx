import { useEffect, useMemo, useState } from "react";
import { Check, ClipboardCopy, Download, RefreshCw, Trash2 } from "lucide-react";
import { save } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/tauri";
import type { DiagnosticEntry, ModelInfo, SystemStatus } from "../types";

const formatTimestamp = (timestamp: number) => new Intl.DateTimeFormat("vi-VN", {
  dateStyle: "short",
  timeStyle: "medium",
}).format(new Date(timestamp));

function buildReport(entries: DiagnosticEntry[], system: SystemStatus | null, models: ModelInfo[]) {
  const status = system ? [
    `FFmpeg=${system.ffmpeg}`,
    `FFprobe=${system.ffprobe}`,
    `Whisper=${system.whisperCli}`,
    `Llama=${system.llamaCli}`,
    `VieNeu=${system.vieneuCli}`,
  ].join(", ") : "Không đọc được";
  const modelLines = models.map((model) =>
    `- ${model.id}: ${model.state}, ${model.installedBytes}/${model.sizeBytes} bytes`,
  );
  const logLines = entries.map((item) => [
    `[${new Date(item.timestampMs).toISOString()}] ${item.level.toUpperCase()} ${item.source}: ${item.message}`,
    item.details ? `  ${item.details.replaceAll("\n", "\n  ")}` : "",
  ].filter(Boolean).join("\n"));
  return [
    "LỒNG TIẾNG - DIAGNOSTIC REPORT",
    `Generated: ${new Date().toISOString()}`,
    `Platform: ${navigator.platform}`,
    `User agent: ${navigator.userAgent}`,
    `Engines: ${status}`,
    "Models:",
    ...(modelLines.length ? modelLines : ["- Không đọc được"]),
    "",
    "Logs:",
    ...(logLines.length ? logLines : ["Không có nhật ký."]),
  ].join("\n");
}

export function Diagnostics() {
  const [entries, setEntries] = useState<DiagnosticEntry[]>([]);
  const [system, setSystem] = useState<SystemStatus | null>(null);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [copied, setCopied] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = async () => {
    setError(null);
    try {
      const [nextEntries, nextSystem, nextModels] = await Promise.all([
        api.diagnostics(), api.systemStatus(), api.models(),
      ]);
      setEntries(nextEntries.reverse());
      setSystem(nextSystem);
      setModels(nextModels);
    } catch (reason) {
      setError(String(reason));
    }
  };

  useEffect(() => {
    void refresh();
    let stop: (() => void) | undefined;
    void api.onDiagnostic((entry) => {
      setEntries((current) => [entry, ...current].slice(0, 300));
    }).then((unlisten) => (stop = unlisten));
    return () => stop?.();
  }, []);

  const report = useMemo(() => buildReport([...entries].reverse(), system, models), [entries, system, models]);

  const copyReport = async () => {
    try {
      await navigator.clipboard.writeText(report);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1800);
    } catch (reason) {
      setError(`Không sao chép được: ${String(reason)}`);
    }
  };

  const exportReport = async () => {
    const path = await save({
      defaultPath: `long-tieng-diagnostics-${new Date().toISOString().slice(0, 10)}.txt`,
      filters: [{ name: "Text", extensions: ["txt"] }],
    });
    if (path) await api.writeTextFile(path, report);
  };

  const clear = async () => {
    await api.clearDiagnostics();
    setEntries([]);
  };

  return (
    <section className="diagnostics-page">
      <div className="section-heading diagnostics-heading">
        <div>
          <p className="eyebrow">CHẨN ĐOÁN</p>
          <h2>{entries.length} sự kiện gần đây</h2>
        </div>
        <div className="diagnostic-actions">
          <button className="icon-button" onClick={() => void refresh()} title="Làm mới"><RefreshCw size={17} /></button>
          <button className="button secondary" onClick={() => void copyReport()}>
            {copied ? <Check size={16} /> : <ClipboardCopy size={16} />}{copied ? "Đã sao chép" : "Sao chép báo cáo"}
          </button>
          <button className="button secondary" onClick={() => void exportReport()}><Download size={16} />Xuất TXT</button>
          <button className="icon-button danger" onClick={() => void clear()} title="Xóa nhật ký"><Trash2 size={17} /></button>
        </div>
      </div>

      <div className="diagnostic-summary">
        <span>Qwen: {models.find((model) => model.id === "qwen3-14b-q4")?.state ?? "unknown"}</span>
        <span>Llama: {system?.llamaCli ? "ready" : "missing"}</span>
        <span>App data: {system?.appDataDir ?? "unknown"}</span>
      </div>

      <div className="diagnostic-list">
        {entries.map((item, index) => (
          <article className={`diagnostic-entry ${item.level}`} key={`${item.timestampMs}-${index}`}>
            <div className="diagnostic-meta">
              <span>{formatTimestamp(item.timestampMs)}</span>
              <strong>{item.level.toUpperCase()}</strong>
              <code>{item.source}</code>
            </div>
            <p>{item.message}</p>
            {item.details && <pre>{item.details}</pre>}
          </article>
        ))}
        {!entries.length && <div className="diagnostic-empty">Không có lỗi hoặc sự kiện chẩn đoán.</div>}
      </div>
      {error && <div className="inline-error">{error}</div>}
    </section>
  );
}
