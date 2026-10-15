import { useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { ChevronLeft, ChevronRight, Download, FolderOpen, Pencil, Play, RefreshCw, Square, Volume2 } from "lucide-react";
import { api } from "../lib/tauri";
import { fullSettings, trialIssues, VOICE_PRESETS } from "../lib/audio-preview";
import { timestamp } from "../lib/ocr-editor";
import type { AudioFullPreflight, AudioTrialSettings, OcrAudioProject, OcrDocument } from "../types";

const errorText=(e:unknown)=>e instanceof Error?e.message:String(e);
const bytes=(n:number)=>`${(n/1048576).toFixed(1)} MiB`;
const PAGE=50;
function sameSettings(a:AudioTrialSettings,b:AudioTrialSettings) {
  return a.startMs===b.startMs&&a.durationMs===b.durationMs&&a.voice===b.voice&&JSON.stringify(a.full??null)===JSON.stringify(b.full??null);
}
export function OcrAudioPreview({document,active,blocked,onBusyChange,onEditCue}:{document:OcrDocument;active:boolean;blocked:boolean;onBusyChange:(busy:boolean)=>void;onEditCue?:(index:number)=>void}) {
  const [project,setProject]=useState<OcrAudioProject|null>(null);
  const [settings,setSettings]=useState<AudioTrialSettings>({startMs:0,durationMs:30000,voice:"thuy_dung"});
  const [starting,setStarting]=useState(false),[loading,setLoading]=useState(false),[error,setError]=useState("");
  const [exportBusy,setExportBusy]=useState(false),[issuePage,setIssuePage]=useState(0);
  const [notice,setNotice]=useState("");
  const [preflight,setPreflight]=useState<AudioFullPreflight|null>(null),[checking,setChecking]=useState(false);
  const [requestedJob,setRequestedJob]=useState<string|null>(null);
  const [voicePage,setVoicePage]=useState(0),[metricPage,setMetricPage]=useState(0);
  const player=useRef<HTMLAudioElement>(null),generation=useRef(0);
  const modeSettings=useRef<{trial?:AudioTrialSettings;full?:AudioTrialSettings}>({});
  const job=project?.lastJob;
  const running=starting||job?.status==="running";
  const busy=running||exportBusy;
  const full=!!settings.full;
  const issues=full?(preflight?.issues.map(i=>i.message)??[]):trialIssues(document,settings);
  const preview=full?project?.lastFull:project?.lastPreview;
  const stalePreview=!!preview&&!sameSettings(preview.settings,settings);
  const knownDuration=document.coverage.durationMs;
  useEffect(()=>{onBusyChange(busy);return()=>onBusyChange(false);},[busy,onBusyChange]);
  const create=async()=>{
    const token=++generation.current;setLoading(true);setError("");
    try {const p=await api.createOcrAudioProject(document.sessionId,document.revision);if(generation.current===token){setProject(p);if(p.lastJob)setSettings(p.lastJob.settings);}}
    catch(e){if(generation.current===token)setError(errorText(e));}finally{if(generation.current===token)setLoading(false);}
  };
  useEffect(()=>{if(active&&!blocked&&!project&&!loading&&!error)void create();},[active,blocked,project,loading,error]);
  useEffect(()=>{
    if(!project||!busy)return;let stale=false;
    const timer=setInterval(()=>void api.getOcrAudioJob(project.id).then(lastJob=>{if(!stale)setProject(p=>p?{...p,lastJob}:p);}).catch(e=>{if(!stale)setError(errorText(e));}),1500);
    return()=>{stale=true;clearInterval(timer);};
  },[project?.id,busy]);
  useEffect(()=>{
    setPreflight(null);if(!full||!project||busy||!active)return;
    let stale=false;setChecking(true);
    const timer=setTimeout(()=>void api.preflightOcrAudioFull(project.id,settings).then(p=>{if(!stale){setPreflight(p);setIssuePage(0);}}).catch(e=>{if(!stale)setError(errorText(e));}).finally(()=>{if(!stale)setChecking(false);}),300);
    return()=>{stale=true;clearTimeout(timer);};
  },[project?.id,JSON.stringify(settings),full,busy,active]);
  useEffect(()=>()=>{generation.current++;player.current?.pause();},[]);
  const changeMode=(whole:boolean)=>{
    if(whole===full)return;
    player.current?.pause();
    modeSettings.current[full?"full":"trial"]=settings;
    setError("");setNotice("");setMetricPage(0);setIssuePage(0);
    setSettings(whole?modeSettings.current.full??fullSettings(document,settings.voice):modeSettings.current.trial??{startMs:0,durationMs:30000,voice:settings.voice});
  };
  const render=async()=>{
    if(!project||busy||blocked||issues.length||full&&(!preflight||checking))return;
    setStarting(true);setProject(p=>p?{...p,lastJob:null}:p);setError("");setNotice("");setMetricPage(0);player.current?.pause();
    try {const jobId=await api.newOcrAudioJobId();setRequestedJob(jobId);const result=await api.renderOcrAudioPreview(project.id,jobId,settings);setProject(result);setNotice(full?"Đã lưu WAV toàn timeline":"Đã lưu WAV thử");if(player.current)player.current.currentTime=settings.startMs/1000;}
    catch(e){try{const next=await api.loadOcrAudioProject(project.id);setProject(next);if(next.lastJob?.status==="cancelled")setNotice("Đã hủy WAV");else setError(errorText(e));}catch{setError(errorText(e));}}
    finally{setStarting(false);setRequestedJob(null);}
  };
  const cancel=async()=>{
    if(!project)return;const jobId=job?.status==="running"?job.id:requestedJob;
    if(!jobId)return;
    try{await api.cancelOcrAudioPreview(project.id,jobId);setNotice("Đang yêu cầu hủy WAV");}catch(e){setError(errorText(e));}
  };
  const exporting=async()=>{
    if(!project||!preview||busy)return;setExportBusy(true);setError("");setNotice("Đang xuất WAV...");
    try{
      if(full){const directory=await open({directory:true,multiple:false,title:"Thư mục xuất WAV, SRT và metadata"});if(typeof directory==="string"){const path=await api.exportOcrAudioFull(project.id,preview.key,directory);setNotice(`Đã xuất bộ WAV: ${path}`);}else setNotice("Đã hủy xuất WAV");}
      else{const path=await save({defaultPath:"dub.vi.trial.wav",filters:[{name:"WAV",extensions:["wav"]}]});if(path){await api.exportOcrAudioPreview(project.id,preview.key,path);setNotice("Đã xuất WAV thử");}else setNotice("Đã hủy xuất WAV");}
    }catch(e){setError(errorText(e));setNotice("");}finally{setExportBusy(false);}
  };
  const setVoice=(index:number,voice:string)=>{
    if(!settings.full)return;
    const overrides={...settings.full.cueVoices};if(voice)overrides[index]=voice;else delete overrides[index];
    setSettings({...settings,full:{...settings.full,cueVoices:Object.keys(overrides).length?overrides:undefined}});
  };
  const edit=(index:number)=>{player.current?.pause();onEditCue?.(index);};
  const jobFull=!!job?.settings.full;
  const resumed=job?.status==="interrupted"||job?.status==="cancelled";
  return <section className="ocr-audio-preview">
    <header><h2><Volume2 size={18}/>WAV</h2><div className="ocr-actions"><span>Revision {document.revision.slice(0,12)}</span><button className="icon-button" aria-label="Mở thư mục audio" title="Mở thư mục audio" disabled={!project} onClick={()=>project&&void api.openOcrAudioFolder(project.id).catch(e=>setError(errorText(e)))}><FolderOpen size={16}/></button></div></header>
    <fieldset className="ocr-audio-mode" disabled={busy}><legend>Phạm vi</legend><div className="ocr-tabs"><button aria-pressed={!full} className={!full?"active":""} onClick={()=>changeMode(false)}>Thử</button><button aria-pressed={full} className={full?"active":""} onClick={()=>changeMode(true)}>Toàn tập</button></div></fieldset>
    <div className="ocr-audio-controls">
      {!full?<><label>Bắt đầu (giây)<input aria-label="Bắt đầu WAV thử (giây)" type="number" min="0" max={(7200000-settings.durationMs)/1000} step="0.001" value={settings.startMs/1000} disabled={busy} onChange={e=>setSettings({...settings,startMs:Math.round(Number(e.target.value)*1000)})}/></label>
      <fieldset disabled={busy}><legend>Đoạn thử</legend><div className="ocr-tabs">{[30000,60000].map(durationMs=><button key={durationMs} className={settings.durationMs===durationMs?"active":""} aria-pressed={settings.durationMs===durationMs} onClick={()=>setSettings({...settings,durationMs})}>{durationMs/1000} giây</button>)}</div></fieldset></>:<label>Thời lượng (giây)<input aria-label="Thời lượng WAV toàn tập (giây)" type="number" min="0.001" max="7200" step="0.001" value={settings.durationMs/1000} disabled={busy||knownDuration>0} onChange={e=>setSettings({...settings,durationMs:Math.round(Number(e.target.value)*1000)})}/></label>}
      <label>Giọng mặc định<select aria-label="Giọng WAV" disabled={busy} value={settings.voice} onChange={e=>setSettings({...settings,voice:e.target.value})}>{VOICE_PRESETS.map(v=><option key={v.id} value={v.id}>{v.label}</option>)}</select></label>
      <div className="ocr-actions">{running?<button className="button secondary" disabled={!requestedJob&&job?.status!=="running"} onClick={()=>void cancel()}><Square size={15}/>Hủy</button>:<button className="button primary" disabled={busy||!project||loading||blocked||issues.length>0||full&&(!preflight||checking)} onClick={()=>void render()}><Play size={15}/>{resumed?"Tiếp tục tạo WAV":full?"Tạo WAV toàn tập":"Tạo WAV thử"}</button>}
        <button className="icon-button" title="Tải lại dự án audio" aria-label="Tải lại dự án audio" disabled={busy||blocked||loading} onClick={()=>void create()}><RefreshCw size={16}/></button></div>
    </div>
    <div className="ocr-audio-range"><span>WAV: {timestamp(0)} → {timestamp(settings.startMs+settings.durationMs)}</span>{!full&&<span>Đoạn: {timestamp(settings.startMs)} → {timestamp(settings.startMs+settings.durationMs)}</span>}</div>
    {full&&<label className="ocr-reviewed ocr-audio-partial"><input type="checkbox" disabled={busy} checked={settings.full!.allowGaps} onChange={e=>setSettings({...settings,full:{...settings.full!,allowGaps:e.target.checked}})}/>Tôi chấp nhận bản Một phần khi thiếu vùng thu hoặc chưa biết thời lượng</label>}
    {blocked&&<div className="ocr-warning">Bản OCR chưa lưu hoặc đã thay đổi.</div>}
    {loading&&<p className="ocr-audio-notice">Đang mở snapshot phụ đề...</p>}
    {full&&checking&&!busy&&<p className="ocr-audio-notice">Đang kiểm tra toàn timeline và dung lượng...</p>}
    {full&&preflight&&<>
      <div className="ocr-audio-storage"><span>{preflight.reviewedCount}/{preflight.cueCount} câu đã kiểm tra</span><span>{preflight.partial?"Một phần":"Đủ vùng thu · Chưa xác minh cả tập"}</span><span>WAV {bytes(preflight.wavBytes)}</span><span>Dự án {bytes(preflight.projectBytes)} / {bytes(preflight.projectQuotaBytes)}</span><span>Thư viện {bytes(preflight.libraryBytes)} / {bytes(preflight.libraryQuotaBytes)}</span><span>Ổ trống {bytes(preflight.freeBytes)} · Dự trù {bytes(preflight.estimatedGrowthBytes)}</span></div>
      {preflight.warnings.map((w,i)=><p key={i} className="ocr-warning-text">{w}</p>)}
      {preflight.gaps.length>0&&<details className="ocr-coverage"><summary>{preflight.gaps.length} khoảng chưa thu</summary><div>{preflight.gaps.slice(0,200).map((g,i)=><span key={i}>{timestamp(g.startMs)} → {timestamp(g.endMs)}</span>)}</div></details>}
    </>}
    {issues.length>0&&<><div className="ocr-audio-issues" role="status">{issues.slice(issuePage*PAGE,(issuePage+1)*PAGE).map((issue,i)=>{const index=issuePage*PAGE+i;const cueIndex=preflight?.issues[index]?.cueIndex;return <div key={index}><span>{issue}</span>{full&&cueIndex!=null&&onEditCue&&<button className="icon-button" title={`Sửa câu ${cueIndex}`} aria-label={`Sửa câu ${cueIndex}`} disabled={busy} onClick={()=>edit(cueIndex)}><Pencil size={14}/></button>}</div>;})}</div><Pager page={issuePage} total={issues.length} change={setIssuePage}/></>}
    {error&&<div className="inline-error" role="alert">{error}</div>}
    {notice&&<p className="ocr-audio-notice" role="status">{notice}</p>}
    {job&&<div className="ocr-audio-job"><div><strong>{job.status==="running"?job.message:({completed:jobFull?"Đã lưu WAV toàn timeline":"Đã lưu WAV thử",failed:"WAV có lỗi",cancelled:"Đã hủy",interrupted:"Gián đoạn"} as Record<string,string>)[job.status]??job.status}</strong><span>{job.processed}/{job.total} câu · {job.generated} mới · {job.reused} cache · {job.elapsedSeconds} s{job.etaSeconds!=null?` · còn ~${job.etaSeconds} s`:""}</span></div><progress value={job.percent} max="100"/>{job.warnings.map((w,i)=><div className="ocr-audio-runtime-issue" key={i}><p className="ocr-warning-text">{w}</p>{onEditCue&&/cue (\d+)/.test(w)&&<button className="icon-button" disabled={busy} title="Sửa câu quá dài" aria-label={`Sửa câu ${w.match(/cue (\d+)/)![1]}`} onClick={()=>edit(Number(w.match(/cue (\d+)/)![1]))}><Pencil size={14}/></button>}</div>)}</div>}
    {preview&&<section className="ocr-audio-result"><header><div><strong>{stalePreview?"WAV đã lưu · Cấu hình khác":full?"WAV toàn timeline đã lưu":"WAV thử đã lưu"}</strong><small>{VOICE_PRESETS.find(v=>v.id===preview.settings.voice)?.label??preview.settings.voice} · {preview.renderedCues} câu{preview.settings.full?` · ${Object.keys(preview.settings.full.cueVoices??{}).length} giọng riêng`:""} · {timestamp(0)} → {timestamp(preview.durationMs)}</small></div><button className="button secondary" disabled={busy} onClick={()=>void exporting()}><Download size={15}/>{full?"Xuất bộ WAV":"Xuất WAV"}</button></header>
      {full&&<p className={preview.partial?"ocr-warning-text":"ocr-audio-notice"}>{preview.partial?"Một phần · Có khoảng chưa thu hoặc thời lượng do người dùng nhập":"Đủ vùng thu · Chưa xác minh chất lượng cả tập"}</p>}
      <audio ref={player} controls preload="metadata" src={"__TAURI_INTERNALS__" in window?convertFileSrc(preview.path):preview.path} onLoadedMetadata={()=>{if(player.current)player.current.currentTime=preview.settings.startMs/1000;}}/>
    </section>}
    {full&&<details className="ocr-audio-voices"><summary>Giọng từng câu · {Object.keys(settings.full!.cueVoices??{}).length} riêng</summary><div className="ocr-audio-metrics"><table><thead><tr><th>Câu</th><th>Phụ đề</th><th>Giọng</th></tr></thead><tbody>{document.cues.slice(voicePage*PAGE,(voicePage+1)*PAGE).map(c=><tr key={c.index}><td>{c.index}</td><td className="ocr-audio-cue-text">{c.text}</td><td><select aria-label={`Giọng câu ${c.index}`} disabled={busy} value={settings.full!.cueVoices?.[c.index]??""} onChange={e=>setVoice(c.index,e.target.value)}><option value="">Mặc định</option>{VOICE_PRESETS.map(v=><option key={v.id} value={v.id}>{v.label}</option>)}</select></td></tr>)}</tbody></table></div><Pager page={voicePage} total={document.cues.length} change={setVoicePage}/></details>}
    {job&&job.metrics.length>0&&<><div className="ocr-audio-metrics"><table><thead><tr><th>Câu</th><th>Khoảng thoại</th><th>Audio</th><th>Tempo</th><th>Cache</th></tr></thead><tbody>{job.metrics.slice(metricPage*PAGE,(metricPage+1)*PAGE).map((m,i)=><tr key={`${m.index}-${i}`}><td>{m.index}</td><td>{((m.endMs-m.startMs)/1000).toFixed(2)} s</td><td>{(m.audioDurationMs/1000).toFixed(2)} s</td><td>{m.tempo.toFixed(2)}×</td><td>{m.reused?"Có":"Mới"}</td></tr>)}</tbody></table></div><Pager page={metricPage} total={job.metrics.length} change={setMetricPage}/></>}
  </section>;
}
function Pager({page,total,change}:{page:number;total:number;change:(page:number)=>void}) {
  if(total<=PAGE)return null;return <div className="ocr-audio-pager"><button className="icon-button" aria-label="Trang trước" title="Trang trước" disabled={page===0} onClick={()=>change(page-1)}><ChevronLeft size={15}/></button><span>{page+1}/{Math.ceil(total/PAGE)}</span><button className="icon-button" aria-label="Trang sau" title="Trang sau" disabled={(page+1)*PAGE>=total} onClick={()=>change(page+1)}><ChevronRight size={15}/></button></div>;
}
