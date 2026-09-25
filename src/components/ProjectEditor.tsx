import { convertFileSrc } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import {
  AlertTriangle,
  BookOpen,
  Captions,
  Check,
  ChevronLeft,
  Download,
  FileWarning,
  Film,
  FolderOpen,
  Languages,
  LoaderCircle,
  Mic2,
  Play,
  Plus,
  RefreshCw,
  Save,
  Square,
  Trash2,
  Users,
  Volume2,
  WandSparkles,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../lib/tauri";
import type {
  DialogueCue,
  DubbingProject,
  ModelInfo,
  ProjectProfile,
  ProjectProgress,
  ProjectSummary,
} from "../types";

interface Props {
  modelRevision: number;
  onOpenModels: () => void;
  onOpenDiagnostics: () => void;
}

const profiles: { id: ProjectProfile; label: string }[] = [
  { id: "auto", label: "Tự động" },
  { id: "modern", label: "Hiện đại" },
  { id: "anime", label: "Anime" },
  { id: "historical", label: "Cổ trang" },
  { id: "documentary", label: "Tài liệu" },
];

const voices = [
  { id: "thuy_dung", label: "Thùy Dung · nữ" },
  { id: "thuc_doan", label: "Thục Đoan · nữ" },
  { id: "my_duyen", label: "Mỹ Duyên · nữ" },
  { id: "kim_thanh", label: "Kim Thanh · nữ" },
  { id: "thai_son", label: "Thái Sơn · nam" },
  { id: "minh_triet", label: "Minh Triết · nam" },
  { id: "duc_tri", label: "Đức Trí · nam" },
];

const formatTime = (milliseconds: number) => {
  const totalSeconds = Math.max(0, milliseconds) / 1000;
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${String(minutes).padStart(2, "0")}:${seconds.toFixed(1).padStart(4, "0")}`;
};

const videoUrl = (path: string | null | undefined) => path ? convertFileSrc(path) : undefined;

const formatDuration = (seconds: number | null | undefined) => {
  if (seconds === null || seconds === undefined) return "--";
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const rest = Math.floor(seconds % 60);
  return hours > 0 ? `${hours}g ${minutes}p` : `${minutes}p ${String(rest).padStart(2, "0")}s`;
};

export function ProjectEditor({ modelRevision, onOpenModels, onOpenDiagnostics }: Props) {
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [project, setProject] = useState<DubbingProject | null>(null);
  const [models, setModels] = useState<ModelInfo[]>([]);
  const [sourceLanguage, setSourceLanguage] = useState<"ja" | "en">("ja");
  const [profile, setProfile] = useState<ProjectProfile>("auto");
  const [srtLanguage, setSrtLanguage] = useState<"en" | "vi">("vi");
  const [selectedCueIds, setSelectedCueIds] = useState<string[]>([]);
  const [focusedCueId, setFocusedCueId] = useState<string | null>(null);
  const [inspector, setInspector] = useState<"characters" | "voices" | "glossary" | "warnings">("voices");
  const [progress, setProgress] = useState<ProjectProgress | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [stopping, setStopping] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [currentMs, setCurrentMs] = useState(0);
  const [previewPath, setPreviewPath] = useState<string | null>(null);
  const [previewStartMs, setPreviewStartMs] = useState<number | null>(null);
  const videoRef = useRef<HTMLVideoElement>(null);
  const timelineRef = useRef<HTMLDivElement>(null);
  const notifiedJobs = useRef(new Set<string>());

  const refreshHome = async () => {
    const [recent, availableModels] = await Promise.all([api.listProjects(), api.models()]);
    setProjects(recent);
    setModels(availableModels);
  };

  useEffect(() => {
    void refreshHome().catch((reason) => setError(String(reason)));
  }, [modelRevision]);

  useEffect(() => {
    let stop: (() => void) | undefined;
    void api.onProjectProgress((next) => setProgress((current) => {
      if (!current || current.jobId !== next.jobId || next.status !== "running") return next;
      return { ...next, percent: Math.max(current.percent, next.percent) };
    })).then((unlisten) => (stop = unlisten));
    return () => stop?.();
  }, []);

  useEffect(() => {
    const job = project?.lastJob;
    if (!job || !["completed", "failed", "cancelled"].includes(job.status) || notifiedJobs.current.has(job.id)) return;
    notifiedJobs.current.add(job.id);
    void (async () => {
      try {
        if (await isPermissionGranted()) {
          sendNotification({
            title: job.status === "completed" ? "Lồng tiếng hoàn tất" : "Tác vụ lồng tiếng đã dừng",
            body: job.message,
          });
        }
      } catch {
        // Banner trong app vẫn là nguồn trạng thái chính khi macOS từ chối notification.
      }
    })();
  }, [project?.lastJob]);

  const installed = useMemo(() => new Set(models.filter((model) => model.state === "installed").map((model) => model.id)), [models]);
  const translationReady = installed.has("hy-mt2-7b-q4") && installed.has("qwen3-14b-q4");
  const ttsReady = installed.has("vieneu-v3-turbo-q8");
  const focusedCue = project?.cues.find((cue) => cue.id === focusedCueId) ?? null;
  const activeCue = project?.cues.find((cue) => cue.startMs <= currentMs && cue.endMs >= currentMs) ?? null;
  const activeCueId = activeCue?.id ?? null;
  const shownVideoPath = previewPath ?? project?.assets.previewOutput ?? project?.previewPath ?? project?.videoPath;
  const sourceCueCount = project?.cues.filter((cue) => cue.sourceText.trim()).length ?? 0;
  const vietnameseCues = project?.cues.filter((cue) => cue.translatedText.trim()) ?? [];
  const vietnameseCueCount = vietnameseCues.length;
  const voicedCueCount = vietnameseCues.filter((cue) => cue.audioPath).length;
  const existingSelectedCueIds = selectedCueIds.filter((id) => project?.cues.some((cue) => cue.id === id));
  const selectedVietnameseCueIds = existingSelectedCueIds.filter((id) => vietnameseCues.some((cue) => cue.id === id));
  const subtitlesReady = Boolean(project?.cues.length);
  const vietnameseReady = subtitlesReady && vietnameseCueCount === project?.cues.length;
  const voicesReady = vietnameseReady && voicedCueCount === vietnameseCueCount;
  const resumableSynthesis = project?.lastJob?.kind === "synthesize"
    && ["failed", "cancelled", "interrupted"].includes(project.lastJob.status)
    && !voicesReady;
  const importedSubtitleLabel = sourceCueCount > 0 ? "SRT tiếng Anh" : "SRT tiếng Việt";
  const nextStepMessage = !subtitlesReady
    ? "Bước tiếp theo: nhập file SRT đầy đủ từ máy."
    : !vietnameseReady
      ? "Bước tiếp theo: dịch SRT nguồn sang tiếng Việt."
      : voicedCueCount === 0
        ? "Bước tiếp theo: Test 5 phút, sau đó tạo giọng cho toàn bộ phim."
        : !voicesReady
          ? `Còn ${vietnameseCueCount - voicedCueCount} cue cần tạo giọng trước khi xuất.`
          : "Đã đủ phụ đề và audio Việt. Dự án sẵn sàng xuất video.";

  useEffect(() => {
    if (!activeCueId) return;
    setFocusedCueId(activeCueId);
    const row = timelineRef.current?.querySelector<HTMLElement>(`[data-cue-id="${activeCueId}"]`);
    row?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [activeCueId]);

  const run = async (name: string, action: () => Promise<DubbingProject | void>) => {
    setBusy(name);
    setError(null);
    setProgress(null);
    try {
      const next = await action();
      if (next) setProject(next);
      await refreshHome();
    } catch (reason) {
      setError(String(reason));
      if (project?.id) {
        try {
          setProject(await api.loadProject(project.id));
        } catch {
          // Giữ project hiện tại nếu checkpoint không thể đọc lại.
        }
      }
    } finally {
      setBusy(null);
      setStopping(false);
    }
  };

  const prepareLongJob = async () => {
    try {
      if (await isPermissionGranted() || localStorage.getItem("long-job-notification-asked")) return;
      localStorage.setItem("long-job-notification-asked", "1");
      await requestPermission();
    } catch {
      // Tác vụ vẫn chạy bình thường với banner trong app.
    }
  };

  const chooseVideo = async () => {
    const selected = await open({ multiple: false, filters: [{ name: "Video", extensions: ["mp4", "mkv", "mov", "m4v", "avi", "webm"] }] });
    if (!selected) return;
    await run("create", async () => {
      const created = await api.createProject(selected, sourceLanguage, profile);
      setFocusedCueId(null);
      setSelectedCueIds([]);
      setPreviewPath(null);
      setPreviewStartMs(null);
      return created;
    });
  };

  const openProject = async (id: string) => {
    await run("open", async () => {
      const loaded = await api.loadProject(id);
      setFocusedCueId(loaded.cues[0]?.id ?? null);
      setSelectedCueIds([]);
      setInspector("voices");
      setPreviewPath(null);
      setPreviewStartMs(null);
      return loaded;
    });
  };

  const patchProject = (mutate: (draft: DubbingProject) => void) => {
    setProject((current) => {
      if (!current) return current;
      const next = structuredClone(current);
      mutate(next);
      return next;
    });
  };

  const persist = async () => {
    if (!project) return;
    await run("save", async () => api.updateProject(project));
  };

  const importSrt = async () => {
    if (!project) return;
    const selected = await open({ multiple: false, filters: [{ name: "SubRip", extensions: ["srt"] }] });
    if (!selected) return;
    await run("import", async () => {
      const saved = await api.updateProject(project);
      const imported = await api.importProjectSrt(saved.id, selected, srtLanguage);
      setSelectedCueIds([]);
      setFocusedCueId(imported.cues[0]?.id ?? null);
      setInspector("voices");
      return imported;
    });
  };

  const translate = async () => {
    if (!project) return;
    await run("translate", async () => {
      const saved = await api.updateProject(project);
      return api.translateProject(saved.id);
    });
  };

  const rememberCorrection = async (cue: DialogueCue) => {
    if (!project || !cue.sourceText.trim() || !cue.machineTranslatedText.trim() || cue.translatedText.trim() === cue.machineTranslatedText.trim()) return;
    try {
      await api.saveTranslationCorrection(project.id, cue.id, cue.translatedText);
    } catch (reason) {
      setError(`Không lưu được mẫu văn phong: ${String(reason)}`);
    }
  };

  const synthesize = async (cueIds = selectedCueIds, maxSeconds: number | null = null) => {
    if (!project) return;
    const existingIds = cueIds.filter((id) => project.cues.some((cue) => cue.id === id));
    const validIds = existingIds.filter((id) => project.cues.some((cue) => cue.id === id && cue.translatedText.trim()));
    if (existingIds.length > 0 && validIds.length === 0) {
      setError("Các cue đang chọn chưa có lời tiếng Việt. Hãy hoàn thành bước Dịch Việt hoặc bỏ chọn để tạo giọng cho toàn bộ cue Việt.");
      return;
    }
    await prepareLongJob();
    await run("synthesize", async () => {
      const saved = await api.updateProject(project);
      return (await api.synthesizeCues(saved.id, validIds, maxSeconds)).project;
    });
  };

  const testFiveMinutes = async () => {
    if (!project) return;
    await prepareLongJob();
    await run("test", async () => {
      const saved = await api.updateProject(project);
      const voiced = await api.synthesizeCues(saved.id, [], 300);
      const rendered = await api.renderProject(saved.id, null, 300);
      const firstDialogue = voiced.project.cues.find((cue) => cue.audioPath && cue.startMs >= 60_000 && cue.startMs < 300_000)
        ?? voiced.project.cues.find((cue) => cue.audioPath && cue.startMs < 300_000);
      setPreviewStartMs(firstDialogue?.startMs ?? 0);
      setPreviewPath(rendered.path);
      return { ...rendered.project, cues: voiced.project.cues };
    });
  };

  const exportVideo = async () => {
    if (!project) return;
    const destination = await save({
      defaultPath: `${project.title}-long-tieng.mp4`,
      filters: [
        { name: "MP4 lồng tiếng", extensions: ["mp4"] },
        { name: "MKV nhiều track", extensions: ["mkv"] },
      ],
    });
    if (!destination) return;
    await prepareLongJob();
    await run("render", async () => {
      const saved = await api.updateProject(project);
      const result = await api.renderProject(saved.id, destination, null);
      return result.project;
    });
  };

  const stopJob = async () => {
    setStopping(true);
    try {
      await api.cancelJob();
    } catch (reason) {
      setError(String(reason));
    }
  };

  const toggleCue = (id: string) => setSelectedCueIds((current) => current.includes(id) ? current.filter((value) => value !== id) : [...current, id]);

  const focusCue = (cue: DialogueCue) => {
    setFocusedCueId(cue.id);
    setCurrentMs(cue.startMs);
    if (videoRef.current) videoRef.current.currentTime = cue.startMs / 1000;
  };

  const playCueAudio = (cue: DialogueCue) => {
    if (!cue.audioPath) return;
    const audio = new Audio(convertFileSrc(cue.audioPath));
    audio.volume = Math.min(1, Math.max(0, cue.volume));
    void audio.play().catch((reason) => setError(String(reason)));
  };

  if (!project) {
    return (
      <section className="project-home">
        <div className="new-project-panel">
          <div className="new-project-icon"><Film size={28} /></div>
          <h2>Tạo dự án lồng tiếng</h2>
          <p>Nhập phim tiếng Nhật hoặc tiếng Anh. Mọi model và dữ liệu dự án nằm trên máy.</p>
          <div className="project-start-options">
            <label>Ngôn ngữ nguồn<select value={sourceLanguage} onChange={(event) => setSourceLanguage(event.target.value as "ja" | "en")}><option value="ja">Tiếng Nhật</option><option value="en">Tiếng Anh</option></select></label>
            <label>Phong cách<select value={profile} onChange={(event) => setProfile(event.target.value as ProjectProfile)}>{profiles.map((item) => <option value={item.id} key={item.id}>{item.label}</option>)}</select></label>
          </div>
          <button className="button primary create-project-button" onClick={() => void chooseVideo()} disabled={Boolean(busy)}>
            {busy === "create" ? <LoaderCircle className="spin" size={17} /> : <FolderOpen size={17} />} Chọn video
          </button>
          <button className="button ghost" onClick={onOpenModels}>Kiểm tra model local</button>
        </div>
        <div className="recent-projects">
          <div className="section-heading"><div><p className="eyebrow">GẦN ĐÂY</p><h2>Dự án đã lưu</h2></div><button className="icon-button" title="Tải lại" onClick={() => void refreshHome()}><RefreshCw size={16} /></button></div>
          {projects.length === 0 ? <div className="recent-empty">Chưa có dự án nào.</div> : projects.map((item) => (
            <button className="recent-project" key={item.id} onClick={() => void openProject(item.id)}>
              <span className="recent-thumb"><Film size={18} /></span>
              <span><strong>{item.title}</strong><small>{item.cueCount} cue · {item.sourceLanguage === "ja" ? "Nhật" : "Anh"} → Việt</small></span>
              <Play size={15} />
            </button>
          ))}
        </div>
        {error && <div className="project-error">{error}</div>}
      </section>
    );
  }

  return (
    <section className="editor-shell">
      <div className="editor-header">
        <div className="editor-project-bar">
          <button className="icon-button" title="Về danh sách dự án" onClick={() => { setProject(null); setPreviewPath(null); }}><ChevronLeft size={18} /></button>
          <div className="project-title"><strong>{project.title}</strong><span>{project.cues.length} cue · {project.sourceLanguage === "ja" ? "Nhật" : "Anh"} → Việt</span></div>
          <label className="profile-control"><span>Phong cách</span><select className="toolbar-select" value={project.profile} onChange={(event) => patchProject((draft) => { draft.profile = event.target.value as ProjectProfile; draft.translationGuide.scenes = []; draft.translationGuide.preparedAtMs = 0; })}>{profiles.map((item) => <option value={item.id} key={item.id}>{item.label}</option>)}</select></label>
          <span className="workflow-next">{nextStepMessage}</span>
          <button className="button secondary" onClick={() => void persist()} disabled={Boolean(busy)}><Save size={15} /> Lưu</button>
        </div>

        <div className="workflow-bar">
          <div className={`workflow-step ${subtitlesReady ? "done" : "current"}`}>
            <span className="workflow-step-number">{subtitlesReady ? <Check size={13} /> : "1"}</span>
            <div className="workflow-step-body"><div><strong>Nhập SRT đầy đủ</strong><small>{subtitlesReady ? `${project.cues.length} cue · ${importedSubtitleLabel}` : "Chọn SRT Việt hoặc Anh đã tải về"}</small></div><div className="workflow-actions"><div className="import-control"><select value={srtLanguage} onChange={(event) => setSrtLanguage(event.target.value as "en" | "vi")}><option value="vi">SRT Việt</option><option value="en">SRT Anh</option></select><button className="button primary" onClick={() => void importSrt()} disabled={Boolean(busy)}><FolderOpen size={14} /> Chọn file SRT</button></div></div></div>
          </div>

          <div className={`workflow-step ${vietnameseReady ? "done" : subtitlesReady ? "current" : "locked"}`}>
            <span className="workflow-step-number">{vietnameseReady ? <Check size={13} /> : "2"}</span>
            <div className="workflow-step-body"><div><strong>Dịch sang Việt <span className="optional-label">Tùy chọn</span></strong><small>{!subtitlesReady ? "Cần nhập SRT ở bước 1" : sourceCueCount > 0 ? vietnameseReady ? `${vietnameseCueCount}/${project.cues.length} cue Việt hoàn tất` : "AI tự đọc toàn bộ kịch bản trước khi dịch" : "SRT đã là tiếng Việt · không cần dịch"}</small></div><div className="workflow-actions">{!subtitlesReady ? <span className="workflow-skip">Chưa có SRT</span> : sourceCueCount > 0 ? <><button className="button primary" onClick={() => void translate()} disabled={Boolean(busy) || !translationReady} title="Tự đọc toàn bộ kịch bản rồi dịch và biên tập"><Languages size={14} /> {vietnameseCueCount ? "Dịch lại toàn bộ" : "Dịch sang Việt"}</button>{!translationReady && <button className="button ghost" onClick={onOpenModels}>Tải model</button>}</> : <span className="workflow-skip"><Check size={13} /> Đã bỏ qua</span>}</div></div>
          </div>

          <div className={`workflow-step ${voicesReady ? "done" : vietnameseReady ? "current" : "locked"}`}>
            <span className="workflow-step-number">{voicesReady ? <Check size={13} /> : "3"}</span>
            <div className="workflow-step-body"><div><strong>Tạo giọng & kiểm tra</strong><small>{voicesReady ? `${voicedCueCount}/${vietnameseCueCount} cue có audio · ${project.speakers.length} nhân vật` : vietnameseReady ? `${voicedCueCount}/${vietnameseCueCount} cue có audio · dùng giọng đã gán trong tab Nhân vật` : "Cần lời thoại tiếng Việt"}</small></div><div className="workflow-actions"><button className="button secondary" onClick={() => void testFiveMinutes()} disabled={Boolean(busy) || !ttsReady || !vietnameseReady} title="Tạo giọng và mix 5 phút đầu"><Play size={14} /> Test 5 phút</button><button className="button primary" onClick={() => void synthesize()} disabled={Boolean(busy) || !ttsReady || !vietnameseReady || (existingSelectedCueIds.length > 0 && selectedVietnameseCueIds.length === 0)} title={!vietnameseReady ? "Hãy dịch hoặc nhập SRT Việt trước" : !ttsReady ? "Cần tải VieNeu-TTS" : selectedVietnameseCueIds.length ? "Tạo giọng cho cue đang chọn" : "Tạo giọng cho toàn bộ cue Việt"}><Mic2 size={14} /> {selectedVietnameseCueIds.length ? `Tạo ${selectedVietnameseCueIds.length} câu` : resumableSynthesis ? "Tiếp tục tạo toàn bộ" : "Tạo toàn bộ"}</button>{!ttsReady && <button className="button ghost" onClick={onOpenModels}>Tải model</button>}</div></div>
          </div>

          <div className={`workflow-step ${voicesReady ? "current" : "locked"}`}>
            <span className="workflow-step-number">4</span>
            <div className="workflow-step-body"><div><strong>Xuất video</strong><small>{voicesReady ? "Sẵn sàng MP4 hoặc MKV" : `Còn ${Math.max(0, vietnameseCueCount - voicedCueCount)} cue chưa có audio`}</small></div><div className="workflow-actions"><button className="button primary" onClick={() => void exportVideo()} disabled={Boolean(busy) || !voicesReady} title={!voicesReady ? "Phải tạo đủ giọng trước khi xuất" : "Xuất video hoàn chỉnh"}><Download size={14} /> Xuất video</button></div></div>
          </div>
        </div>
      </div>

      {busy && (
        <div className="project-progress-bar">
          <div><LoaderCircle className="spin" size={14} /><span>{progress?.message ?? "Đang chuẩn bị..."}{progress?.total ? ` · ${progress.processed}/${progress.total} · cache ${progress.reused} · ${formatDuration(progress.elapsedSeconds)} · ETA ${formatDuration(progress.etaSeconds)}` : ""}</span><strong>{Math.round(progress?.percent ?? 0)}%</strong><button className="icon-button" onClick={() => void stopJob()} disabled={stopping} title="Hủy tác vụ"><Square size={13} /></button></div>
          <span style={{ width: `${progress?.percent ?? 2}%` }} />
          {stopping && <em>Đang dừng process...</em>}
        </div>
      )}

      {!busy && project.lastJob && (
        <div className={`job-banner ${project.lastJob.status}`}>
          <span>{project.lastJob.status === "completed" ? <Check size={16} /> : <AlertTriangle size={16} />}</span>
          <div><strong>{project.lastJob.message}</strong><small>{project.lastJob.kind === "synthesize" ? `${project.lastJob.processed}/${project.lastJob.total} cue · ${project.lastJob.generated} tạo mới · ${project.lastJob.reused} dùng cache` : `Pha ${project.lastJob.stage}`} · {formatDuration(project.lastJob.elapsedSeconds)}{project.lastJob.error?.cueIndex ? ` · cue ${project.lastJob.error.cueIndex}` : ""}{project.lastJob.error?.voice ? ` · ${project.lastJob.error.voice}` : ""}</small></div>
          {project.lastJob.status !== "completed" && <button className="button ghost" onClick={onOpenDiagnostics}><FileWarning size={14} /> Nhật ký</button>}
          {resumableSynthesis && <button className="button primary" onClick={() => void synthesize([], null)}><RefreshCw size={14} /> Tiếp tục</button>}
        </div>
      )}

      <div className="editor-grid">
        <div className="editor-preview-column">
          <div className="editor-video">
            <video
              ref={videoRef}
              src={videoUrl(shownVideoPath)}
              controls
              onLoadedMetadata={(event) => {
                if (previewPath && previewStartMs !== null) {
                  event.currentTarget.currentTime = previewStartMs / 1000;
                  setCurrentMs(previewStartMs);
                }
              }}
              onTimeUpdate={(event) => setCurrentMs(event.currentTarget.currentTime * 1000)}
            />
            {activeCue?.translatedText && <div className="editor-subtitle"><span>{activeCue.translatedText}</span></div>}
          </div>
          <div className="preview-meta">
            <span><Film size={14} />{shownVideoPath?.split("/").pop()}</span>
            {previewPath && <span className="preview-ready"><Check size={14} /> Bản test đang mở tại {formatTime(previewStartMs ?? 0)}</span>}
          </div>
          {focusedCue && (
            <CueInspector cue={focusedCue} project={project} onChange={(next) => patchProject((draft) => {
              const index = draft.cues.findIndex((cue) => cue.id === next.id);
              if (index >= 0) draft.cues[index] = next;
            })} onCommit={() => void rememberCorrection(focusedCue)} onRegenerate={() => void synthesize([focusedCue.id])} busy={Boolean(busy)} />
          )}
          {error && <div className="project-error">{error}</div>}
        </div>

        <div className="timeline-column">
          <div className="timeline-heading"><div><p className="eyebrow">TIMELINE</p><h2>Lời thoại</h2></div><label><input type="checkbox" checked={project.cues.length > 0 && selectedCueIds.length === project.cues.length} onChange={(event) => setSelectedCueIds(event.target.checked ? project.cues.map((cue) => cue.id) : [])} /> Chọn tất cả</label></div>
          <div className="dialogue-list" ref={timelineRef}>
            {project.cues.length === 0 ? <div className="timeline-empty"><Captions size={24} /><strong>Chưa có lời thoại</strong><span>Nhập file SRT đầy đủ ở bước 1.</span></div> : project.cues.map((cue) => {
              const speaker = project.speakers.find((item) => item.id === cue.speakerId);
              return (
                <article
                  className={`dialogue-row ${focusedCueId === cue.id ? "active" : ""} ${activeCueId === cue.id ? "playing" : ""} ${cue.warnings.length ? "warning" : ""}`}
                  data-cue-id={cue.id}
                  aria-current={activeCueId === cue.id ? "true" : undefined}
                  key={cue.id}
                  onClick={() => focusCue(cue)}
                >
                  <input type="checkbox" checked={selectedCueIds.includes(cue.id)} onChange={(event) => { event.stopPropagation(); toggleCue(cue.id); }} onClick={(event) => event.stopPropagation()} />
                  <span className="speaker-dot" style={{ background: speaker?.color ?? "#78817b" }} />
                  <div className="dialogue-copy"><div><strong>{speaker?.displayName ?? "Chưa gán"}{cue.sceneId ? ` · ${cue.sceneId.replace("scene-", "Cảnh ")}` : ""}</strong><span>{formatTime(cue.startMs)} – {formatTime(cue.endMs)}</span></div><p>{cue.translatedText || cue.sourceText}</p><small>{cue.sourceText && cue.translatedText ? cue.sourceText : cue.status === "voiced" ? "Đã tạo giọng" : ""}</small></div>
                  {cue.audioPath && <button className="cue-audio-button" title="Nghe audio cue" onClick={(event) => { event.stopPropagation(); playCueAudio(cue); }}><Volume2 className="cue-audio-mark" size={14} /></button>}
                  {cue.warnings.length > 0 && <AlertTriangle className="cue-warning-mark" size={14} />}
                </article>
              );
            })}
          </div>
        </div>

        <div className="inspector-column">
          <div className="inspector-tabs">
            <button className={inspector === "characters" ? "active" : ""} onClick={() => setInspector("characters")} title="Nhân vật"><Users size={17} /></button>
            <button className={inspector === "voices" ? "active" : ""} onClick={() => setInspector("voices")} title="Giọng"><Mic2 size={17} /></button>
            <button className={inspector === "glossary" ? "active" : ""} onClick={() => setInspector("glossary")} title="Glossary"><BookOpen size={17} /></button>
            <button className={inspector === "warnings" ? "active" : ""} onClick={() => setInspector("warnings")} title="Cảnh báo"><AlertTriangle size={17} /></button>
          </div>
          {inspector === "characters" && <CharactersPanel project={project} onChange={patchProject} />}
          {inspector === "voices" && <VoicesPanel project={project} onChange={patchProject} disabled={Boolean(busy)} />}
          {inspector === "glossary" && <GlossaryPanel project={project} onChange={patchProject} />}
          {inspector === "warnings" && <WarningsPanel project={project} />}
          <div className="inspector-footer"><button className="button secondary" onClick={() => void persist()} disabled={Boolean(busy)}><Save size={15} /> Lưu thay đổi</button></div>
        </div>
      </div>
    </section>
  );
}

function CueInspector({ cue, project, onChange, onCommit, onRegenerate, busy }: { cue: DialogueCue; project: DubbingProject; onChange: (cue: DialogueCue) => void; onCommit: () => void; onRegenerate: () => void; busy: boolean }) {
  const change = <K extends keyof DialogueCue>(key: K, value: DialogueCue[K]) => {
    const invalidatesAudio = ["translatedText", "speakerId", "startMs", "endMs", "speed", "volume"].includes(key);
    onChange({ ...cue, [key]: value, ...(invalidatesAudio ? { audioPath: null, audioDurationMs: null, status: "translated" } : {}) });
  };
  return (
    <div className="cue-editor">
      <div className="cue-editor-heading"><div><p className="eyebrow">CUE {cue.index}</p><strong>{formatTime(cue.startMs)} – {formatTime(cue.endMs)}</strong></div><button className="button secondary" onClick={onRegenerate} disabled={busy || !cue.translatedText}><WandSparkles size={15} /> Tạo lại câu</button></div>
      {cue.sourceText && <div className="source-line">{cue.sourceText}</div>}
      <textarea value={cue.translatedText} onChange={(event) => change("translatedText", event.target.value)} onBlur={onCommit} placeholder="Lời thoại tiếng Việt" />
      <div className="cue-controls">
        <label>Nhân vật<select value={cue.speakerId} onChange={(event) => change("speakerId", event.target.value)}>{project.speakers.map((speaker) => <option value={speaker.id} key={speaker.id}>{speaker.displayName}</option>)}</select></label>
        <label>Bắt đầu<input type="number" step="100" value={cue.startMs} onChange={(event) => change("startMs", Number(event.target.value))} /></label>
        <label>Kết thúc<input type="number" step="100" value={cue.endMs} onChange={(event) => change("endMs", Number(event.target.value))} /></label>
        <label>Tốc độ<input type="number" min="0.8" max="1.25" step="0.01" value={cue.speed} onChange={(event) => change("speed", Number(event.target.value))} /></label>
        <label>Âm lượng<input type="number" min="0.1" max="2" step="0.05" value={cue.volume} onChange={(event) => change("volume", Number(event.target.value))} /></label>
      </div>
    </div>
  );
}

function CharactersPanel({ project, onChange }: { project: DubbingProject; onChange: (mutate: (draft: DubbingProject) => void) => void }) {
  const removeSpeaker = (index: number) => onChange((draft) => {
    if (draft.speakers.length <= 1) return;
    const removed = draft.speakers[index];
    const replacement = draft.speakers[index === 0 ? 1 : 0];
    draft.cues.filter((cue) => cue.speakerId === removed.id).forEach((cue) => {
      cue.speakerId = replacement.id;
      cue.audioPath = null;
      cue.audioDurationMs = null;
      cue.status = cue.translatedText ? "translated" : "source";
    });
    draft.speakers.splice(index, 1);
  });
  return <div className="inspector-panel"><div className="panel-heading"><div><p className="eyebrow">NHÂN VẬT</p><h2>{project.speakers.length} người nói</h2><small>Thêm và gán nhân vật thủ công cho từng cue</small></div><button className="icon-button" title="Thêm nhân vật" onClick={() => onChange((draft) => draft.speakers.push({ id: `speaker-${Date.now()}`, displayName: `Nhân vật ${draft.speakers.length + 1}`, voicePreset: "thuy_dung", color: "#668b72", notes: "" }))}><Plus size={16} /></button></div>{project.characterBible.summary && <div className="character-summary">{project.characterBible.summary}</div>}{project.speakers.map((speaker, index) => <div className="speaker-card" key={speaker.id}><input type="color" value={speaker.color} onChange={(event) => onChange((draft) => { draft.speakers[index].color = event.target.value; })} /><div><input value={speaker.displayName} onChange={(event) => onChange((draft) => { draft.speakers[index].displayName = event.target.value; })} /><textarea value={speaker.notes} onChange={(event) => onChange((draft) => { draft.speakers[index].notes = event.target.value; })} placeholder="Quan hệ, vai vế, cách xưng hô..." /></div><button className="icon-button danger" title="Gộp cue vào nhân vật khác và xóa" disabled={project.speakers.length <= 1} onClick={() => removeSpeaker(index)}><Trash2 size={14} /></button></div>)}</div>;
}

function VoicesPanel({ project, onChange, disabled }: { project: DubbingProject; onChange: (mutate: (draft: DubbingProject) => void) => void; disabled: boolean }) {
  const [loadingVoice, setLoadingVoice] = useState<string | null>(null);
  const [playingVoice, setPlayingVoice] = useState<string | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const previewAudio = useRef<HTMLAudioElement | null>(null);

  useEffect(() => () => {
    previewAudio.current?.pause();
    previewAudio.current = null;
  }, []);

  const playPreview = async (voice: string) => {
    if (playingVoice === voice && previewAudio.current) {
      previewAudio.current.pause();
      previewAudio.current = null;
      setPlayingVoice(null);
      return;
    }
    previewAudio.current?.pause();
    previewAudio.current = null;
    setPlayingVoice(null);
    setLoadingVoice(voice);
    setPreviewError(null);
    try {
      const path = await api.previewVoice(voice);
      const audio = new Audio(convertFileSrc(path));
      audio.onended = () => {
        previewAudio.current = null;
        setPlayingVoice(null);
      };
      audio.onerror = () => {
        previewAudio.current = null;
        setPlayingVoice(null);
        setPreviewError("Không phát được file nghe thử.");
      };
      previewAudio.current = audio;
      setPlayingVoice(voice);
      await audio.play();
    } catch (reason) {
      setPreviewError(String(reason));
      setPlayingVoice(null);
    } finally {
      setLoadingVoice(null);
    }
  };

  return (
    <div className="inspector-panel">
      <div className="panel-heading"><div><p className="eyebrow">GIỌNG PRESET</p><h2>Nghe thử giọng</h2></div></div>
      <div className="voice-preview-list">
        {voices.map((voice) => (
          <div className={`voice-preview-row ${playingVoice === voice.id ? "playing" : ""}`} key={voice.id}>
            <span>{voice.label}</span>
            <button className="icon-button" disabled={disabled || loadingVoice !== null} onClick={() => void playPreview(voice.id)} title={`Đọc thử: Sushi đẹp trai, ham ăn, ham chơi, thích anime!`}>
              {loadingVoice === voice.id ? <LoaderCircle className="spin" size={14} /> : playingVoice === voice.id ? <Square size={13} /> : <Play size={14} />}
            </button>
          </div>
        ))}
      </div>
      {previewError && <div className="voice-preview-error">{previewError}</div>}
      <p className="panel-subheading">Gán cho nhân vật</p>
      {project.speakers.map((speaker, index) => (
        <label className="voice-row" key={speaker.id}>
          <span className="speaker-dot" style={{ background: speaker.color }} />
          <span><strong>{speaker.displayName}</strong><small>Giữ cố định toàn dự án</small></span>
          <select value={speaker.voicePreset} onChange={(event) => onChange((draft) => {
            draft.speakers[index].voicePreset = event.target.value;
            draft.cues.filter((cue) => cue.speakerId === speaker.id).forEach((cue) => {
              cue.audioPath = null;
              cue.audioDurationMs = null;
            });
          })}>{voices.map((voice) => <option value={voice.id} key={voice.id}>{voice.label}</option>)}</select>
        </label>
      ))}
      <div className="audio-settings"><label><input type="checkbox" checked={project.audioSettings.separateBackground} onChange={(event) => onChange((draft) => { draft.audioSettings.separateBackground = event.target.checked; })} /> Tách vocal khi xuất video cuối</label><small>Test 5 phút luôn dùng ducking nhanh và không chạy tách nền.</small><label>Âm lượng thoại<input type="range" min="0.6" max="1.4" step="0.05" value={project.audioSettings.dialogueVolume} onChange={(event) => onChange((draft) => { draft.audioSettings.dialogueVolume = Number(event.target.value); })} /></label></div>
    </div>
  );
}

function GlossaryPanel({ project, onChange }: { project: DubbingProject; onChange: (mutate: (draft: DubbingProject) => void) => void }) {
  return <div className="inspector-panel"><div className="panel-heading"><div><p className="eyebrow">GLOSSARY</p><h2>Tên riêng và thuật ngữ</h2></div><button className="icon-button" title="Thêm thuật ngữ" onClick={() => onChange((draft) => draft.glossary.push({ source: "", target: "", note: "" }))}><Plus size={16} /></button></div>{project.glossary.length === 0 && <div className="panel-empty">Thêm tên nhân vật, địa danh hoặc thuật ngữ cần dịch nhất quán.</div>}{project.glossary.map((entry, index) => <div className="glossary-row" key={index}><input value={entry.source} placeholder="Nguồn" onChange={(event) => onChange((draft) => { draft.glossary[index].source = event.target.value; })} /><input value={entry.target} placeholder="Tiếng Việt" onChange={(event) => onChange((draft) => { draft.glossary[index].target = event.target.value; })} /><button className="icon-button danger" title="Xóa" onClick={() => onChange((draft) => { draft.glossary.splice(index, 1); })}><Trash2 size={14} /></button></div>)}</div>;
}

function WarningsPanel({ project }: { project: DubbingProject }) {
  const cueWarnings = project.cues.flatMap((cue) => cue.warnings.map((warning) => `Cue ${cue.index}: ${warning}`));
  const warnings = [...project.warnings, ...cueWarnings];
  return <div className="inspector-panel"><div className="panel-heading"><div><p className="eyebrow">KIỂM TRA</p><h2>{warnings.length} cảnh báo</h2></div></div>{warnings.length === 0 ? <div className="panel-empty success"><Check size={18} /> Chưa có cảnh báo.</div> : warnings.map((warning, index) => <div className="warning-row" key={index}><AlertTriangle size={15} /><span>{warning}</span></div>)}</div>;
}
