import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import ts from "typescript";

const root=await fs.mkdtemp(path.join(os.tmpdir(),"long-tieng-audio-ui-test-"));
const source=await fs.readFile(new URL("../src/lib/audio-preview.ts",import.meta.url),"utf8");
await fs.writeFile(path.join(root,"preview.mjs"),ts.transpileModule(source,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ES2022}}).outputText);
const {trialIssues,fullSettings,VOICE_PRESETS}=await import(path.join(root,"preview.mjs"));
const settings={startMs:30000,durationMs:30000,voice:"thuy_dung"};
const doc={cues:[{index:1,startMs:31000,endMs:36000,text:"Xin chào",reviewed:true},{index:2,startMs:45000,endMs:50000,text:"Tạm biệt",reviewed:true}],coverage:{durationMs:120000,ranges:[{startMs:0,endMs:120000,segment:0}]}};
test("trial accepts saved reviewed cues without a local video",()=>assert.deepEqual(trialIssues(doc,settings),[]));
test("unreviewed cue blocks generation instead of being skipped",()=>assert.match(trialIssues({...doc,cues:doc.cues.map(c=>({...c,reviewed:false}))},settings).join(" "),/2 câu chưa kiểm tra/));
test("cross-boundary cue cannot be clipped silently",()=>assert.match(trialIssues(doc,{...settings,startMs:32000}).join(" "),/biên đoạn thử/));
test("overlap, coverage gap and unknown voice fail preflight",()=>{
  assert.match(trialIssues({...doc,cues:[doc.cues[0],{...doc.cues[1],startMs:35000}]},settings).join(" "),/chồng lấn/);
  assert.match(trialIssues({...doc,coverage:{...doc.coverage,ranges:[]}},settings).join(" "),/vùng thu/);
  assert.ok(trialIssues(doc,{...settings,voice:"../../escape"}).length);
});
test("numeric limits and 30/60 modes are enforced",()=>{
  for(const change of [{startMs:NaN},{startMs:-1},{durationMs:90000},{startMs:7200000}])assert.ok(trialIssues(doc,{...settings,...change}).length);
  assert.equal(VOICE_PRESETS.length,7);assert.equal(new Set(VOICE_PRESETS.map(v=>v.id)).size,7);
});
test("full mode anchors to zero and never silently shortens known duration",()=>{
  assert.deepEqual(fullSettings(doc,"thai_son"),{startMs:0,durationMs:120000,voice:"thai_son",full:{allowGaps:false}});
  assert.equal(fullSettings({...doc,coverage:{...doc.coverage,durationMs:8000000}},"thuy_dung").durationMs,8000000);
});
test("unknown duration proposes a tail but never approves partial automatically",()=>{
  const value=fullSettings({...doc,coverage:{...doc.coverage,durationMs:0}},"thuy_dung");
  assert.equal(value.durationMs,51000);assert.equal(value.full.allowGaps,false);
});
test.after(async()=>fs.rm(root,{recursive:true,force:true}));
