import { useEffect, useMemo, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  AlignVerticalJustifyCenter,
  AudioLines,
  Check,
  ChevronRight,
  Download,
  FileText,
  Film,
  Languages,
  LoaderCircle,
  Play,
  RefreshCw,
  Upload,
  WandSparkles,
} from "lucide-react";
import { api } from "../lib/tauri";
import type {
  DubProgress,
  DubResult,
  MediaInfo,
  ModelInfo,
  SubtitleDocument,
  SyncResult,
  TranslationProgress,
  TranslationQuality,
  TranslationResult,
} from "../types";

type SourceLanguage = "auto" | "en" | "ja" | "ko" | "zh";
type WorkflowMode = "simple" | "advanced";
type SimpleSubtitleLanguage = "vi" | "en";

const sourceLanguages: Array<{ value: SourceLanguage; label: string }> = [
  { value: "auto", label: "Tự nhận diện" },
  { value: "ja", label: "Tiếng Nhật" },
  { value: "en", label: "Tiếng Anh" },
  { value: "ko", label: "Tiếng Hàn" },
  { value: "zh", label: "Tiếng Trung" },
];

const formatTime = (ms: number) => {
  const total = Math.floor(ms / 1000);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = total % 60;
  return [hours, minutes, seconds].map((value) => String(value).padStart(2, "0")).join(":");
};

const detectSubtitleLanguage = (document: SubtitleDocument): SimpleSubtitleLanguage => {
  const sample = document.cues.slice(0, 80).map((cue) => cue.text).join(" ");
  return /[ăâđêôơưáàảãạấầẩẫậắằẳẵặéèẻẽẹếềểễệíìỉĩịóòỏõọốồổỗộớờởỡợúùủũụứừửữựýỳỷỹỵ]/i.test(sample)
    ? "vi"
    : "en";
};

interface Props {
  modelRevision: number;
  onOpenModels: () => void;
}

