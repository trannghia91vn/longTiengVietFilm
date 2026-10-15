import assert from "node:assert/strict";
import {readFile,readdir,mkdir,writeFile,copyFile} from "node:fs/promises";
import {spawnSync} from "node:child_process";
import {createHash} from "node:crypto";
import path from "node:path";
import {fileURLToPath} from "node:url";

const repo=fileURLToPath(new URL("../",import.meta.url));
const data=process.argv[2];if(!data)throw Error("Pass the isolated QA app-data directory.");
const ids=await readdir(path.join(data,"audio-projects"));const id=ids.find(s=>/^[a-f0-9]{64}$/.test(s));assert.ok(id);
const directory=path.join(data,"audio-projects",id);
const p=JSON.parse(await readFile(path.join(directory,"project.json"),"utf8"));
const full=process.argv.includes("--full");
if(full)p.lastPreview=p.lastFull;
assert.equal(p.lastJob.status,"completed");assert.ok(p.lastPreview);assert.equal(p.lastPreview.durationMs,p.lastPreview.settings.startMs+p.lastPreview.settings.durationMs);
const artifact=path.join(directory,"previews",p.lastPreview.key,"preview.wav");
const hash=b=>createHash("sha256").update(b).digest("hex");
assert.equal(hash(await readFile(artifact)),p.lastPreview.wavHash);
assert.equal(hash(await readFile(path.join(directory,"source.json"))),p.sourceHash);
const report=JSON.parse(await readFile(path.join(directory,"previews",p.lastPreview.key,"report.json"),"utf8"));assert.equal(report.wavHash,p.lastPreview.wavHash);assert.equal(report.anchorMs,0);assert.equal(report.wholeEpisodeVerified,false);
if(full){assert.equal(report.sampleRate,48000);assert.equal(report.offsetMs,0);assert.equal(report.srtHash,hash(await readFile(path.join(directory,"previews",p.lastPreview.key,"subtitles.vi.srt"))));assert.equal(report.allCuesReviewed,true);}
const ffprobe=path.join(repo,"src-tauri/engines/ffprobe"),ffmpeg=path.join(repo,"src-tauri/engines/ffmpeg");
const probe=spawnSync(ffprobe,["-v","error","-show_streams","-show_format","-of","json",artifact],{encoding:"utf8"});assert.equal(probe.status,0,probe.stderr);const media=JSON.parse(probe.stdout);assert.equal(media.streams[0].codec_name,"pcm_s16le");assert.equal(media.streams[0].sample_rate,"48000");assert.equal(media.streams[0].channels,1);assert.equal(Number(media.format.duration)*1000,p.lastPreview.durationMs);
const pcm=(file)=>{const result=spawnSync(ffmpeg,["-v","error","-i",file,"-f","s16le","-acodec","pcm_s16le","-"],{maxBuffer:800*1024*1024});assert.equal(result.status,0,result.stderr?.toString());return result.stdout;};
const output=pcm(artifact);let last=0;const metrics=[];
for(const metric of report.metrics){
  const entries=await readdir(path.join(directory,"cache"));let cached;
  for(const key of entries){if(!/^[a-f0-9]{64}$/.test(key))continue;const item=JSON.parse(await readFile(path.join(directory,"cache",key,"cache.json"),"utf8"));if(item.metric.index===metric.index&&item.metric.audioDurationMs===metric.audioDurationMs){const candidate=path.join(directory,"cache",key,"processed.wav");const raw=await readFile(candidate);const decoded=pcm(candidate);const start=metric.startMs*48*2;if(hash(raw)===item.processedHash&&output.subarray(start,start+decoded.length).equals(decoded)){cached=candidate;break;}}}
  assert.ok(cached);const speech=pcm(cached);const start=metric.startMs*48*2;assert.ok(output.subarray(last,start).every(v=>v===0),"Gap is not silence");assert.deepEqual(output.subarray(start,start+speech.length),speech,"Full cue samples must be preserved at the original timestamp");assert.ok(speech.some(v=>v!==0),"Speech is blank");assert.ok(speech.length<=((metric.endMs-metric.startMs)*48*2),"Speech exceeds cue");last=start+speech.length;metrics.push({...metric,allSamplesPreserved:true});
}
assert.ok(output.subarray(last).every(v=>v===0));
const result={projectId:p.id,sourceRevision:p.sourceRevision,anchorMs:0,wavDurationMs:p.lastPreview.durationMs,settings:p.lastPreview.settings,partial:report.partial,generated:p.lastJob.generated,reused:p.lastJob.reused,elapsedSeconds:p.lastJob.elapsedSeconds,codec:media.streams[0].codec_name,rate:48000,channels:1,leadingAndGapSilenceVerified:true,metrics,wavHash:p.lastPreview.wavHash};
const evidence=path.join(repo,"browser-extension/tests/results/",full?"ocr-audio-full":"ocr-audio");await mkdir(evidence,{recursive:true});await copyFile(artifact,path.join(evidence,full?"narration.vi.wav":"trial.wav"));await writeFile(path.join(evidence,"report.json"),JSON.stringify(result,null,2));
console.log(JSON.stringify(result,null,2));
