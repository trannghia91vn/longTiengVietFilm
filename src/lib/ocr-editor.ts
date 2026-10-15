import type { OcrCue } from "../types";

export function timestamp(ms: number): string {
  const v = Math.max(0, Math.round(ms));
  return `${String(Math.floor(v / 3600000)).padStart(2, "0")}:${String(Math.floor(v / 60000) % 60).padStart(2, "0")}:${String(Math.floor(v / 1000) % 60).padStart(2, "0")},${String(v % 1000).padStart(3, "0")}`;
}
export function serializeOcr(cues: OcrCue[]): string {
  return cues.map((c,i) => `${i+1}\n${timestamp(c.startMs)} --> ${timestamp(c.endMs)}\n${c.text.trim()}\n`).join("\n");
}
export function invalidCue(c: OcrCue): boolean {
  return !Number.isSafeInteger(c.startMs) || !Number.isSafeInteger(c.endMs) || c.startMs < 0 || c.endMs <= c.startMs || c.endMs > 3600000000 || !c.text.trim() || c.text.length > 8192 || c.text.trim().split(/\r?\n/).some(l=>!l.trim()) || /[\u0000-\u0008\u000b\u000c\u000e-\u001f]/.test(c.text);
}
export function splitCue(c: OcrCue, time: number, left: string, right: string): [OcrCue,OcrCue] {
  if (time <= c.startMs || time >= c.endMs || !Number.isSafeInteger(time) || !left.trim() || !right.trim()) throw new Error("Điểm tách hoặc nội dung không hợp lệ.");
  return [{...c,endMs:time,text:left.trim(),reviewed:false},{...c,startMs:time,text:right.trim(),reviewed:false}];
}
export function mergeCue(a: OcrCue, b: OcrCue): OcrCue {
  if (a.segment !== b.segment) throw new Error("Không gộp các segment khác nhau.");
  return {...a,startMs:Math.min(a.startMs,b.startMs),endMs:Math.max(a.endMs,b.endMs),text:`${a.text.trim()}\n${b.text.trim()}`,reviewed:false,
    confidence:Math.min(a.confidence,b.confidence),evidenceSequences:[...new Set([...a.evidenceSequences,...b.evidenceSequences])].sort((x,y)=>x-y),warnings:[...new Set([...a.warnings,...b.warnings])]};
}

const comparableWords = (text: string) => text.normalize("NFC").toLocaleLowerCase("vi").match(/[\p{L}\p{M}\p{N}]+/gu) ?? [];

// Compare OCR spelling without rewriting the displayed text. Short Vietnamese
// words often differ only in a vowel or tone (bồ/bổ, dễ/dể, trì/trừ).
const consonants = (word: string) => word.normalize("NFD").replace(/\p{M}/gu,"").replace(/[aeiouy]/g,"");
const protectedWords = new Set(["không","chẳng","chưa","đừng","chớ","chả","có"]);
function oneCharacterApart(a: string, b: string): boolean {
  if (Math.abs(a.length-b.length)>1) return false;
  let i=0,j=0,edits=0;
  while(i<a.length&&j<b.length) {
    if(a[i]===b[j]) {i++;j++;continue;}
    if(++edits>1)return false;
    if(a.length>=b.length)i++;
    if(b.length>=a.length)j++;
  }
  return edits+(i<a.length||j<b.length?1:0)<=1;
}

function similarWords(a: string[], b: string[]): boolean {
  if(!a.length||a.length!==b.length)return false;
  const length=Math.max(a.join("").length,b.join("").length);
  let differences=0;
  for(let i=0;i<a.length;i++) {
    if(a[i]===b[i])continue;
    if(length<24||protectedWords.has(a[i])||protectedWords.has(b[i])||/[\p{N}]/u.test(a[i]+b[i])||!oneCharacterApart(a[i],b[i]))return false;
    if(Math.min(a[i].length,b[i].length)<5&&consonants(a[i])!==consonants(b[i]))return false;
    if(++differences/length>0.1||differences/a.length>0.25)return false;
  }
  return true;
}

function representativeCue(group: OcrCue[]): OcrCue {
  const words=group.map(c=>comparableWords(c.text));
  const counts=words[0].map(()=>new Map<string,number>());
  for(const row of words)row.forEach((word,i)=>counts[i].set(word,(counts[i].get(word)??0)+1));
  let best=0,bestScore=-1;
  for(let i=0;i<group.length;i++) {
    const score=words[i].reduce((sum,word,j)=>sum+(counts[j].get(word)??0),0);
    // Keep a reviewed wording first, otherwise select an existing whole cue
    // closest to the group's consensus. Confidence alone often ties at 100%.
    if((group[i].reviewed&&!group[best].reviewed)||(group[i].reviewed===group[best].reviewed&&(score>bestScore||score===bestScore&&group[i].confidence>group[best].confidence))) {
      best=i;bestScore=score;
    }
  }
  return group[best];
}

