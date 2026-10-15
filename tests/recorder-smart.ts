// Isolated UI transport; no real tab capture or native messaging is invoked.
import { saveSession } from "../browser-extension/src/storage";
import { emptyNative } from "../browser-extension/src/native-protocol";
import type { RecorderView, Command, NativeCommand } from "../browser-extension/src/types";
const old = new URLSearchParams(location.search).get("helper") === "old";
const missingNavigation = new URLSearchParams(location.search).get("navigation") === "missing";
const deniedNavigation = new URLSearchParams(location.search).get("navigation") === "denied";
const undeclaredNavigation = new URLSearchParams(location.search).get("navigation") === "undeclared";
const view: RecorderView = { target: {id:1,title:"QA · Subtitle fixture",url:"http://localhost/fixture"},connected:true,preview:"/browser-extension/tests/results/ocr-review/frame.png",previewSize:{width:960,height:110},region:{x:0,y:0,width:1,height:1},source:null,session:null,clock:null,error:null,busy:false,sources:[],frameOrigins:[],topLayout:"fixed",captureMode:"all",detectorState:"stable",navigationAllowed:true,native:{connected:true,appData:"QA only",hostVersion:"QA",error:null,capabilities:old?[]:["smart-observations-v1"]}};
const clock={videoId:"video",sourceKey:"fixture",mediaTime:12,duration:20,rate:1,paused:false,seeking:false,waiting:false,ended:false,readyState:4,epochMs:1000,frameMediaTime:null,frameEpochMs:null,layout:"fixed",width:960,height:110};
view.clock=clock;view.source={id:"doc:video",frameId:1,documentId:"doc",origin:"http://localhost",clock,seenAt:Date.now()};view.sources=[view.source];
if (missingNavigation || deniedNavigation || undeclaredNavigation) view.navigationAllowed = false;
const request=async ({command}:{command:Command|NativeCommand})=>{
  if(command.action==="capture-mode")view.captureMode=command.mode;
  if(command.action==="scan"&&!deniedNavigation)view.navigationAllowed=true;
  if(command.action==="start"){
    view.session=await saveSession({schemaVersion:view.captureMode==="smart"?2:1,captureMode:view.captureMode,observationCount:120,fallbackCount:4,id:crypto.randomUUID(),title:"QA · Subtitle fixture",url:"http://localhost/fixture",createdAt:Date.now(),endedAt:null,status:"recording",source:{id:"doc:video",frameId:1,documentId:"doc",origin:"http://localhost"},region:view.region!,sampleCount:18,imageBytes:500000,events:[],native:{...emptyNative(),savedSamples:18,savedBytes:500000}});
  }
  if(command.action==="pause"&&view.session){view.session={...view.session,status:"paused"};view.error="QA: bố cục player đã đổi. Không thu thêm mẫu.";}
  if(command.action==="state"&&view.session?.status==="recording")view.lastObservationAt=Date.now();
  if(command.action==="stop"&&view.session){view.connected=false;view.session={...view.session,status:"stopped",native:{...view.session.native!,finalized:true}};}
  return {ok:true,value:command.action==="state"?structuredClone(view):null};
};
Object.assign(globalThis,{chrome:{runtime:{id:"qa-local-only",sendMessage:request,getManifest:()=>({version:undeclaredNavigation?"0.3.0":"0.3.1",optional_permissions:undeclaredNavigation?[]:["webNavigation"]})},permissions:{contains:async()=>true,request:async()=>!deniedNavigation}}});
await import("../browser-extension/src/recorder");
