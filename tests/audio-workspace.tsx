// QA-only transport. No synthesis or writes to app data occur in this page.
import ReactDOM from "react-dom/client";
import {useState} from "react";
import {OcrAudioPreview} from "../src/components/OcrAudioPreview";
import {api} from "../src/lib/tauri";
import type {OcrAudioProject,OcrDocument} from "../src/types";
import "../src/styles.css";

const fixture=await fetch("/browser-extension/tests/results/ocr-review/cues.json").then(r=>r.json());
const cues=[{...fixture.cues[0],index:1,startMs:31000,endMs:36000,text:"Xin chào, đây là phụ đề tiếng Việt.",reviewed:true},{...fixture.cues[0],index:2,startMs:45000,endMs:50000,text:"Kết thúc đoạn kiểm thử.",reviewed:true}];
const blocked=new URLSearchParams(location.search).get("state")==="blocked";
if(blocked)cues[0].reviewed=false;
const doc:OcrDocument={sessionId:fixture.sessionId,revision:"a".repeat(64),baseRevision:"a".repeat(64),edited:true,newerOcrAvailable:false,cues,originalCues:cues,coverage:{...fixture.coverage,ranges:[{startMs:0,endMs:60000,segment:0}],durationMs:blocked?120000:60000},srt:"",text:""};
let project:OcrAudioProject={schemaVersion:1,id:"b".repeat(64),title:"WAV trial QA",url:"",sessionId:doc.sessionId,sourceRevision:doc.revision,sourceHash:"c".repeat(64),createdAtMs:0,updatedAtMs:0,lastJob:null,lastPreview:{key:"d".repeat(64),path:"/browser-extension/tests/results/ocr-audio/ui.wav",wavHash:"e".repeat(64),reportHash:"f".repeat(64),durationMs:60000,settings:{startMs:30000,durationMs:30000,voice:"thuy_dung"},sourceRevision:doc.revision,renderedCues:2}};
api.createOcrAudioProject=async()=>structuredClone(project);
api.loadOcrAudioProject=async()=>structuredClone(project);
api.getOcrAudioJob=async()=>structuredClone(project.lastJob);
api.newOcrAudioJobId=async()=>crypto.randomUUID();
api.preflightOcrAudioFull=async(_id,settings)=>({issues:blocked?[{code:"unreviewed",cueIndex:1,message:"Câu 1 chưa kiểm tra và lưu."},...(!settings.full?.allowGaps?[{code:"coverage-gap",cueIndex:null,message:"Chưa thu đủ timeline; thu bổ sung hoặc xác nhận xuất Một phần."}]:[])]:[],warnings:blocked?["1 khoảng chưa thu; WAV giữ silence, không khôi phục thoại bị thiếu."]:[],gaps:blocked?[{startMs:60000,endMs:120000}]:[],durationMs:doc.coverage.durationMs,knownDurationMs:doc.coverage.durationMs,cueCount:2,reviewedCount:blocked?1:2,wavBytes:doc.coverage.durationMs*96+44,estimatedGrowthBytes:20000000,projectBytes:12000000,libraryBytes:20000000,freeBytes:30000000000,projectQuotaBytes:2147483648,libraryQuotaBytes:10737418240,partial:blocked});
api.renderOcrAudioPreview=async(_id,jobId,settings)=>{project={...project,lastJob:{id:jobId,kind:settings.full?"ocr-audio-full":"ocr-audio-preview",status:"completed",stage:"completed",message:"Đã lưu WAV",startedAtMs:0,updatedAtMs:0,currentCueIndex:null,error:null,processed:2,total:2,generated:0,reused:2,percent:100,elapsedSeconds:1,etaSeconds:null,settings,metrics:[{index:1,startMs:31000,endMs:36000,rawDurationMs:2080,audioDurationMs:2048,tempo:1,reused:true}],warnings:[]}};if(settings.full)project.lastFull={...project.lastPreview!,settings,durationMs:settings.durationMs,partial:blocked};return structuredClone(project);};
function QaRoot(){const [selected,setSelected]=useState<number|null>(null);return <div style={{padding:16,maxWidth:1100,margin:"0 auto"}}><OcrAudioPreview document={doc} active blocked={false} onBusyChange={()=>{}} onEditCue={setSelected}/>{selected!=null&&<p role="status">Câu QA đã chọn: {selected}</p>}</div>;}
ReactDOM.createRoot(document.getElementById("root")!).render(<QaRoot/>);