export function compactCues(source: OcrCue[], maxGapMs=1000): {cues: OcrCue[]; removedCount: number; positions: number[]} {
  const cues: OcrCue[]=[],positions:number[]=[];
  let anchor:string[]=[],groupStart=0;
  const finishGroup=(end:number)=>{
    if(end-groupStart>1) {
      const last=cues.length-1;
      cues[last]={...cues[last],text:representativeCue(source.slice(groupStart,end)).text};
    }
  };
  for(let i=0;i<source.length;i++) {
    const current=source[i],previous=source[i-1],last=cues[cues.length-1];
    const words=comparableWords(current.text);
    // Only consolidate one continuous subtitle occurrence. A repeated line after
    // another sentence, a long pause or a seek remains a separate occurrence.
    if(last&&previous&&!invalidCue(current)&&!invalidCue(last)&&current.segment===last.segment&&current.startMs>=previous.startMs&&current.startMs-previous.endMs<=maxGapMs&&similarWords(anchor,words)) {
      const merged=mergeCue(last,current);
      cues[cues.length-1]={...merged,text:last.text,startRangeMs:last.startMs<=current.startMs?last.startRangeMs:current.startRangeMs,endRangeMs:last.endMs>=current.endMs?last.endRangeMs:current.endRangeMs};
    } else {
      finishGroup(i);cues.push(current);anchor=words;groupStart=i;
    }
    positions.push(cues.length-1);
  }
  finishGroup(source.length);
  return {cues,removedCount:source.length-cues.length,positions};
}

export function selectCueRange(visible: number[], anchor: number | null, target: number): number[] {
  const end=visible.indexOf(target),start=anchor===null?-1:visible.indexOf(anchor);
  if(end<0)return [];
  return start<0?[target]:visible.slice(Math.min(start,end),Math.max(start,end)+1);
}

export function mergeSelectedCues(source: OcrCue[], selection: number[]): OcrCue[] {
  const indices=[...new Set(selection)].sort((a,b)=>a-b);
  if(indices.length<2||indices.some(i=>!Number.isSafeInteger(i)||!source[i]))throw new Error("Chọn ít nhất hai câu để gộp.");
  if(indices.some((i,n)=>n>0&&i!==indices[n-1]+1))throw new Error("Chỉ gộp các câu liên tiếp. Hãy bỏ bộ lọc rồi chọn lại dải câu.");
  const picked=indices.map(i=>source[i]);
  if(picked.some(c=>c.segment!==picked[0].segment))throw new Error("Không gộp các câu ở những đoạn thu khác nhau.");
  if(picked.some(invalidCue))throw new Error("Sửa nội dung hoặc thời gian không hợp lệ trước khi gộp.");
  // Repeated OCR variants keep one wording; distinct sentences retain their
  // text, in list order. Explicit merging may span pauses within one segment.
  const merged=compactCues(picked,Infinity).cues.reduce(mergeCue);
  const first=picked.reduce((a,b)=>a.startMs<=b.startMs?a:b);
  const last=picked.reduce((a,b)=>a.endMs>=b.endMs?a:b);
  const result={...merged,startRangeMs:first.startRangeMs,endRangeMs:last.endRangeMs,reviewed:false};
  if(invalidCue(result))throw new Error("Nội dung sau khi gộp quá dài hoặc không hợp lệ.");
  return [...source.slice(0,indices[0]),result,...source.slice(indices[indices.length-1]+1)];
}
export const ocrState: Record<string,string> = {"not-started":"Chưa OCR",queued:"Chờ OCR",processing:"Đang OCR",interrupted:"Gián đoạn",cancelled:"Đã dừng",error:"Có lỗi",partial:"Đã lưu · Chưa xác minh", "no-text":"Không thấy phụ đề"};
export const warningLabel: Record<string,string> = {
  "single-observation":"Chỉ một ảnh", "low-confidence":"Độ tin cậy thấp", "start-unbounded":"Đầu câu chưa rõ", "end-unbounded":"Cuối câu chưa rõ", "manual-snapshot-only":"Ảnh chụp thủ công", "temporal-consensus-correction":"Đã đối chiếu ảnh lân cận", "variant-disagreement":"Các engine khác kết quả", "short-cue":"Câu rất ngắn", "overlapping-recapture":"Trùng vùng thu lại", "overlap-needs-review":"Thời gian chồng lấn", "unexpected-script-needs-review":"Ký tự bất thường", "user-edited":"Đã chỉnh sửa"
};
