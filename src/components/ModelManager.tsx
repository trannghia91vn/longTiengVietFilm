import { useEffect, useMemo, useState } from "react";
import { Check, Download, HardDrive, LoaderCircle, RefreshCw, Trash2 } from "lucide-react";
import { api } from "../lib/tauri";
import type { DownloadProgress, ModelInfo } from "../types";

const formatBytes = (bytes: number) => {
  if (!bytes) return "0 MB";
  const units = ["B", "KB", "MB", "GB"];
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  return `${(bytes / 1024 ** index).toFixed(index > 1 ? 1 : 0)} ${units[index]}`;
};

interface Props {
  compact?: boolean;
  onChanged?: () => void;
}

export function ModelManager({ compact = false, onChanged }: Props) {
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [progress, setProgress] = useState<Record<string, DownloadProgress>>({});
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = async () => {
    try {
      setModels(await api.models());
      onChanged?.();
    } catch (reason) {
      setError(String(reason));
    }
  };

  useEffect(() => {
    void refresh();
    let stop: (() => void) | undefined;
    void api.onDownloadProgress((event) => {
      setProgress((current) => ({ ...current, [event.modelId]: event }));
    }).then((unlisten) => (stop = unlisten));
    return () => stop?.();
  }, []);

  const installed = useMemo(() => models.filter((item) => item.state === "installed").length, [models]);

  const download = async (id: string) => {
    setBusy(id);
    setError(null);
    try {
      await api.downloadModel(id);
      await refresh();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(null);
    }
  };

  const remove = async (id: string) => {
    setBusy(id);
    setError(null);
    try {
      await api.removeModel(id);
      await refresh();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(null);
    }
  };

  return (
    <section className={compact ? "models compact" : "models"}>
      <div className="section-heading">
        <div>
          <p className="eyebrow">MODEL LOCAL</p>
          <h2>{compact ? `${installed}/${models.length} model sẵn sàng` : "Quản lý model"}</h2>
        </div>
        <button className="icon-button" onClick={() => void refresh()} title="Kiểm tra lại model">
          <RefreshCw size={17} />
        </button>
      </div>

      <div className="model-list">
        {models.map((model) => {
          const current = progress[model.id];
          const isBusy = busy === model.id;
          const partialPercent = model.sizeBytes > 0 ? model.installedBytes / model.sizeBytes * 100 : 0;
          const phaseLabel = current && ({
            connecting: "Đang kết nối",
            resuming: "Chuẩn bị tải tiếp",
            downloading: `${current.percent.toFixed(1)}%`,
            verifying: current.phasePercent == null ? "Đang kiểm tra checksum" : `Checksum ${current.phasePercent.toFixed(1)}%`,
            installing: "Đang cài đặt",
            complete: "Hoàn tất",
          }[current.phase]);
          return (
            <article className="model-row" key={model.id}>
              <div className={`model-mark ${model.provider}`}>
                {model.state === "installed" ? <Check size={18} /> : <HardDrive size={18} />}
              </div>
              <div className="model-copy">
                <div className="model-title-line">
                  <strong>{model.name}</strong>
                  <span>{formatBytes(model.sizeBytes)}</span>
                </div>
                {!compact && <p>{model.description}</p>}
                <div className="model-meta">
                  <span>{model.provider.toUpperCase()}</span>
                  <span>{model.license}</span>
                  <span className={model.state === "installed" ? "status-ok" : "status-muted"}>
                    {model.state === "installed" ? "Đã cài" : model.state === "partial" ? `Tải dở ${partialPercent.toFixed(1)}%` : "Chưa tải"}
                  </span>
                </div>
                {isBusy && current && (
                  <div className="download-track" aria-label={current.phase === "verifying" ? `Đang kiểm tra checksum ${current.phasePercent?.toFixed(0) ?? 0}%` : `Đang tải ${current.percent.toFixed(0)}%`}>
                    <span style={{ width: `${current.phase === "verifying" ? current.phasePercent ?? 0 : current.percent}%` }} />
                  </div>
                )}
                {isBusy && <div className="download-status">{phaseLabel ?? "Đang chuẩn bị"}</div>}
              </div>
              {model.state === "installed" ? (
                <button className="icon-button danger" onClick={() => void remove(model.id)} disabled={isBusy} title="Xóa model">
                  {isBusy ? <LoaderCircle className="spin" size={17} /> : <Trash2 size={17} />}
                </button>
              ) : (
                <button className="button secondary" onClick={() => void download(model.id)} disabled={isBusy}>
                  {isBusy ? <LoaderCircle className="spin" size={16} /> : <Download size={16} />}
                  {isBusy ? (current?.phase === "verifying" ? `${current.phasePercent?.toFixed(0) ?? 0}%` : `${current?.percent.toFixed(1) ?? partialPercent.toFixed(1)}%`) : model.state === "partial" ? "Tải tiếp" : "Tải"}
                </button>
              )}
            </article>
          );
        })}
      </div>
      {error && <div className="inline-error">{error}</div>}
    </section>
  );
}
