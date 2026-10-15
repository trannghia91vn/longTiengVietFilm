import { createHash } from "node:crypto";
import { readFile, readdir, writeFile } from "node:fs/promises";
import path from "node:path";
const directory=path.resolve(process.argv[2]??"");if(!process.argv[2])throw Error("Usage: node browser-extension/scripts/smart-audit.mjs <finalized-session> [report-output]");
const digest=bytes=>createHash("sha256").update(bytes).digest("hex");const session=JSON.parse(await readFile(path.join(directory,"session.json"),"utf8"));
if(session.schemaVersion!==2||session.nativeStore?.status!=="captured")throw Error("Audit requires a finalized smart capture.");
const bytes=await readFile(path.join(directory,"journal.ndjson"));if(bytes.at(-1)!==10)throw Error("Finalized journal has incomplete tail.");
let images=0,observations=0,imageBytes=0,fallback=0,last=null,first=null,maxInterval=0;const files=new Set();
const records=bytes.toString("utf8").trim().split("\n");
for(let i=0;i<records.length;i++){
  const record=JSON.parse(records[i]);if(record.messageSeq!==i+1)throw Error("Journal sequence is not contiguous.");let image=Buffer.alloc(0);const payload=JSON.parse(record.payloadJson);
  if(record.operation==="appendSample"){
    if(payload.sequence!==++images)throw Error("Image sequence is not contiguous.");const name=`${String(images).padStart(8,"0")}.png`;files.add(name);image=await readFile(path.join(directory,"frames",name));
    if(image.length!==record.imageBytes||digest(image)!==record.imageHash)throw Error(`Image integrity failure ${name}`);imageBytes+=image.length;
  }
  if(createHash("sha256").update(`${record.operation}\n${session.id}\n${record.messageSeq}\n${record.payloadJson}`).update(image).digest("hex")!==record.checksum)throw Error(`Journal checksum failure ${record.messageSeq}`);
  if(record.operation==="appendObservations")for(const o of payload.observations){if(o.observationSeq!==++observations||o.imageSequence<1||o.imageSequence>images)throw Error("Invalid observation reference/order");first??=o;if(last&&last.segment===o.segment)maxInterval=Math.max(maxInterval,o.estimatedVideoTimeMs-last.estimatedVideoTimeMs);last=o;if(o.smart.role==="fallback")fallback++;}
}
if(JSON.parse(records.at(-1)).operation!=="finalize"||images!==session.sampleCount||observations!==session.observationCount||fallback!==session.fallbackCount||imageBytes!==session.imageBytes||session.nativeStore.ackSequence!==records.length)throw Error("Finalized manifest does not match journal.");
const orphanPng=(await readdir(path.join(directory,"frames"))).filter(name=>name.endsWith(".png")&&!files.has(name));
const report={sessionId:session.id,url:session.url,finalized:true,images,observations,fallbackObservations:fallback,imageBytes,imageMiB:imageBytes/1048576,firstTimeMs:first?.estimatedVideoTimeMs,lastTimeMs:last?.estimatedVideoTimeMs,mediaSpanSeconds:(last?.estimatedVideoTimeMs-first?.estimatedVideoTimeMs)/1000,maxObservedIntervalMs:maxInterval,sourceJournalHash:digest(bytes),orphanPng,events:session.events,wholeEpisodeVerified:false};
if(process.argv[3])await writeFile(path.resolve(process.argv[3]),JSON.stringify(report,null,2));console.log(JSON.stringify(report,null,2));
