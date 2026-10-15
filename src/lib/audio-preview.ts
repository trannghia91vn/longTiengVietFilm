import type { AudioTrialSettings, OcrDocument } from "../types";

export const VOICE_PRESETS = [
  { id: "thuy_dung", label: "Thùy Dung · nữ" },
  { id: "thuc_doan", label: "Thục Đoan · nữ" },
  { id: "my_duyen", label: "Mỹ Duyên · nữ" },
  { id: "kim_thanh", label: "Kim Thanh · nữ" },
  { id: "thai_son", label: "Thái Sơn · nam" },
  { id: "minh_triet", label: "Minh Triết · nam" },
  { id: "duc_tri", label: "Đức Trí · nam" },
];
export function fullSettings(doc:OcrDocument,voice:string):AudioTrialSettings {
  const last=doc.cues.reduce((end,c)=>Math.max(end,c.endMs),0);
  return {startMs:0,durationMs:doc.coverage.durationMs||Math.min(7200000,last+1000),voice,full:{allowGaps:false}};
}
export function trialIssues(doc: OcrDocument, settings: AudioTrialSettings): string[] {
  const end = settings.startMs + settings.durationMs;
  if (!Number.isSafeInteger(settings.startMs) || settings.startMs < 0 || end > 7200000 || ![30000,60000].includes(settings.durationMs) || !VOICE_PRESETS.some(v=>v.id===settings.voice)) return ["Phạm vi hoặc giọng không hợp lệ."];
  const issues: string[] = [];
  if (doc.coverage.durationMs > 0 && end > doc.coverage.durationMs) issues.push("Đoạn thử vượt thời lượng video.");
  if (!doc.coverage.ranges.some(r=>r.startMs<=settings.startMs+250&&r.endMs+250>=end)) issues.push("Ngoài vùng thu liên tục.");
  const cues = doc.cues.filter(c=>c.startMs<end&&c.endMs>settings.startMs).sort((a,b)=>a.startMs-b.startMs||a.endMs-b.endMs);
  if (!cues.length) issues.push("Không có câu trong đoạn thử.");
  if (cues.length>64) issues.push("Hơn 64 cue; cần gộp/sửa cue vụn.");
  const unreviewed=cues.filter(c=>!c.reviewed).length;
  if (unreviewed) issues.push(`${unreviewed} câu chưa kiểm tra và lưu.`);
  for (let i=0;i<cues.length;i++) {
    if (cues[i].startMs<settings.startMs||cues[i].endMs>end) issues.push(`Cue ${cues[i].index} vắt qua biên đoạn thử.`);
    if (i>0&&cues[i].startMs<cues[i-1].endMs) issues.push(`Cue ${cues[i-1].index}/${cues[i].index} chồng lấn.`);
  }
  return issues;
}
