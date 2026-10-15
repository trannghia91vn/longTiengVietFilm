import { readFile,writeFile,mkdir,copyFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
const output=path.resolve(process.argv[2]??"");if(!process.argv[2])throw Error("Usage: node browser-extension/scripts/smart-qa.mjs <replay-output>");
const root=fileURLToPath(new URL("../../",import.meta.url));const report=JSON.parse(await readFile(path.join(output,"report.json"),"utf8"));
const home=path.join(output,"home"),data=path.join(home,"Library/Application Support/com.basushi.longtieng"),host=path.join(output,"host");
await mkdir(path.join(data,"models/ocr-vie-fast"),{recursive:true});await mkdir(host,{recursive:true});
await copyFile(path.join(root,"browser-extension/native-host/runtime/vie.traineddata"),path.join(data,"models/ocr-vie-fast/vie.traineddata"));
for(const [source,destination] of [["browser-extension/native-host/target/release/long-tieng-capture-host","long-tieng-capture-host"],["src-tauri/engines/vision-ocr","vision-ocr"],["src-tauri/engines/tesseract","tesseract"]])await copyFile(path.join(root,source),path.join(host,destination));
function run(binary,args,env=process.env){const p=spawnSync(binary,args,{env,encoding:"utf8",timeout:1200000,maxBuffer:8*1024*1024});if(p.status!==0)throw Error(p.stderr||p.error||`Exit ${p.status}`);return p.stdout;}
run(path.join(root,"browser-extension/native-host/target/release/examples/smart_import"),[report.source,path.join(data,"ocr-sessions"),path.join(output,"selection.json")]);
const start=performance.now();run(path.join(host,"long-tieng-capture-host"),["--ocr-process",report.sessionId],{...process.env,HOME:home});
const directory=path.join(data,"ocr-sessions",report.sessionId),ocr=JSON.parse(await readFile(path.join(directory,"ocr.json"),"utf8")),smart=JSON.parse(await readFile(path.join(directory,"cues.json"),"utf8")),baseline=JSON.parse(await readFile(path.join(report.source,"cues.json"),"utf8"));
const normalize=s=>s.normalize("NFC").replace(/\s+/g," ").trim();const used=new Set(),deltas=[],missing=[];
for(const cue of baseline.cues){const matches=smart.cues.map((c,i)=>({c,i})).filter(({c,i})=>!used.has(i)&&normalize(c.text)===normalize(cue.text)&&c.startMs<cue.endMs&&c.endMs>cue.startMs);matches.sort((a,b)=>Math.abs(a.c.startMs-cue.startMs)-Math.abs(b.c.startMs-cue.startMs));const best=matches[0];if(!best){missing.push(cue.index);continue;}used.add(best.i);deltas.push(Math.abs(best.c.startMs-cue.startMs),Math.abs(best.c.endMs-cue.endMs));}
deltas.sort((a,b)=>a-b);
Object.assign(report,{directory,ocrMs:performance.now()-start,recognitionCalls:ocr.recognitionCallsThisRun,recognitionReductionPercent:100*(1-ocr.recognitionCallsThisRun/report.baselineImages),baselineCueCount:baseline.cues.length,smartCueCount:smart.cues.length,exactMatchedBaselineCues:used.size,unmatchedBaselineCueIndices:missing,unmatchedSmartCues:smart.cues.length-used.size,boundaryDifferenceP95Ms:deltas.length?deltas[Math.min(deltas.length-1,Math.ceil(deltas.length*.95)-1)]:null,coverage:smart.coverage,qualityGate:"comparison to existing OCR output only; human film ground truth and Chrome acceptance remain required"});
await writeFile(path.join(output,"report.json"),JSON.stringify(report,null,2));console.log(JSON.stringify(report,null,2));
