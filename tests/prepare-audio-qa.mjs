// Synthetic 4A transport fixture, not evidence of browser capture or OCR accuracy.
import {mkdtemp,mkdir,writeFile,access,readFile} from "node:fs/promises";
import {spawnSync} from "node:child_process";
import {createHash,randomUUID} from "node:crypto";
import os from "node:os";
import path from "node:path";
import {fileURLToPath} from "node:url";

const repo=fileURLToPath(new URL("../",import.meta.url));
const bundledFfmpeg=path.join(repo,"src-tauri/engines/ffmpeg");
const ffmpeg=await access(bundledFfmpeg).then(()=>bundledFfmpeg).catch(()=>"ffmpeg");
const overflow=process.argv.includes("--overflow");
const full=process.argv.includes("--full")||overflow;
const data=await mkdtemp(path.join(os.tmpdir(),full?"long-tieng-m4b-qa-":"long-tieng-m4a-qa-"));
const id=randomUUID(),revision=createHash("sha256").update(`audio-qa:${id}`).digest("hex"),observationsRevision="b".repeat(64);
const session=path.join(data,"ocr-sessions",id),output=path.join(session,"ocr-revisions",revision),work=path.join(session,"ocr-work",observationsRevision),frames=path.join(session,"frames");
for(const folder of [output,work,frames])await mkdir(folder,{recursive:true});
let cues=[
  {index:1,startMs:31000,endMs:36000,text:"Xin chào, đây là phụ đề tiếng Việt.",confidence:100,segment:0,evidenceSequences:[1],startRangeMs:[31000,31000],endRangeMs:[36000,36000],warnings:[],reviewed:true},
  {index:2,startMs:45000,endMs:50000,text:"Kết thúc đoạn kiểm thử.",confidence:100,segment:0,evidenceSequences:[2],startRangeMs:[45000,45000],endRangeMs:[50000,50000],warnings:[],reviewed:true},
];
if(full&&!overflow)cues=Array.from({length:80},(_,i)=>({...cues[i%2],index:i+1,startMs:3000+i*17000,endMs:9000+i*17000,evidenceSequences:[i%2+1],startRangeMs:[3000+i*17000,3000+i*17000],endRangeMs:[9000+i*17000,9000+i*17000]}));
if(overflow)cues=cues.map(c=>({...c,endMs:c.startMs+180,endRangeMs:[c.startMs+180,c.startMs+180]}));
const ts=ms=>`${String(Math.floor(ms/3600000)).padStart(2,"0")}:${String(Math.floor(ms/60000)%60).padStart(2,"0")}:${String(Math.floor(ms/1000)%60).padStart(2,"0")},${String(ms%1000).padStart(3,"0")}`;
const srt=cues.map(c=>`${c.index}\n${ts(c.startMs)} --> ${ts(c.endMs)}\n${c.text}\n`).join("\n");
const duration=full&&!overflow?1422000:120000;
const coverage={durationMs:duration,ranges:[{startMs:0,endMs:duration,segment:0}],gaps:[],wholeEpisodeVerified:false};
const contents={"subtitles.vi.srt":srt,"subtitles.vi.txt":cues.map(c=>c.text).join("\n\n"),"cues.json":JSON.stringify({schemaVersion:1,sessionId:id,revision,cues,coverage},null,2)};
const hash=b=>createHash("sha256").update(b).digest("hex");
for(const [name,content]of Object.entries(contents)){await writeFile(path.join(output,name),content);await writeFile(path.join(session,name),content);}
await writeFile(path.join(session,"session.json"),JSON.stringify({schemaVersion:1,id,title:overflow?"Mốc 4B QA · Câu vượt thời gian":full?"Mốc 4B QA · 80 câu · 23 phút 42 giây":"Mốc 4A QA · WAV 30 giây",url:"qa://ocr-audio-trial",createdAt:Date.now(),sampleCount:2,nativeStore:{status:"captured"}},null,2));
await writeFile(path.join(session,"journal.ndjson"),JSON.stringify({qaOnly:true})+"\n");
await writeFile(path.join(session,"ocr.json"),JSON.stringify({schemaVersion:1,sessionId:id,revision,observationRevision:observationsRevision,committed:true,state:"partial",srtHash:hash(srt),txtHash:hash(contents["subtitles.vi.txt"]),cuesHash:hash(contents["cues.json"])},null,2));
const rows=[];
for(const [i,time] of [1.5,18].entries()){
  const file=path.join(frames,`${String(i+1).padStart(8,"0")}.png`);
  const result=spawnSync(ffmpeg,["-y","-loglevel","error","-ss",String(time),"-i",path.join(repo,"browser-extension/tests/media/clock.webm"),"-frames:v","1","-vf","crop=960:110:0:400",file],{encoding:"utf8"});
  if(result.status!==0)throw Error(result.error?.message??result.stderr??"FFmpeg failed");
  const observation={sequence:i+1,segment:0,timeMs:cues[i].startMs,capturedEpochMs:Date.now(),uncertaintyMs:0,manual:false,imageHash:hash(await readFile(file)),text:cues[i].text,confidence:100,candidates:[{text:cues[i].text,confidence:100,variant:"qa-reviewed"}]};
  rows.push(JSON.stringify({observation,checksum:hash(JSON.stringify(observation))}));
}
await writeFile(path.join(work,"observations.ndjson"),rows.join("\n")+"\n");
console.log(JSON.stringify({data,sessionId:id,revision,trialStartMs:30000,trialDurationMs:30000,fullDurationMs:duration},null,2));
