import "fake-indexeddb/auto";
import test from "node:test";
import assert from "node:assert/strict";
import { glyphSignature, sameGlyphs, SmartDetector, SMART_RAM_BYTES } from "../src/smart-capture";
import { saveSession, saveSample, saveObservations, readSession, nextOutbox, acknowledge } from "../src/storage";
import { emptyNative } from "../src/native-protocol";
import type { CaptureSession, CaptureObservation, Sample } from "../src/types";

// Seed all v2 stores before the lazy v3 migration, independently of the v1 test file.
await new Promise<void>((resolve,reject)=>{
  const request=indexedDB.open("long-tieng-capture",2);
  request.onupgradeneeded=()=>{const db=request.result;db.createObjectStore("sessions",{keyPath:"id"});db.createObjectStore("samples",{keyPath:["sessionId","sequence"]}).createIndex("sessionId","sessionId");db.createObjectStore("outbox",{keyPath:["sessionId","messageSeq"]});db.createObjectStore("nativeSamples",{keyPath:["sessionId","sequence"]});db.createObjectStore("previews",{keyPath:"sessionId"});};
  request.onerror=()=>reject(request.error);request.onsuccess=()=>{const db=request.result;const tx=db.transaction("outbox","readwrite");tx.objectStore("outbox").add({sessionId:"v2-pending",messageSeq:4,operation:"appendSample",payloadJson:"{}",image:new Blob(["pending-png"]),byteLength:11});tx.oncomplete=()=>{db.close();resolve();};tx.onerror=()=>reject(tx.error);};
});
test("v2 migration keeps unacknowledged PNG and message sequence",async()=>{const pending=await nextOutbox("v2-pending");assert.equal(pending?.messageSeq,4);assert.equal(await pending?.image?.text(),"pending-png");});

function pixels(seed=0) {
  const data = new Uint8ClampedArray(100 * 40 * 4);
  for(let i=0;i<data.length;i+=4){data[i]=data[i+1]=data[i+2]=20+seed%40;data[i+3]=255;}
  for(let y=12;y<25;y++)for(let x=25;x<70;x++){if(x%9<4){const i=(y*100+x)*4;data[i]=data[i+1]=data[i+2]=240;}}
  return data;
}
test("moving dark background preserves glyph signature; accents change it",()=>{
  const a=glyphSignature(pixels(0),100,40), b=glyphSignature(pixels(30),100,40);
  assert.equal(a.supported,true);assert.equal(sameGlyphs(a,b),true);
  const changed=pixels();for(let x=25;x<30;x++){const i=(7*100+x)*4;changed[i]=changed[i+1]=changed[i+2]=255;}
  assert.equal(sameGlyphs(a,glyphSignature(changed,100,40)),false);
});
test("three observations confirm stability; periodic evidence, reset, and fixed anchor",()=>{
  const d=new SmartDetector(), a=glyphSignature(pixels(),100,40);
  assert.equal(d.observe(a,0).role,"start");assert.equal(d.observe(a,100).stable,false);
  assert.equal(d.observe(a,200).role,"stable");assert.equal(d.observe(a,300).save,false);
  assert.equal(d.observe(a,2200).role,"periodic");assert.equal(d.observe(a,2200).save,false);
  d.reset();assert.equal(d.observe(a,2300).role,"start");
  assert.throws(()=>d.observe(a,NaN));assert.throws(()=>glyphSignature(new Uint8ClampedArray(SMART_RAM_BYTES),2000,2000));
});
test("unsupported/empty glyphs always retain images rather than inventing silence",()=>{
  const d=new SmartDetector(), empty=glyphSignature(new Uint8ClampedArray(100*40*4),100,40);
  for(let i=0;i<30;i++)assert.deepEqual([d.observe(empty,i*100).role,empty.supported],["fallback",false]);
});
test("short changing subtitles are retained before stability",()=>{
  const d=new SmartDetector();let previous=0;
  for(let i=0;i<8;i++){
    const a=glyphSignature(pixels(),100,40);a.mask[100+i]=1;
    const result=d.observe(a,i*100);assert.equal(result.save,true);assert.ok(result.group>=previous);previous=result.group;
  }
});
test("smart outbox commits observation references, rejects missing evidence and preserves recovery",async()=>{
  const id="smart-test";
  let s:CaptureSession={schemaVersion:2,captureMode:"smart",observationCount:0,fallbackCount:0,id,title:"smart",url:"http://localhost",createdAt:1,endedAt:null,status:"recording",source:{id:"doc:video",documentId:"doc",frameId:1,origin:"http://localhost"},region:{x:0,y:0,width:1,height:1},sampleCount:0,imageBytes:0,events:[],native:emptyNative()};
  s=await saveSession(s);
  const image:Sample={sessionId:id,sequence:1,segment:0,capturedEpochMs:1,estimatedVideoTimeMs:1,uncertaintyMs:100,roundTripMs:1,captureSize:{width:2,height:2},region:s.region,manual:false,image:new Blob(["png"]),smart:{group:1,stable:true,save:true,role:"stable"},clock:{videoId:"video",sourceKey:"movie",mediaTime:.001,duration:20,rate:1,paused:false,seeking:false,waiting:false,ended:false,readyState:4,epochMs:1,frameMediaTime:null,frameEpochMs:null,layout:"fixed",width:2,height:2}};
  s=await saveSample({...s,sampleCount:1,imageBytes:3},image);
  const {image:_image,sequence:_sequence,...metadata}=image;
  const o:CaptureObservation={...metadata,observationSeq:1,imageSequence:1,smart:image.smart!};
  await assert.rejects(saveObservations(s,[{...o,imageSequence:2}]));
  assert.equal((await readSession(id))?.observationCount,0);
  s=await saveObservations(s,[o]);await assert.rejects(saveObservations(s,[o]));
  assert.equal((await readSession(id))?.observationCount,1);
  for(let i=1;i<=3;i++){const pending=(await nextOutbox(id))!;assert.equal(pending.messageSeq,i);await acknowledge(pending,{ackSequence:i,savedSamples:i>1?1:0,savedBytes:i>1?3:0,finalized:false,sessionPath:"/qa"});}
  assert.equal((await readSession(id))?.native?.pendingBytes,0);
});