export function Workflow({ modelRevision, onOpenModels }: Props) {
  const [workflowMode, setWorkflowMode] = useState<WorkflowMode>("simple");
  const [media, setMedia] = useState<MediaInfo | null>(null);
  const [source, setSource] = useState<SubtitleDocument | null>(null);
  const [translated, setTranslated] = useState<SubtitleDocument | null>(null);
  const [externalVietnamese, setExternalVietnamese] = useState<SubtitleDocument | null>(null);
  const [sync, setSync] = useState<SyncResult | null>(null);
  const [simpleSubtitle, setSimpleSubtitle] = useState<SubtitleDocument | null>(null);
  const [simpleSubtitleLanguage, setSimpleSubtitleLanguage] = useState<SimpleSubtitleLanguage>("vi");
  const [simpleTranslation, setSimpleTranslation] = useState<SubtitleDocument | null>(null);
  const [simpleTranslationResult, setSimpleTranslationResult] = useState<TranslationResult | null>(null);
  const [advancedTranslationResult, setAdvancedTranslationResult] = useState<TranslationResult | null>(null);
  const [translationProgress, setTranslationProgress] = useState<TranslationProgress | null>(null);
  const [translationQuality, setTranslationQuality] = useState<TranslationQuality>(() =>
    window.localStorage.getItem("translation-quality") === "fast" ? "fast" : "natural"
  );
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [dubResult, setDubResult] = useState<DubResult | null>(null);
  const [finalDubResult, setFinalDubResult] = useState<DubResult | null>(null);
  const [dubProgress, setDubProgress] = useState<DubProgress | null>(null);
  const [dubBusy, setDubBusy] = useState<"test" | "export" | null>(null);
  const [previewPath, setPreviewPath] = useState<string | null>(null);
  const [previewStatus, setPreviewStatus] = useState<"idle" | "preparing" | "ready" | "error">("idle");
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [previewPercent, setPreviewPercent] = useState(0);
  const [currentTimeMs, setCurrentTimeMs] = useState(0);
  const videoRef = useRef<HTMLVideoElement>(null);
  const previewRequest = useRef(0);
  const [sourceLanguage, setSourceLanguage] = useState<SourceLanguage>(() => {
    const saved = window.localStorage.getItem("source-language");
    return sourceLanguages.some((item) => item.value === saved) ? saved as SourceLanguage : "auto";
  });
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void api.models().then(setModels).catch(() => setModels([]));
  }, [modelRevision]);

  const simpleActiveSubtitle = simpleTranslation ?? simpleSubtitle;
  const activeDocument = workflowMode === "simple"
    ? simpleActiveSubtitle
    : sync?.output ?? externalVietnamese ?? translated ?? source;
  const activeLabel = workflowMode === "simple"
    ? simpleTranslation
      ? "SRT Việt đã dịch"
      : simpleSubtitle
        ? simpleSubtitleLanguage === "vi" ? "SRT Việt để lồng tiếng" : "SRT English"
        : "PHỤ ĐỀ"
    : sync
      ? "SRT Việt đã căn"
      : externalVietnamese
        ? "SRT Việt đầy đủ"
        : translated
          ? "Bản dịch Việt 5 phút"
          : source
            ? "Phụ đề nguồn 5 phút"
            : "PHỤ ĐỀ";
  const simpleSubtitleReady = Boolean(simpleActiveSubtitle && (simpleSubtitleLanguage === "vi" || simpleTranslation));
  const videoUrl = useMemo(() => (previewPath ? convertFileSrc(previewPath) : ""), [previewPath]);
  const currentCue = useMemo(() => {
    const cues = activeDocument?.cues;
    if (!cues?.length) return null;
    let low = 0;
    let high = cues.length - 1;
    let match = -1;
    while (low <= high) {
      const middle = Math.floor((low + high) / 2);
      if (cues[middle].startMs <= currentTimeMs) {
        match = middle;
        low = middle + 1;
      } else {
        high = middle - 1;
      }
    }
    return match >= 0 && currentTimeMs <= cues[match].endMs ? cues[match] : null;
  }, [activeDocument, currentTimeMs]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void api.onPreviewProgress((progress) => setPreviewPercent(progress.percent)).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void api.onTranslationProgress(setTranslationProgress).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void api.onDubProgress(setDubProgress).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const run = async <T,>(label: string, action: () => Promise<T>, done: (value: T) => void) => {
    setBusy(label);
    setError(null);
    try {
      done(await action());
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(null);
    }
  };

  const preparePreview = async (nextMedia: MediaInfo, forceProxy = false) => {
    const requestId = ++previewRequest.current;
    setPreviewPath(null);
    setPreviewStatus("preparing");
    setPreviewPercent(0);
    setPreviewError(null);
    try {
      const path = await api.prepareVideoPreview(nextMedia.path, forceProxy);
      if (requestId !== previewRequest.current) return;
      setPreviewPath(path);
      setPreviewStatus("ready");
    } catch (reason) {
      if (requestId !== previewRequest.current) return;
      setPreviewStatus("error");
      setPreviewError(String(reason));
    }
  };

  const chooseVideo = async () => {
    const selected = await open({
      multiple: false,
      filters: [{ name: "Video", extensions: ["mp4", "mkv", "mov", "avi", "webm", "m4v"] }],
    });
    if (selected) await run("video", () => api.inspectMedia(selected), (nextMedia) => {
      setMedia(nextMedia);
      setSource(null);
      setTranslated(null);
      setExternalVietnamese(null);
      setSync(null);
      setSimpleSubtitle(null);
      setSimpleTranslation(null);
      setSimpleTranslationResult(null);
      setAdvancedTranslationResult(null);
      setDubResult(null);
      setFinalDubResult(null);
      setDubProgress(null);
      setCurrentTimeMs(0);
      void preparePreview(nextMedia);
    });
  };

  const handleVideoError = () => {
    if (!media || previewStatus !== "ready") return;
    if (previewPath === media.path) {
      void preparePreview(media, true);
      return;
    }
    setPreviewStatus("error");
    setPreviewPath(null);
    setPreviewError("WebView không mở được bản preview. Hãy thử tạo lại hoặc xem Nhật ký.");
  };

  const seekToCue = (startMs: number) => {
    const video = videoRef.current;
    if (!video || previewStatus !== "ready") return;
    video.currentTime = Math.max(0, startMs / 1000);
    setCurrentTimeMs(startMs);
    void video.play().catch(() => undefined);
  };

  const reviewCue = (ratio: number) => {
    const cues = sync?.output.cues;
    if (!cues?.length) return;
    seekToCue(cues[Math.round((cues.length - 1) * ratio)].startMs);
  };

  const chooseSubtitle = async (kind: "source" | "full-vi") => {
    const selected = await open({ multiple: false, filters: [{ name: "SubRip", extensions: ["srt"] }] });
    if (!selected) return;
    const language = kind === "source" ? sourceLanguage : "vi";
    await run(`srt-${kind}`, () => api.readSrt(selected, language), (document) => {
      if (kind === "source") {
        setSource(document);
        setTranslated(null);
        setAdvancedTranslationResult(null);
        setExternalVietnamese(null);
      } else {
        setExternalVietnamese(document);
      }
      setSync(null);
    });
  };

  const changeSourceLanguage = (language: SourceLanguage) => {
    setSourceLanguage(language);
    window.localStorage.setItem("source-language", language);
  };

  const chooseSimpleSubtitle = async () => {
    const selected = await open({ multiple: false, filters: [{ name: "SubRip", extensions: ["srt"] }] });
    if (!selected) return;
    await run("simple-srt", () => api.readSrt(selected, "auto"), (document) => {
      const language = detectSubtitleLanguage(document);
      setSimpleSubtitle({ ...document, language });
      setSimpleSubtitleLanguage(language);
      setSimpleTranslation(null);
      setSimpleTranslationResult(null);
      setDubResult(null);
      setFinalDubResult(null);
    });
  };

  const changeSimpleSubtitleLanguage = (language: SimpleSubtitleLanguage) => {
    if (language === simpleSubtitleLanguage) return;
    setSimpleSubtitleLanguage(language);
    setSimpleTranslation(null);
    setSimpleTranslationResult(null);
    setDubResult(null);
    setFinalDubResult(null);
    if (simpleSubtitle) setSimpleSubtitle({ ...simpleSubtitle, language });
  };

  const runDub = async (kind: "test" | "export") => {
    if (!media || !simpleActiveSubtitle || !simpleSubtitleReady) return;
    let target: string | null = null;
    if (kind === "export") {
      target = await save({
        defaultPath: "phim-long-tieng.mkv",
        filters: [{ name: "Matroska video", extensions: ["mkv"] }],
      });
      if (!target) return;
    }
    setDubBusy(kind);
    setDubProgress(null);
    setError(null);
    try {
      const result = await api.renderDubbedVideo(
        media.path,
        simpleActiveSubtitle.path,
        target,
        kind === "test" ? 300 : null,
      );
      if (kind === "test") {
        setDubResult(result);
        setPreviewPath(result.path);
        setPreviewStatus("ready");
        setPreviewError(null);
        setCurrentTimeMs(0);
      } else {
        setFinalDubResult(result);
      }
    } catch (reason) {
      setError(String(reason));
    } finally {
      setDubBusy(null);
    }
  };

  const exportSubtitle = async () => {
    if (!activeDocument) return;
    const target = await save({ defaultPath: "phu-de-viet-da-dong-bo.srt", filters: [{ name: "SubRip", extensions: ["srt"] }] });
    if (target) await run("export", () => api.writeSrt(target, activeDocument.cues), () => undefined);
  };

  const changeTranslationQuality = (quality: TranslationQuality) => {
    setTranslationQuality(quality);
    window.localStorage.setItem("translation-quality", quality);
  };

  const installedModelIds = new Set(models.filter((model) => model.state === "installed").map((model) => model.id));
  const requiredTranslationModels = translationQuality === "natural"
    ? ["hy-mt2-7b-q4", "qwen3-14b-q4"]
    : ["hy-mt2-7b-q4"];
  const missingTranslationModels = requiredTranslationModels.filter((id) => !installedModelIds.has(id));

  const translationOptions = (result: TranslationResult | null, busyLabel: string) => (
    <>
      <div className="translation-mode">
        <span>Chất lượng dịch</span>
        <div className="language-segment" role="group" aria-label="Chất lượng dịch">
          <button className={translationQuality === "natural" ? "active" : ""} disabled={busy !== null} onClick={() => changeTranslationQuality("natural")}>Tự nhiên</button>
          <button className={translationQuality === "fast" ? "active" : ""} disabled={busy !== null} onClick={() => changeTranslationQuality("fast")}>Nhanh</button>
        </div>
        <small>{translationQuality === "natural" ? "Hy-MT2 dịch nghĩa, Qwen biên tập lời thoại." : "Hy-MT2 dịch trực tiếp, bỏ qua lượt biên tập."}</small>
      </div>
      {missingTranslationModels.length > 0 && (
        <div className="translation-model-missing">
          <span>Thiếu {missingTranslationModels.includes("hy-mt2-7b-q4") ? "Hy-MT2" : "Qwen3"}{missingTranslationModels.length > 1 ? " và Qwen3" : ""}.</span>
          <button className="button secondary" onClick={onOpenModels}><Download size={14} />Mở Model Manager</button>
        </div>
      )}
      {busy === busyLabel && translationProgress && (
        <div className="dub-progress translation-progress" aria-live="polite">
          <div><span>{translationProgress.message}</span><strong>{translationProgress.percent.toFixed(0)}%</strong></div>
          <div className="download-track"><span style={{ width: `${translationProgress.percent}%` }} /></div>
        </div>
      )}
      {result && (
        <div className={`translation-result ${result.warnings.length ? "has-warning" : ""}`}>
          <div className="translation-summary">
            <span><Check size={13} />{result.quality === "natural" ? `${result.editedCues} câu đã biên tập` : "Đã dịch nhanh bằng Hy-MT2"}</span>
            {result.fallbackCues > 0 && <span>{result.fallbackCues} câu dùng bản dịch thô</span>}
            {result.warnings.length > 0 && <span>{result.warnings.length} cảnh báo cần kiểm tra</span>}
          </div>
          {result.warnings.length > 0 && (
            <div className="translation-warning-list">
              {result.warnings.slice(0, 3).map((warning, index) => <small key={`${warning.code}-${warning.cueIndex}-${index}`}>{warning.message}</small>)}
              {result.warnings.length > 3 && <small>Và {result.warnings.length - 3} cảnh báo khác trong Nhật ký.</small>}
            </div>
          )}
        </div>
      )}
    </>
  );

  if (!media) {
    return (
      <div className="empty-workspace">
        <button className="drop-zone" onClick={() => void chooseVideo()}>
          <span className="drop-icon"><Upload size={28} /></span>
          <strong>Chọn phim để bắt đầu</strong>
          <span>MP4, MKV, MOV hoặc WebM</span>
        </button>
        <div className="start-note">
          <Check size={16} /> Video và âm thanh được xử lý hoàn toàn trên máy.
        </div>
        {error && <div className="inline-error">{error}</div>}
      </div>
    );
  }

  return (
    <div className="workspace-grid">
      <section className="preview-pane">
        <div className="video-frame">
          {previewPath ? (
            <video
              key={previewPath}
              ref={videoRef}
              controls
              src={videoUrl}
              onError={handleVideoError}
              onTimeUpdate={(event) => setCurrentTimeMs(event.currentTarget.currentTime * 1000)}
              onSeeked={(event) => setCurrentTimeMs(event.currentTarget.currentTime * 1000)}
            />
          ) : (
            <div className="video-preview-state" aria-live="polite">
              {previewStatus === "preparing" ? <LoaderCircle className="spin" size={24} /> : <Film size={24} />}
              <strong>{previewStatus === "preparing" ? `Đang tạo video preview ${previewPercent.toFixed(0)}%` : "Chưa mở được video"}</strong>
              <span>{previewStatus === "preparing" ? "MKV và codec lạ sẽ được đổi sang MP4 tương thích." : previewError}</span>
              {previewStatus === "error" && (
                <button className="button secondary" onClick={() => void preparePreview(media, true)}><RefreshCw size={15} />Tạo lại preview</button>
              )}
            </div>
          )}
          {currentCue && previewStatus === "ready" && (
            <div className="video-subtitle"><span>{currentCue.text}</span></div>
          )}
        </div>
        <div className="media-summary">
          <div className="file-badge"><Film size={18} /></div>
          <div>
            <strong>{media.fileName}</strong>
            <span>{formatTime(media.durationMs)} · {media.width ?? "?"}×{media.height ?? "?"} · {media.audioCodec ?? "audio"}</span>
          </div>
          <button className="button ghost" onClick={() => void chooseVideo()}>Đổi phim</button>
        </div>

        <div className="subtitle-preview">
          <div className="section-heading">
            <div>
              <p className="eyebrow">{activeLabel}</p>
              <h2>{activeDocument ? `${activeDocument.cues.length} câu thoại` : "Chưa có dữ liệu"}</h2>
            </div>
            {activeDocument && <button className="button secondary" onClick={() => void exportSubtitle()}><Download size={16} />Xuất SRT</button>}
          </div>
          <div className="cue-list">
            {activeDocument?.cues.map((cue) => (
              <button className={`cue-row ${currentCue === cue ? "active" : ""}`} key={`${cue.index}-${cue.startMs}`} onClick={() => seekToCue(cue.startMs)} disabled={previewStatus !== "ready"}>
                <span>{formatTime(cue.startMs)}</span>
                <p>{cue.text}</p>
              </button>
            )) ?? <div className="cue-empty">Phụ đề sẽ xuất hiện ở đây sau khi phân tích hoặc nhập file.</div>}
          </div>
        </div>
      </section>

      <aside className="pipeline-pane">
        <div className="workflow-tabs" role="tablist" aria-label="Chọn quy trình">
          <button
            role="tab"
            aria-selected={workflowMode === "simple"}
            className={workflowMode === "simple" ? "active" : ""}
            onClick={() => setWorkflowMode("simple")}
          >
            Đơn giản
          </button>
          <button
            role="tab"
            aria-selected={workflowMode === "advanced"}
            className={workflowMode === "advanced" ? "active" : ""}
            onClick={() => setWorkflowMode("advanced")}
          >
            Phức tạp
          </button>
        </div>
        <div className="section-heading">
          <div>
            <p className="eyebrow">QUY TRÌNH</p>
            <h2>{workflowMode === "simple" ? "Lồng tiếng từ SRT chuẩn" : "Tạo và căn bản tiếng Việt"}</h2>
          </div>
        </div>

        {workflowMode === "simple" ? (
          <>
            <div className="step-card done">
              <div className="step-index"><Check size={16} /></div>
              <div className="step-body">
                <strong>Import video</strong>
                <p>{media.fileName}</p>
                <button className="button secondary" disabled={busy !== null || dubBusy !== null} onClick={() => void chooseVideo()}><Film size={16} />Đổi video</button>
              </div>
            </div>

            <ChevronRight className="step-arrow" size={18} />

            <div className={`step-card ${simpleSubtitleReady ? "done" : "active"}`}>
              <div className="step-index">{simpleSubtitleReady ? <Check size={16} /> : "2"}</div>
              <div className="step-body">
                <strong>Import phụ đề chuẩn</strong>
                <p>SRT phải đúng nội dung và timestamp của video này.</p>
                <button className="button secondary" disabled={busy !== null || dubBusy !== null} onClick={() => void chooseSimpleSubtitle()}>
                  {busy === "simple-srt" ? <LoaderCircle className="spin" size={16} /> : <FileText size={16} />} Nhập SRT
                </button>
                {simpleSubtitle && (
                  <>
                    <div className="language-segment" role="group" aria-label="Ngôn ngữ phụ đề">
                      <button className={simpleSubtitleLanguage === "vi" ? "active" : ""} onClick={() => changeSimpleSubtitleLanguage("vi")}>Tiếng Việt</button>
                      <button className={simpleSubtitleLanguage === "en" ? "active" : ""} onClick={() => changeSimpleSubtitleLanguage("en")}>English</button>
                    </div>
                    <span className="subtitle-file-status"><Check size={13} />Đã nhập {simpleSubtitle.cues.length} câu</span>
                    {simpleSubtitleLanguage === "en" && (
                      <>
                        {translationOptions(simpleTranslationResult, "simple-translate")}
                        <button className="button primary translate-simple" disabled={busy !== null || dubBusy !== null || missingTranslationModels.length > 0} onClick={() => void run("simple-translate", () => api.translateSrt(simpleSubtitle.path, "en", translationQuality), (result) => {
                          setSimpleTranslation(result.output);
                          setSimpleTranslationResult(result);
                          setDubResult(null);
                          setFinalDubResult(null);
                        })}>
                          {busy === "simple-translate" ? <LoaderCircle className="spin" size={16} /> : <Languages size={16} />} {simpleTranslation ? "Dịch lại sang tiếng Việt" : `Dịch ${translationQuality === "natural" ? "tự nhiên" : "nhanh"}`}
                        </button>
                      </>
                    )}
                  </>
                )}
              </div>
            </div>

            <ChevronRight className="step-arrow" size={18} />

            <div className={`step-card ${dubResult ? "done" : simpleSubtitleReady ? "active" : "disabled"}`}>
              <div className="step-index">{dubResult ? <Check size={16} /> : "3"}</div>
              <div className="step-body">
                <strong>Lồng giọng nữ tiếng Việt</strong>
                <p>Linh · macOS offline · nhạc nền giữ ở mức 24%</p>
                <button className="button primary" disabled={!simpleSubtitleReady || busy !== null || dubBusy !== null} onClick={() => void runDub("test")}>
                  {dubBusy === "test" ? <LoaderCircle className="spin" size={16} /> : <Play size={16} />} Test 5 phút
                </button>
                {dubBusy && dubProgress && (
                  <div className="dub-progress" aria-live="polite">
                    <div><span>{dubProgress.message}</span><strong>{dubProgress.percent.toFixed(0)}%</strong></div>
                    <div className="download-track"><span style={{ width: `${dubProgress.percent}%` }} /></div>
                  </div>
                )}
                {dubResult && (
                  <div className="dub-result">
                    <span><Check size={13} />Đã lồng {dubResult.renderedCues} câu bằng {dubResult.voice}</span>
                    <div className="button-row">
                      <button className="button secondary" onClick={() => {
                        setPreviewPath(dubResult.path);
                        setPreviewStatus("ready");
                        setCurrentTimeMs(0);
                      }}><Play size={14} />Xem bản test</button>
                      <button className="button ghost" onClick={() => void preparePreview(media)}>Xem video gốc</button>
                    </div>
                  </div>
                )}
                <span className="phase-note">Giọng Linh là fallback hiện tại; app sẽ chuyển sang preset Mỹ Duyên khi runtime VieNeu native hoàn tất.</span>
              </div>
            </div>

            <ChevronRight className="step-arrow" size={18} />

            <div className={`step-card ${finalDubResult ? "done" : dubResult ? "active" : "disabled"}`}>
              <div className="step-index">{finalDubResult ? <Check size={16} /> : "4"}</div>
              <div className="step-body">
                <strong>Xuất video lồng tiếng</strong>
                <p>Tạo video đầy đủ sau khi đã nghe và duyệt bản test 5 phút.</p>
                <button className="button primary" disabled={!dubResult || busy !== null || dubBusy !== null} onClick={() => void runDub("export")}>
                  {dubBusy === "export" ? <LoaderCircle className="spin" size={16} /> : <Download size={16} />} Xuất video cuối
                </button>
                {finalDubResult && <span className="output-path"><Check size={13} />{finalDubResult.path}</span>}
              </div>
            </div>
          </>
        ) : (
          <>
            <div className={`step-card ${source ? "done" : "active"}`}>
              <div className="step-index">{source ? <Check size={16} /> : "1"}</div>
              <div className="step-body">
                <strong>Tạo phụ đề ngôn ngữ nguồn</strong>
                <p>Whisper và VAD phân tích 5 phút đầu hoặc dùng SRT có sẵn.</p>
                <label className="language-control">
                  <Languages size={15} />
                  <select value={sourceLanguage} disabled={busy !== null} onChange={(event) => changeSourceLanguage(event.target.value as SourceLanguage)}>
                    {sourceLanguages.map((language) => <option key={language.value} value={language.value}>{language.label}</option>)}
                  </select>
                </label>
                <div className="button-row">
                  <button className="button primary" disabled={busy !== null} onClick={() => void run("asr", () => api.transcribePreview(media.path, sourceLanguage), (document) => {
                    setSource(document);
                    setTranslated(null);
                    setAdvancedTranslationResult(null);
                    setExternalVietnamese(null);
                    setSync(null);
                  })}>
                    {busy === "asr" ? <LoaderCircle className="spin" size={16} /> : <AudioLines size={16} />} Phân tích 5 phút
                  </button>
                  <button className="button secondary" disabled={busy !== null} onClick={() => void chooseSubtitle("source")}><FileText size={16} />Nhập SRT</button>
                </div>
              </div>
            </div>

            <ChevronRight className="step-arrow" size={18} />

            <div className={`step-card ${translated ? "done" : source ? "active" : "disabled"}`}>
              <div className="step-index">{translated ? <Check size={16} /> : "2"}</div>
              <div className="step-body">
                <strong>Dịch bản 5 phút sang Việt</strong>
                <p>Hy-MT2 dịch đúng nghĩa; chế độ Tự nhiên dùng thêm Qwen để biên tập lời thoại.</p>
                {translationOptions(advancedTranslationResult, "translate")}
                <button className="button primary" disabled={!source || busy !== null || missingTranslationModels.length > 0} onClick={() => source && void run("translate", () => api.translateSrt(source.path, source.language, translationQuality), (result) => {
                  setTranslated(result.output);
                  setAdvancedTranslationResult(result);
                  setExternalVietnamese(null);
                  setSync(null);
                })}>
                  {busy === "translate" ? <LoaderCircle className="spin" size={16} /> : <Languages size={16} />} Dịch {translationQuality === "natural" ? "tự nhiên" : "nhanh"}
                </button>
              </div>
            </div>

            <ChevronRight className="step-arrow" size={18} />

            <div className={`step-card ${sync ? "done" : translated ? "active" : "disabled"}`}>
              <div className="step-index">{sync ? <Check size={16} /> : "3"}</div>
              <div className="step-body">
                <strong>Nhập và căn SRT Việt đầy đủ</strong>
                <p>So nội dung với bản dịch 5 phút, rồi áp offset và drift lên toàn bộ file tải về.</p>
                <div className="button-row">
                  <button className="button secondary" disabled={!translated || busy !== null} onClick={() => void chooseSubtitle("full-vi")}><FileText size={16} />Nhập SRT Việt đầy đủ</button>
                  <button className="button primary" disabled={!translated || !externalVietnamese || busy !== null} onClick={() => translated && externalVietnamese && void run("sync", () => api.syncSubtitles(translated.path, externalVietnamese.path), setSync)}>
                    {busy === "sync" ? <LoaderCircle className="spin" size={16} /> : <AlignVerticalJustifyCenter size={16} />} Căn vào video
                  </button>
                </div>
                {externalVietnamese && !sync && (
                  <span className="subtitle-file-status"><Check size={13} />Đã nhập {externalVietnamese.cues.length} câu</span>
                )}
                {sync && (
                  <div className="sync-stats">
                    <span>Lệch {sync.offsetMs > 0 ? "+" : ""}{sync.offsetMs} ms</span>
                    <span>Drift {((sync.speedRatio - 1) * 100).toFixed(3)}%</span>
                    <span>Khớp {sync.matchedCues} câu</span>
                    <span>Tin cậy {(sync.confidence * 100).toFixed(0)}%</span>
                  </div>
                )}
                {sync && sync.confidence < 0.55 && (
                  <span className="sync-warning">Độ tin cậy thấp. Hãy xem thử vài câu đầu trước khi lồng tiếng.</span>
                )}
                {sync && (
                  <div className="review-actions">
                    <span>Kiểm tra nhanh</span>
                    <button className="button secondary" disabled={previewStatus !== "ready"} onClick={() => reviewCue(0.1)}><Play size={14} />Đầu</button>
                    <button className="button secondary" disabled={previewStatus !== "ready"} onClick={() => reviewCue(0.5)}><Play size={14} />Giữa</button>
                    <button className="button secondary" disabled={previewStatus !== "ready"} onClick={() => reviewCue(0.9)}><Play size={14} />Cuối</button>
                  </div>
                )}
              </div>
            </div>

            <ChevronRight className="step-arrow" size={18} />

            <div className={`step-card ${sync ? "active" : "disabled"}`}>
              <div className="step-index">4</div>
              <div className="step-body">
                <strong>Lồng giọng nữ miền Nam</strong>
                <p>VieNeu · Mỹ Duyên · giới hạn tốc độ 0.85–1.18×</p>
                <button className="button primary" disabled><WandSparkles size={16} />Tạo bản lồng tiếng</button>
                <span className="phase-note">Mở ở phase TTS sau khi hoàn tất MVP subtitle.</span>
              </div>
            </div>
          </>
        )}
        {error && <div className="inline-error">{error}</div>}
      </aside>
    </div>
  );
}
