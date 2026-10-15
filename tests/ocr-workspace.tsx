// QA-only transport; production never imports this file.
import ReactDOM from "react-dom/client";
import App from "../src/App";
import { api } from "../src/lib/tauri";
import { serializeOcr } from "../src/lib/ocr-editor";
import type { OcrCue, OcrDocument, OcrSession } from "../src/types";
import "../src/styles.css";

const fixture=await fetch("/browser-extension/tests/results/ocr-review/cues.json").then(r=>r.json());
if(new URLSearchParams(location.search).get("scenario")==="tokyo") {
  fixture.cues=await fetch("/tests/fixtures/ocr-compact-tokyo.json").then(r=>r.json());
}
if(new URLSearchParams(location.search).get("scenario")==="compact") {
  const first=fixture.cues[0];
  fixture.cues=[
    {...first,index:1,startMs:1000,endMs:2000,text:"Chúng ta sẽ đến trường học vào ngày mai.",reviewed:true},
    {...first,index:2,startMs:2000,endMs:3500,text:"Chúng ta sẽ đến trường học vào ngày mai.",reviewed:false},
    {...first,index:3,startMs:3500,endMs:5000,text:"Chúng ta sẽ đến truờng học vào ngày mai.",confidence:50,reviewed:false},
    {...first,index:4,startMs:6000,endMs:7500,text:"Được rồi, hẹn gặp lại!",reviewed:true}
  ];
}
const id=fixture.sessionId;
let doc:OcrDocument={sessionId:id,revision:"a".repeat(64),baseRevision:"a".repeat(64),edited:false,newerOcrAvailable:false,cues:fixture.cues,originalCues:fixture.cues,coverage:fixture.coverage,srt:serializeOcr(fixture.cues),text:fixture.cues.map((c:OcrCue)=>c.text).join("\n\n")};
const captureId="9c4212da-a38b-4452-bb9c-ebfb5d55c66c";
const state=new URLSearchParams(location.search).get("state")??"partial";
api.listOcrSessions=async()=>[
  {sessionId:id,title:"Tokyo Revengers · Tập 01",url:"https://animevietsub.nl/phim/tokyo-revengers-a4036/tap-01-75695.html",createdAt:1791963476117,captureState:"captured",sampleCount:2092,state,processedSamples:120,totalSamples:2092,cueCount:doc.cues.length,reviewedCount:doc.cues.filter(c=>c.reviewed).length,warningCount:doc.cues.filter(c=>!c.reviewed&&c.warnings.length>0).length,revision:doc.revision,edited:doc.edited,error:state==="error"?"Engine thử nghiệm bị gián đoạn":null},
  {sessionId:captureId,title:"Bản thu chỉ có ảnh",url:"",createdAt:1791963475000,captureState:"captured",sampleCount:200,state:"not-started",processedSamples:0,totalSamples:200,cueCount:0,reviewedCount:0,warningCount:0,revision:null,edited:false,error:null}
] satisfies OcrSession[];
api.loadOcrSession=async(session)=>{if(session!==id)throw Error("Chưa có kết quả OCR đã lưu.");return structuredClone(doc);};
api.saveOcrReview=async(session,expected,cues)=>{if(session!==id||expected!==doc.revision)throw Error("REVISION_CONFLICT");doc={...doc,edited:true,revision:crypto.randomUUID().replaceAll("-","").repeat(2),cues:structuredClone(cues),srt:serializeOcr(cues)};return structuredClone(doc);};
api.ocrEvidence=async(_session,sequence)=>({imageUrl:"/browser-extension/tests/results/ocr-review/frame.png",observation:{sequence,timeMs:3261,segment:0,uncertaintyMs:74.5,text:fixture.cues[0].text,confidence:95,manual:false,candidates:[{text:fixture.cues[0].text,confidence:95,variant:"vision-accurate-vi-candidate-1"}]}});
api.recheckOcrEvidence=async()=>[{text:fixture.cues[0].text,confidence:95,variant:"vision-accurate-vi-candidate-1"}];
ReactDOM.createRoot(document.getElementById("root")!).render(<App/>);
