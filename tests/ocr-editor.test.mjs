import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import ts from "typescript";

const root=await fs.mkdtemp(path.join(os.tmpdir(),"long-tieng-editor-test-"));
const output=ts.transpileModule(await fs.readFile(new URL("../src/lib/ocr-editor.ts",import.meta.url),"utf8"),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ES2022}}).outputText;
await fs.writeFile(path.join(root,"editor.mjs"),output);
const {timestamp,serializeOcr,invalidCue,splitCue,mergeCue,compactCues,selectCueRange,mergeSelectedCues}=await import(path.join(root,"editor.mjs"));
const cue={index:1,startMs:1234,endMs:5678,text:"Xin chào\ncon",confidence:95,segment:0,evidenceSequences:[24,43],startRangeMs:[1200,1250],endRangeMs:[5600,5700],warnings:["short-cue"],reviewed:true};
test("millisecond SRT serialization preserves Vietnamese and multiline",()=>{assert.equal(timestamp(1234),"00:00:01,234");assert.match(serializeOcr([cue]),/Xin chào\ncon/);assert.match(serializeOcr([cue]),/00:00:05,678/);});
test("invalid timings and SRT block injection cannot be saved",()=>{assert.equal(invalidCue(cue),false);for(const edit of [{startMs:-1},{endMs:100},{startMs:NaN},{endMs:Infinity},{text:""},{text:"A\n\nB"},{text:"A\0B"}])assert.equal(invalidCue({...cue,...edit}),true);});
test("split keeps evidence and segment, clears review, does not mutate original",()=>{const [a,b]=splitCue(cue,3000,"Xin chào","con");assert.equal(a.endMs,b.startMs);assert.deepEqual(a.evidenceSequences,cue.evidenceSequences);assert.equal(a.reviewed,false);assert.equal(cue.endMs,5678);assert.throws(()=>splitCue(cue,6000,"a","b"));});
test("merge preserves evidence union and refuses different seek segments",()=>{const second={...cue,startMs:6000,endMs:7000,evidenceSequences:[43,44],text:"Tạm biệt"};const merged=mergeCue(cue,second);assert.deepEqual(merged.evidenceSequences,[24,43,44]);assert.equal(merged.endMs,7000);assert.equal(merged.reviewed,false);assert.throws(()=>mergeCue(cue,{...second,segment:2}));});
test("empty revision exports empty SRT without a fake cue",()=>assert.equal(serializeOcr([]),""));
test("compact joins normalized duplicates, preserves evidence and outer timestamps without mutating input",()=>{
  const first={...cue,startMs:1000,endMs:2000,text:"Xin chào\ncon!",evidenceSequences:[1],startRangeMs:[950,1050],endRangeMs:[1950,2050]};
  const second={...cue,index:2,startMs:2000,endMs:4000,text:"  XIN CHÀO con. ".normalize("NFD"),evidenceSequences:[2,1],warnings:["low-confidence"],startRangeMs:[1950,2050],endRangeMs:[3950,4050],confidence:80};
  const source=structuredClone([first,second]),before=structuredClone(source);
  const result=compactCues(source);
  assert.equal(result.removedCount,1);assert.deepEqual(result.positions,[0,0]);
  assert.deepEqual([result.cues[0].startMs,result.cues[0].endMs],[1000,4000]);
  assert.deepEqual(result.cues[0].startRangeMs,first.startRangeMs);assert.deepEqual(result.cues[0].endRangeMs,second.endRangeMs);
  assert.equal(result.cues[0].text,first.text);assert.equal(result.cues[0].reviewed,false);assert.equal(result.cues[0].confidence,80);
  assert.deepEqual(result.cues[0].evidenceSequences,[1,2]);assert.deepEqual(result.cues[0].warnings,["short-cue","low-confidence"]);
  assert.deepEqual(source,before);assert.match(serializeOcr(result.cues),/00:00:01,000 --> 00:00:04,000\nXin chào\ncon!/);
});
test("compact tolerates a minor OCR error and keeps the best recognized wording",()=>{
  const text="Chúng ta sẽ đến trường học vào ngày mai";
  const result=compactCues([{...cue,text:text.replace("trường","truờng"),confidence:60,reviewed:false},{...cue,text,confidence:98,reviewed:false,startMs:5678,endMs:8000}]);
  assert.equal(result.cues.length,1);assert.equal(result.cues[0].text,text);
  const reviewed=compactCues([{...cue,text,confidence:60},{...cue,text:text.toUpperCase(),confidence:99,reviewed:false}]);
  assert.equal(reviewed.cues[0].text,text);
});
test("compact never loses negation, short changed words, numbers or distinct dialogue",()=>{
  for(const [a,b] of [
    ["Tôi muốn đi về nhà","Tôi không muốn đi về nhà"],
    ["Chúng ta đang đi đến một nơi rất xa","Chúng ta đang đi đến một nơi rất lạ"],
    ["Tôi đã nhận được 12345 đồng vào hôm qua","Tôi đã nhận được 12346 đồng vào hôm qua"],
    ["Xin chào bạn","Tạm biệt bạn"],
    ["Đây là một cuộc hành trình tuyệt vời","Đây là một cuộc sống thật hạnh phúc"],
    ["!!!","..."]
  ])assert.equal(compactCues([{...cue,text:a},{...cue,text:b}]).removedCount,0,`${a} / ${b}`);
});
test("compact respects segment, one-second gap, chronological order and intervening dialogue",()=>{
  for(const edit of [{segment:1},{startMs:cue.endMs+1001,endMs:9000},{startMs:100,endMs:1000},{text:""},{endMs:NaN}])assert.equal(compactCues([cue,{...cue,...edit}]).removedCount,0);
  const edge={...cue,startMs:cue.endMs+1000,endMs:9000};
  assert.equal(compactCues([cue,edge]).removedCount,1);
  assert.equal(compactCues([cue,{...cue,text:"Câu khác"},cue]).removedCount,0);
});
test("compact handles overlap, chains, selection positions and repeated application",()=>{
  const source=[{...cue,startMs:1000,endMs:6000},{...cue,startMs:2000,endMs:3000},{...cue,startMs:3000,endMs:7000},{...cue,startMs:8000,endMs:9000,text:"Kết thúc"}];
  const result=compactCues(source);
  assert.equal(result.removedCount,2);assert.deepEqual(result.positions,[0,0,0,1]);
  assert.deepEqual([result.cues[0].startMs,result.cues[0].endMs],[1000,7000]);
  assert.equal(result.cues[1],source[3]);assert.equal(compactCues(result.cues).removedCount,0);
  assert.deepEqual(compactCues([]),{cues:[],removedCount:0,positions:[]});assert.equal(compactCues([cue]).cues[0],cue);
});
test("Tokyo Revengers real recording: cues 2–19 become one cue at exactly 7211–10423 ms",async()=>{
  const source=JSON.parse(await fs.readFile(new URL("./fixtures/ocr-compact-tokyo.json",import.meta.url),"utf8"));
  const before=structuredClone(source),result=compactCues(source);
  assert.equal(result.cues.length,3);
  assert.equal(result.removedCount,17);
  assert.deepEqual(result.positions,[0,...Array(18).fill(1),2]);
  const merged=result.cues[1];
  assert.deepEqual([merged.startMs,merged.endMs],[7211,10423]);
  assert.deepEqual(merged.evidenceSequences,[...new Set(source.slice(1,19).flatMap(c=>c.evidenceSequences))].sort((a,b)=>a-b));
  assert.equal(merged.text,"duy trì phân bổ áp suất khí quyển\ndễ tạo điều kiện cho luồng khí nóng tràn vào,");
  assert.equal(merged.reviewed,false);
  assert.equal(result.cues[0],source[0]);assert.equal(result.cues[2],source[19]);
  assert.deepEqual(source,before);assert.equal(compactCues(result.cues).removedCount,0);
  assert.match(serializeOcr(result.cues),/00:00:07,211 --> 00:00:10,423/);
});
test("Shift ranges work forward and backward and only include visible cues",()=>{
  const visible=[0,1,2,3,4,5];
  assert.deepEqual(selectCueRange(visible,1,4),[1,2,3,4]);
  assert.deepEqual(selectCueRange(visible,4,1),[1,2,3,4]);
  assert.deepEqual(selectCueRange(visible,1,1),[1]);
  assert.deepEqual(selectCueRange([1,3,5],1,5),[1,3,5]);
  assert.deepEqual(selectCueRange([1,3,5],null,3),[3]);
  assert.deepEqual(selectCueRange([1,3,5],0,3),[3]);
  assert.deepEqual(selectCueRange([1,3,5],1,7),[]);
});
test("manual bulk merge keeps distinct dialogue, complete timing/evidence and untouched surrounding cues",()=>{
  const source=[{...cue,text:"Trước"},{...cue,text:"Xin chào",startMs:6000,endMs:8000,evidenceSequences:[44],startRangeMs:[5900,6100]},{...cue,text:"Tạm biệt",startMs:8000,endMs:9000,evidenceSequences:[45],endRangeMs:[8900,9100]},{...cue,text:"Sau",startMs:10000,endMs:11000}];
  const before=structuredClone(source),result=mergeSelectedCues(source,[2,1,2]);
  assert.equal(result.length,3);assert.equal(result[0],source[0]);assert.equal(result[2],source[3]);
  assert.equal(result[1].text,"Xin chào\nTạm biệt");assert.deepEqual([result[1].startMs,result[1].endMs],[6000,9000]);
  assert.deepEqual(result[1].evidenceSequences,[44,45]);assert.equal(result[1].reviewed,false);
  assert.deepEqual(result[1].startRangeMs,source[1].startRangeMs);assert.deepEqual(result[1].endRangeMs,source[2].endRangeMs);
  assert.deepEqual(source,before);
});
test("manual merge deduplicates the selected real Tokyo cues 2–19 at 7211–10423 ms",async()=>{
  const source=JSON.parse(await fs.readFile(new URL("./fixtures/ocr-compact-tokyo.json",import.meta.url),"utf8"));
  const result=mergeSelectedCues(source,selectCueRange(source.map((_,i)=>i),1,18));
  assert.equal(result.length,3);assert.deepEqual([result[1].startMs,result[1].endMs],[7211,10423]);
  assert.equal(result[1].text,"duy trì phân bổ áp suất khí quyển\ndễ tạo điều kiện cho luồng khí nóng tràn vào,");
  assert.equal(result[1].evidenceSequences.length,24);
  assert.equal(result[0],source[0]);assert.equal(result[2],source[19]);
  const paused=mergeSelectedCues([cue,{...cue,startMs:20000,endMs:22000}],[0,1]);
  assert.equal(paused[0].text,cue.text);assert.equal(paused[0].endMs,22000);
});
test("manual merge rejects hidden intervening cues, cross-segment ranges, invalid selections and oversized content",()=>{
  const source=[cue,{...cue,index:2},{...cue,index:3}];
  for(const indices of [[],[0],[0,2],[-1,0],[0,99],[0,0.5]])assert.throws(()=>mergeSelectedCues(source,indices));
  assert.throws(()=>mergeSelectedCues([cue,{...cue,segment:1}],[0,1]),/đoạn thu/);
  assert.throws(()=>mergeSelectedCues([cue,{...cue,text:""}],[0,1]),/không hợp lệ/);
  assert.throws(()=>mergeSelectedCues([{...cue,text:"a".repeat(5000)},{...cue,text:"b".repeat(5000)}],[0,1]),/quá dài/);
});
test.after(async()=>fs.rm(root,{recursive:true,force:true}));
