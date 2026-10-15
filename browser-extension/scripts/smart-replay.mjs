import { build } from "esbuild";
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { readFile, writeFile, mkdir, mkdtemp } from "node:fs/promises";
import { randomUUID } from "node:crypto";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const source=path.resolve(process.argv[2]??"");if(!process.argv[2])throw Error("Usage: npm run ocr:smart:replay -- <session-directory> [output-directory]");
const output=process.argv[3]?path.resolve(process.argv[3]):await mkdtemp(path.join(tmpdir(),"long-tieng-smart-replay-"));await mkdir(output,{recursive:true});
const root=fileURLToPath(new URL("../../",import.meta.url));const runtime=path.join(output,"smart-runtime.mjs");
await build({entryPoints:[path.join(root,"browser-extension/src/smart-capture.ts")],bundle:true,platform:"node",format:"esm",outfile:runtime});
const {glyphSignature,SmartDetector}=await import(pathToFileURL(runtime));const detector=new SmartDetector();
const original=JSON.parse(await readFile(path.join(source,"session.json"),"utf8"));if(original.schemaVersion!==1)throw Error("Replay expects a legacy raw capture.");
const id=randomUUID();const session={...original,schemaVersion:2,captureMode:"smart",id,status:"recording",endedAt:null,sampleCount:0,imageBytes:0,observationCount:0,fallbackCount:0};delete session.nativeStore;
let previous=null,tail=null,lastSaved=-1,currentImage=0,baselineBytes=0,fallbacks=0;const images=[],observations=[];
function store(frame,decision){const metadata={...frame.metadata,sessionId:id,sequence:images.length+1,smart:{...decision,save:true}};images.push({sourceSequence:frame.metadata.sequence,imageBytes:frame.imageBytes,metadata});lastSaved=frame.metadata.sequence;currentImage=metadata.sequence;}
function close(){if(tail&&lastSaved!==tail.frame.metadata.sequence)store(tail.frame,{...tail.decision,role:"end"});tail=null;detector.reset();}
const started=performance.now();let peakRss=process.memoryUsage().rss;
const child=spawn("cargo",["run","--quiet","--release","--example","smart_pixels","--",source],{cwd:path.join(root,"browser-extension/native-host"),stdio:["ignore","pipe","inherit"]});
const done=new Promise((resolve,reject)=>{child.on("error",reject);child.on("exit",code=>code===0?resolve():reject(Error(`Decoder exit ${code}`)));});
try{
  for await(const line of createInterface({input:child.stdout,crlfDelay:Infinity})){
    const frame=JSON.parse(line);baselineBytes+=frame.imageBytes;const metadata=frame.metadata;
    if(metadata.manual){close();store(frame,{group:0,stable:false,save:true,role:"fallback"});continue;}
    if(previous&&(metadata.segment!==previous.segment||metadata.estimatedVideoTimeMs-previous.estimatedVideoTimeMs>600||JSON.stringify(metadata.region)!==JSON.stringify(previous.region)))close();
    const bytes=Buffer.from(frame.rgba,"base64");const decision=detector.observe(glyphSignature(new Uint8ClampedArray(bytes.buffer,bytes.byteOffset,bytes.byteLength),frame.width,frame.height),metadata.estimatedVideoTimeMs);
    delete frame.rgba;
    if(tail&&decision.group!==tail.decision.group){if(lastSaved!==tail.frame.metadata.sequence)store(tail.frame,{...tail.decision,role:"end"});tail=null;}
    if(decision.save)store(frame,decision);const {sequence:_sequence,...observation}=metadata;
    observations.push({...observation,sessionId:id,observationSeq:observations.length+1,imageSequence:currentImage,smart:decision});if(decision.role==="fallback")fallbacks++;
    tail={frame,decision};previous=metadata;peakRss=Math.max(peakRss,process.memoryUsage().rss);
  }
  await done;close();
}catch(failure){child.kill("SIGTERM");await done.catch(()=>{});throw failure;}
const bytes=images.reduce((n,i)=>n+i.imageBytes,0);const finalize={...session,status:"stopped",endedAt:original.endedAt??Date.now(),sampleCount:images.length,imageBytes:bytes,observationCount:observations.length,fallbackCount:fallbacks};
const report={source,output,sessionId:id,baselineImages:original.sampleCount,retainedImages:images.length,baselinePngBytes:baselineBytes,retainedPngBytes:bytes,pngReductionPercent:100*(1-bytes/baselineBytes),fallbackObservations:fallbacks,observationCount:observations.length,replayMs:performance.now()-started,nodePeakRssBytes:peakRss,defaultMode:"all",qualityGate:"pending OCR and manual ground truth comparison; replay is not Chrome acceptance"};
await writeFile(path.join(output,"selection.json"),JSON.stringify({session,images,observations,finalize}));await writeFile(path.join(output,"report.json"),JSON.stringify(report,null,2));console.log(JSON.stringify(report,null,2));
