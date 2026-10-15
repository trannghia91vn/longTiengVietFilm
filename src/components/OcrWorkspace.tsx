import { useCallback, useEffect, useRef, useState } from "react";
import { Check, ChevronLeft, ChevronRight, Download, FolderOpen, Merge, RefreshCw, RotateCcw, Save, ScanText, Scissors, Search, Square, Trash2, Undo2, Redo2, AlertTriangle, ZoomIn, X } from "lucide-react";
import { save } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/tauri";
import { compactCues, invalidCue, mergeCue, mergeSelectedCues, ocrState, selectCueRange, serializeOcr, splitCue, timestamp, warningLabel } from "../lib/ocr-editor";
import type { OcrCandidate, OcrCue, OcrDocument, OcrEvidence, OcrSession } from "../types";
import { OcrAudioPreview } from "./OcrAudioPreview";

const message = (e: unknown) => e instanceof Error ? e.message : String(e);
const sequenceFor=(cues:OcrCue[],selected:number,position:number)=>cues[selected]?.evidenceSequences[Math.min(position,(cues[selected]?.evidenceSequences.length??1)-1)];
function Icon({title, children, disabled, onClick}: {title:string;children:React.ReactNode;disabled?:boolean;onClick:()=>void}) {
  return <button className="icon-button" title={title} aria-label={title} disabled={disabled} onClick={onClick}>{children}</button>;
}
export function OcrWorkspace({active}: {active:boolean}) {
  const [sessions,setSessions]=useState<OcrSession[]>([]);
  const [id,setId]=useState<string|null>(null);
  const [document,setDocument]=useState<OcrDocument|null>(null);
  const [cues,setCues]=useState<OcrCue[]>([]);
  const [selected,setSelected]=useState(0);
  const [selection,setSelection]=useState<number[]>([]);
  const selectionAnchor=useRef<number|null>(null);
  const [past,setPast]=useState<OcrCue[][]>([]),[future,setFuture]=useState<OcrCue[][]>([]);
  const [filter,setFilter]=useState("all"),[query,setQuery]=useState("");
  const [mode,setMode]=useState("table");
  const [error,setError]=useState(""),[notice,setNotice]=useState("");
  const [compactNotice,setCompactNotice]=useState("");
  const [loading,setLoading]=useState(false),[saving,setSaving]=useState(false);
  const [audioBusy,setAudioBusy]=useState(false);
  const [jobs,setJobs]=useState<Set<string>>(new Set());
  const [evidence,setEvidence]=useState<OcrEvidence|null>(null),[evidenceError,setEvidenceError]=useState("");
  const [evidencePosition,setEvidencePosition]=useState(0);
  const [candidates,setCandidates]=useState<OcrCandidate[]>([]),[rechecking,setRechecking]=useState(false);
  const [pending,setPending]=useState<string|null>(null);
  const [confirmJob,setConfirmJob]=useState(false);
  const [deleteTargets,setDeleteTargets]=useState<number[]|null>(null);
  const [zoom,setZoom]=useState(false);
  const [split,setSplit]=useState<{time:number;left:string;right:string}|null>(null);
  const textArea=useRef<HTMLTextAreaElement>(null),loadGeneration=useRef(0);
  const modalRef=useRef<HTMLElement>(null);
  const autoLoadKey=useRef("");
  const sessionsRef=useRef(sessions);sessionsRef.current=sessions;
  const evidenceKey=useRef("");evidenceKey.current=`${id}:${sequenceFor(cues,selected,evidencePosition)}`;
  const dirty=!!document && JSON.stringify(cues)!==JSON.stringify(document.cues);
  useEffect(()=>{
    if(!pending&&!confirmJob&&!deleteTargets&&!split&&!zoom)return;
    const modal=modalRef.current;
    const previous=window.document.activeElement as HTMLElement|null;
    modal?.querySelector<HTMLElement>("button,input,textarea")?.focus();
    const keys=(event:KeyboardEvent)=>{
      if(event.key==="Escape"){setPending(null);setConfirmJob(false);setDeleteTargets(null);setSplit(null);setZoom(false);}
      if(event.key==="Tab"&&modal){const elements=[...modal.querySelectorAll<HTMLElement>('button:not(:disabled),input,textarea')];const first=elements[0],last=elements[elements.length-1];if(event.shiftKey&&window.document.activeElement===first){event.preventDefault();last?.focus();}else if(!event.shiftKey&&window.document.activeElement===last){event.preventDefault();first?.focus();}}
    };
    window.addEventListener("keydown",keys);return()=>{window.removeEventListener("keydown",keys);previous?.focus();};
  },[pending,confirmJob,deleteTargets,!!split,zoom]);
  const dirtyRef=useRef(dirty);dirtyRef.current=dirty;
  const current=sessions.find(s=>s.sessionId===id);
  const cue=cues[selected];
  const sequence=cue?.evidenceSequences[Math.min(evidencePosition,(cue?.evidenceSequences.length??1)-1)];
  const busy=!!id&&(jobs.has(id)||current?.state==="processing"||current?.state==="queued");
  const changed=!!document&&!!current?.revision&&document.revision!==current.revision;
  const refresh=useCallback(async()=>{
    try {setSessions(await api.listOcrSessions());} catch(e) {setError(message(e));}
  },[]);
  useEffect(()=>{if(!active)return;void refresh();const timer=setInterval(()=>void refresh(),2500);return()=>clearInterval(timer);},[active,refresh]);
  const open=useCallback(async(next:string)=>{
    const generation=++loadGeneration.current;
    setId(next);setDocument(null);setCues([]);setPast([]);setFuture([]);setSelected(0);setError("");setNotice("");setCompactNotice("");setLoading(true);
    setSelection([]);selectionAnchor.current=null;
    try {
      if(sessionsRef.current.find(s=>s.sessionId===next)?.revision===null)return;
      const doc=await api.loadOcrSession(next);
      if(loadGeneration.current===generation){setDocument(doc);setCues(doc.cues);setSelection(doc.cues.length?[0]:[]);selectionAnchor.current=doc.cues.length?0:null;}
    } catch(e) {if(loadGeneration.current===generation)setError(message(e));}
    finally {if(loadGeneration.current===generation)setLoading(false);}
  },[]);
  // Worker commits become visible without replacing an unsaved review draft.
  useEffect(()=>{
    const key=`${id}:${current?.revision}`;
    if(id&&!document&&!loading&&current?.revision&&autoLoadKey.current!==key){autoLoadKey.current=key;void open(id);}
  },[id,document,loading,current?.revision,open]);
  useEffect(()=>{if(selected>=cues.length)setSelected(Math.max(0,cues.length-1));},[selected,cues.length]);
  useEffect(()=>{
    const guard=(event:BeforeUnloadEvent)=>{if(dirtyRef.current){event.preventDefault();event.returnValue="";}};
    window.addEventListener("beforeunload",guard);return()=>window.removeEventListener("beforeunload",guard);
  },[]);
  useEffect(()=>{setEvidencePosition(0);},[selected,id]);
  useEffect(()=>{
    let stale=false;setEvidence(null);setEvidenceError("");setCandidates([]);
    if(id&&sequence)void api.ocrEvidence(id,sequence).then(value=>{if(!stale){setEvidence(value);setCandidates(value.observation.candidates);}}).catch(e=>{if(!stale)setEvidenceError(message(e));});
    return()=>{stale=true;};
  },[id,sequence,document?.revision]);
  const choose=(next:string)=>{if(dirty)setPending(next);else void open(next);};
  const resetSelection=(count:number,focus=selected)=>{
    const position=Math.max(0,Math.min(focus,count-1));
    setSelected(position);setSelection(count?[position]:[]);selectionAnchor.current=count?position:null;
  };
  const change=(next:OcrCue[],focus=selected,keepSelection=false)=>{setPast(p=>[...p.slice(-49),cues]);setFuture([]);setCues(next);setNotice("");setCompactNotice("");if(!keepSelection)resetSelection(next.length,focus);};
  const compact=()=>{
    const result=compactCues(cues);
    if(result.removedCount){change(result.cues,result.positions[selected]??0);setEvidencePosition(0);}
    setCompactNotice(result.removedCount?`Đã rút gọn ${cues.length} còn ${result.cues.length} câu (gộp ${result.removedCount} câu trùng). Kiểm tra lại và nhấn Lưu; có thể Hoàn tác.`:"Không có câu trùng hoặc gần giống liền nhau để rút gọn.");
  };
  const patch=(values:Partial<OcrCue>)=>change(cues.map((c,i)=>i===selected?{...c,...values}:c),selected,true);
  const mergeSelection=()=>{
    try {change(mergeSelectedCues(cues,selection),Math.min(...selection));setError("");setEvidencePosition(0);}
    catch(e){setError(message(e));}
  };
  const persist=async()=>{
    if(!id||!document||saving)return;setSaving(true);setError("");
    try {const doc=await api.saveOcrReview(id,document.revision,cues);setDocument(doc);setCues(doc.cues);resetSelection(doc.cues.length,0);setPast([]);setFuture([]);setNotice("Đã lưu bản sửa");setCompactNotice("");await refresh();}
    catch(e){setError(message(e));}finally{setSaving(false);}
  };
  const process=async(reprocess=false)=>{
    if(!id)return;const target=id;setConfirmJob(false);setError("");setJobs(prev=>new Set(prev).add(target));
    try {await api.processOcrSession(target,reprocess);setNotice("Đã lưu ảnh và kết quả OCR");}
    catch(e){setError(message(e));}finally{setJobs(prev=>{const next=new Set(prev);next.delete(target);return next;});await refresh();}
  };
  const exporting=async(format:"srt"|"txt")=>{
    if(!id||!document||dirty)return;
    try {const path=await save({defaultPath:`subtitles.vi.${format}`,filters:[{name:format.toUpperCase(),extensions:[format]}]});
      if(path){await api.exportOcrRevision(id,document.revision,format,path);setNotice(`Đã xuất ${format.toUpperCase()}`);}
    }catch(e){setError(message(e));}
  };
  const recheck=async()=>{
    if(!id||!sequence)return;const targetId=id,targetSequence=sequence;setRechecking(true);
    try {const values=await api.recheckOcrEvidence(targetId,targetSequence);if(evidenceKey.current===`${targetId}:${targetSequence}`)setCandidates(values);}catch(e){setError(message(e));}finally{setRechecking(false);}
  };
  const splitSelected=()=>{
    if(!cue)return;const caret=textArea.current?.selectionStart??0;
    const at=caret>0&&caret<cue.text.length?caret:cue.text.indexOf(" ",Math.floor(cue.text.length/2));
    setSplit({time:Math.floor((cue.startMs+cue.endMs)/2),left:cue.text.slice(0,Math.max(0,at)),right:cue.text.slice(Math.max(0,at))});
  };
  const visible=cues.map((c,i)=>({c,i})).filter(({c})=>(filter==="all"||filter==="warnings"&&!c.reviewed&&c.warnings.length>0||filter==="unreviewed"&&!c.reviewed||filter==="reviewed"&&c.reviewed)&&c.text.toLocaleLowerCase("vi").includes(query.toLocaleLowerCase("vi")));
  const visibleIndices=visible.map(({i})=>i),visibleKey=visibleIndices.join(",");
  useEffect(()=>{
    // Editing text/review status can hide a row in the current filter too.
    // Hidden rows must not remain targets of a later bulk action.
    const indices=visibleKey?visibleKey.split(",").map(Number):[];
    setSelection(previous=>previous.filter(i=>indices.includes(i)));
    if(selectionAnchor.current!==null&&!indices.includes(selectionAnchor.current))selectionAnchor.current=null;
  },[visibleKey]);
  const selectCue=(index:number,extend=false)=>{
    setSelected(index);
    setSelection(extend?selectCueRange(visibleIndices,selectionAnchor.current,index):[index]);
    if(!extend||selectionAnchor.current===null)selectionAnchor.current=index;
  };
  const clearSelection=()=>{setSelection([]);selectionAnchor.current=null;};
  const orderedSelection=[...selection].sort((a,b)=>a-b);
  const mergeIssue=selection.length<2?"Chọn ít nhất hai câu để gộp.":orderedSelection.some((i,n)=>n>0&&i!==orderedSelection[n-1]+1)?"Các câu đang chọn không liên tiếp. Bỏ bộ lọc rồi chọn lại để gộp.":selection.some(i=>cues[i]?.segment!==cues[selection[0]]?.segment)?"Không gộp các câu ở những đoạn thu khác nhau.":"";
  const editBlocked=saving||loading||busy||audioBusy;
  return <section className="ocr-workspace">
    <aside className="ocr-library">
      <header><h2>Bản thu <small>{sessions.length}</small></h2><Icon title="Làm mới thư viện" onClick={()=>void refresh()}><RefreshCw size={15}/></Icon></header>
      <div className="ocr-session-list">{sessions.length===0?<div className="ocr-empty"><ScanText size={28}/><p>Chưa có bản thu</p></div>:sessions.map(s=><button disabled={saving||audioBusy} key={s.sessionId} className={`ocr-session ${id===s.sessionId?"active":""}`} onClick={()=>choose(s.sessionId)}>
        <strong>{s.title}</strong><time>{s.createdAt?new Date(s.createdAt).toLocaleString("vi-VN"):s.sessionId.slice(0,8)}</time>
        <span className={s.state==="error"?"ocr-danger":""}>{ocrState[s.state]??s.state}</span>
        <small>{s.cueCount?`${s.cueCount} câu · ${s.reviewedCount} đã kiểm tra`:`${s.sampleCount} ảnh`}{s.edited?" · Bản sửa":""}</small>
        {s.state==="processing"&&<progress value={s.processedSamples} max={Math.max(1,s.totalSamples)}/>}
      </button>)}</div>
    </aside>
    <div className="ocr-detail">
      {!id?<div className="ocr-empty"><ScanText size={32}/><h2>Thư viện OCR</h2><p>{sessions.length} bản thu</p></div>:<>
        <header className="ocr-detail-heading"><div><h2>{current?.title??id}</h2><small title={current?.url}>{current?.url}</small></div>
          <Icon title="Mở thư mục bản thu" onClick={()=>void api.openOcrFolder(id).catch(e=>setError(message(e)))}><FolderOpen size={17}/></Icon>
        </header>
        <div className="ocr-toolbar"><span className="ocr-status">{ocrState[current?.state??""]??"Đang đọc"}{document?.edited?" · Bản sửa":""}</span><span className="ocr-status">{dirty?"Chưa lưu":notice}</span>
          <div className="ocr-actions">
            <Icon title="Tải lại bản lưu" disabled={loading||saving||audioBusy} onClick={()=>choose(id)}><RefreshCw size={16}/></Icon>
            {busy?<button className="button secondary" onClick={()=>void api.cancelOcrSession(id).then(()=>setNotice("Đang yêu cầu dừng OCR")).catch(e=>setError(message(e)))}><Square size={14}/>Dừng OCR</button>:<button className="button secondary" disabled={current?.captureState!=="captured"||saving||audioBusy} onClick={()=>document?setConfirmJob(true):void process()}><ScanText size={15}/>{document?"OCR lại":"Chạy OCR"}</button>}
            <Icon title="Hoàn tác" disabled={!past.length||saving} onClick={()=>{const previous=past[past.length-1];setFuture(f=>[cues,...f]);setCues(previous);resetSelection(previous.length);setPast(p=>p.slice(0,-1));setCompactNotice("");}}><Undo2 size={16}/></Icon>
            <Icon title="Làm lại" disabled={!future.length||saving} onClick={()=>{setPast(p=>[...p,cues]);setCues(future[0]);resetSelection(future[0].length);setFuture(f=>f.slice(1));setCompactNotice("");}}><Redo2 size={16}/></Icon>
            <button className="button primary" disabled={!dirty||saving||audioBusy||cues.some(invalidCue)} onClick={()=>void persist()}><Save size={15}/>{saving?"Đang lưu":"Lưu"}</button>
            <Icon title="Xuất SRT đã lưu" disabled={!document||dirty||saving} onClick={()=>void exporting("srt")}><Download size={16}/></Icon>
            <button className="button secondary" disabled={!document||dirty||saving} onClick={()=>void exporting("txt")}>TXT</button>
          </div>
        </div>
        {busy&&<div className="ocr-progress"><progress value={current?.processedSamples??0} max={Math.max(1,current?.totalSamples??1)}/><small>{current?.processedSamples??0} / {current?.totalSamples??0} ảnh</small></div>}
        {(error||current?.error)&&<div className="inline-error" role="alert">{error||current?.error}</div>}
        {(changed||document?.newerOcrAvailable)&&<div className="ocr-warning"><AlertTriangle size={15}/>Có revision mới; bản nháp hiện tại được giữ nguyên.</div>}
        {loading?<div className="ocr-empty">Đang đọc bản lưu...</div>:!document?<div className="ocr-empty"><ScanText size={32}/><h2>{busy?"Đang xử lý":"Chưa có SRT đã lưu"}</h2><p>{current?.sampleCount??0} ảnh đã thu</p></div>:<>
          <div className="ocr-summary"><span>{cues.length} câu</span><span>{cues.filter(c=>c.reviewed).length} đã kiểm tra</span><span>{cues.filter(c=>!c.reviewed&&c.warnings.length>0).length} nghi vấn</span><span className="ocr-warning-text">Chưa xác minh cả tập</span></div>
          <details className="ocr-coverage"><summary>Vùng đã thu · {document.coverage.ranges.length} đoạn · {document.coverage.gaps.length} gián đoạn</summary><div>{document.coverage.ranges.map((r,i)=><span key={i}>S{r.segment}: {timestamp(r.startMs)} → {timestamp(r.endMs)}</span>)}</div></details>
          <div className="ocr-mode-bar"><div className="ocr-tabs" role="tablist">{[["table","Câu"],["srt","SRT"],["original","OCR gốc"],["audio","WAV"]].map(([value,label])=><button role="tab" aria-selected={mode===value} key={value} className={mode===value?"active":""} onClick={()=>setMode(value)}>{label}</button>)}</div>
            {mode==="table"&&<button type="button" className="button ocr-compact-button" disabled={cues.length<2||saving||loading||busy||audioBusy} title="Gộp các câu trùng hoặc gần giống liền nhau trong toàn bản thu, cùng đoạn và cách nhau tối đa 1 giây; giữ thời gian sớm nhất → muộn nhất." onClick={compact}><Merge size={15} aria-hidden="true"/>Rút gọn câu</button>}
            {mode!=="audio"&&<><label className="ocr-search"><Search size={14}/><input aria-label="Tìm câu" placeholder="Tìm câu" value={query} onChange={e=>{setQuery(e.target.value);clearSelection();}}/></label>
            <select aria-label="Lọc câu" value={filter} onChange={e=>{setFilter(e.target.value);clearSelection();}}><option value="all">Tất cả</option><option value="warnings">Nghi vấn</option><option value="unreviewed">Chưa kiểm tra</option><option value="reviewed">Đã kiểm tra</option></select></>}
          </div>
          {mode==="table"&&<div className="ocr-selection-bar" aria-label="Thao tác với câu đã chọn">
            <span role="status">Đã chọn <strong>{selection.length}</strong> câu</span>
            <small>Giữ Shift và bấm câu cuối để chọn một dải.</small>
            <div className="ocr-actions">
              <button type="button" className="button ocr-bulk-button" disabled={editBlocked||!selection.length} onClick={()=>setDeleteTargets([...selection])}><Trash2 size={15} aria-hidden="true"/>Xóa câu đã chọn</button>
              <button type="button" className="button ocr-bulk-button" disabled={editBlocked||!!mergeIssue} title={mergeIssue||"Gộp nội dung và khoảng thời gian của các câu đã chọn; câu trùng chỉ giữ một lần."} onClick={mergeSelection}><Merge size={15} aria-hidden="true"/>Gộp câu đã chọn</button>
            </div>
            {selection.length>1&&mergeIssue&&<small className="ocr-selection-issue">{mergeIssue}</small>}
          </div>}
          {mode==="table"&&compactNotice&&<div className="ocr-compact-notice" role="status">{compactNotice}</div>}
          <div hidden={mode!=="audio"} className="ocr-audio-surface"><OcrAudioPreview key={`${document.sessionId}:${document.revision}`} document={document} active={active&&mode==="audio"} blocked={dirty||changed||saving||busy} onBusyChange={setAudioBusy} onEditCue={index=>{const position=cues.findIndex(c=>c.index===index);if(position>=0){resetSelection(cues.length,position);setFilter("all");setQuery("");setMode("table");}}}/></div>
          {mode==="audio"?null:mode!=="table"?<pre className="ocr-srt">{serializeOcr(mode==="original"?document.originalCues:cues)||"Không có cue"}</pre>:<div className="ocr-edit-grid">
            <div className="ocr-cues" aria-label="Danh sách câu">{visible.length===0?<div className="ocr-empty">Không có câu phù hợp</div>:visible.map(({c,i})=><button key={i} className={`ocr-cue ${selected===i?"active":""} ${selection.includes(i)?"selected":""}`} aria-pressed={selection.includes(i)} onMouseDown={event=>{if(event.shiftKey)event.preventDefault();}} onClick={event=>selectCue(i,event.shiftKey)}><span className="ocr-cue-number">{i+1}{c.reviewed?<Check size={13}/>:c.warnings.length>0?<AlertTriangle size={13}/>:null}</span><div><small>{timestamp(c.startMs)} → {timestamp(c.endMs)}</small><p>{c.text}</p></div></button>)}</div>
            <aside className="ocr-inspector">{cue?<>
              <header><strong>Câu {selected+1}</strong><div className="ocr-actions"><Icon title="Tách câu" disabled={saving} onClick={splitSelected}><Scissors size={15}/></Icon><Icon title="Gộp với câu sau" disabled={saving||!cues[selected+1]||cues[selected+1].segment!==cue.segment} onClick={()=>{try{change([...cues.slice(0,selected),mergeCue(cue,cues[selected+1]),...cues.slice(selected+2)]);}catch(e){setError(message(e));}}}><Merge size={15}/></Icon><Icon title="Xóa câu" disabled={saving} onClick={()=>setDeleteTargets([selected])}><Trash2 size={15}/></Icon></div></header>
              <fieldset disabled={saving}><div className="ocr-time-fields"><label>Bắt đầu (ms)<input type="number" min="0" max="3600000000" step="1" value={cue.startMs} onChange={e=>patch({startMs:Number(e.target.value),reviewed:false})}/></label><label>Kết thúc (ms)<input type="number" min="0" max="3600000000" step="1" value={cue.endMs} onChange={e=>patch({endMs:Number(e.target.value),reviewed:false})}/></label></div>
              <textarea aria-label="Nội dung câu" ref={textArea} value={cue.text} onChange={e=>patch({text:e.target.value,reviewed:false})}/>
              <label className="ocr-reviewed"><input type="checkbox" checked={cue.reviewed} onChange={e=>patch({reviewed:e.target.checked})}/>Đã kiểm tra</label></fieldset>
              {invalidCue(cue)&&<div className="inline-error">Text hoặc thời gian không hợp lệ.</div>}
              <div className="ocr-evidence-heading"><strong>Ảnh đối chiếu</strong><div className="ocr-actions"><Icon title="Ảnh trước" disabled={evidencePosition<=0} onClick={()=>setEvidencePosition(p=>p-1)}><ChevronLeft size={15}/></Icon><small>{Math.min(evidencePosition,cue.evidenceSequences.length-1)+1}/{cue.evidenceSequences.length}</small><Icon title="Ảnh sau" disabled={evidencePosition>=cue.evidenceSequences.length-1} onClick={()=>setEvidencePosition(p=>p+1)}><ChevronRight size={15}/></Icon></div></div>
              {evidence?<><button className="ocr-image-button" title="Phóng to ảnh" aria-label="Phóng to ảnh" onClick={()=>setZoom(true)}><img className="ocr-evidence-image" src={evidence.imageUrl} alt={`Phụ đề trong ảnh ${sequence}`}/><ZoomIn size={17}/></button><small className="ocr-evidence-meta">Ảnh {sequence} · S{evidence.observation.segment} · {timestamp(evidence.observation.timeMs)} · ±{Math.ceil(evidence.observation.uncertaintyMs)} ms · {Math.round(evidence.observation.confidence)}%</small></>:<div className="ocr-evidence-placeholder">{evidenceError||"Đang đọc ảnh..."}</div>}
              <div className="ocr-warnings">{cue.warnings.map(w=><span key={w}>{warningLabel[w]??w}</span>)}</div>
              <header><strong>Kết quả nhận dạng</strong><Icon title="Nhận dạng lại ảnh này" disabled={!evidence||rechecking||saving} onClick={()=>void recheck()}><RotateCcw size={15}/></Icon></header>
              {rechecking?<small>Đang nhận dạng...</small>:candidates.map((c,i)=><button className="ocr-candidate" disabled={saving} key={`${c.variant}-${i}`} onClick={()=>patch({text:c.text,reviewed:false})}><small>{c.variant} · {Math.round(c.confidence)}%</small><span>{c.text||"∅"}</span></button>)}
            </>:<div className="ocr-empty">Không có câu</div>}</aside>
          </div>}
        </>}
      </>}
    </div>
    {(pending||confirmJob||deleteTargets||split||zoom)&&<div className="ocr-modal-shade"><section ref={modalRef} className={`ocr-modal ${zoom?"ocr-zoom":""}`} role="dialog" aria-modal="true" aria-label={zoom?"Ảnh đối chiếu":pending?"Bản nháp chưa lưu":split?"Tách câu":deleteTargets?"Xóa câu":"OCR lại"}>
      {zoom&&evidence?<><header><h2>Ảnh {sequence}</h2><Icon title="Đóng ảnh" onClick={()=>setZoom(false)}><X size={17}/></Icon></header><img src={evidence.imageUrl} alt={`Ảnh đối chiếu ${sequence}`}/></>:
      pending?<><h2>Bản nháp chưa lưu</h2><div className="button-row"><button className="button secondary" onClick={()=>setPending(null)}>Giữ bản nháp</button><button className="button primary" disabled={saving} onClick={()=>{const next=pending;setPending(null);void open(next);}}>Bỏ bản nháp</button></div></>:confirmJob?<><h2>OCR lại toàn bộ bản thu?</h2><p>Giữ nguyên OCR gốc và mọi revision đã chỉnh sửa.</p><div className="button-row"><button className="button secondary" onClick={()=>setConfirmJob(false)}>Hủy</button><button className="button primary" onClick={()=>void process(true)}>OCR lại</button></div></>:deleteTargets?<><h2>{deleteTargets.length>1?`Xóa ${deleteTargets.length} câu đã chọn?`:`Xóa câu ${deleteTargets[0]+1}?`}</h2><p>{deleteTargets.length>1?`Các câu đã chọn sẽ bị xóa khỏi bản nháp. Bạn có thể Hoàn tác trước khi Lưu.`:cues[deleteTargets[0]]?.text}</p><div className="button-row"><button className="button secondary" onClick={()=>setDeleteTargets(null)}>Hủy</button><button className="button ocr-bulk-button" disabled={saving} onClick={()=>{change(cues.filter((_,i)=>!deleteTargets.includes(i)),Math.min(...deleteTargets));setDeleteTargets(null);}}>Xóa</button></div></>:split&&cue?<><h2>Tách câu {selected+1}</h2><label>Điểm tách (ms)<input type="number" value={split.time} onChange={e=>setSplit({...split,time:Number(e.target.value)})}/></label><textarea aria-label="Nửa đầu" value={split.left} onChange={e=>setSplit({...split,left:e.target.value})}/><textarea aria-label="Nửa sau" value={split.right} onChange={e=>setSplit({...split,right:e.target.value})}/><div className="button-row"><button className="button secondary" onClick={()=>setSplit(null)}>Hủy</button><button className="button primary" disabled={!split.left.trim()||!split.right.trim()||split.time<=cue.startMs||split.time>=cue.endMs} onClick={()=>{try{change([...cues.slice(0,selected),...splitCue(cue,split.time,split.left,split.right),...cues.slice(selected+1)]);setSplit(null);}catch(e){setError(message(e));}}}>Tách</button></div></>:null}
    </section></div>}
  </section>;
}
