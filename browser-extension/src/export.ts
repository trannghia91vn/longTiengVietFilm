import { strToU8, zipSync, type Zippable } from "fflate";
import { loadSamples } from "./storage";
import type { CaptureSession } from "./types";

export async function exportSession(session: CaptureSession): Promise<void> {
  const samples = await loadSamples(session.id);
  if (samples.length !== session.sampleCount || samples.some((sample, index) => sample.sequence !== index + 1) || samples.reduce((bytes, sample) => bytes + sample.image.size, 0) !== session.imageBytes) {
    throw new Error("Ảnh và metadata chưa khớp. Chờ lưu xong rồi thử xuất lại.");
  }
  const files: Zippable = {};
  const metadata = [];
  for (const sample of samples) {
    const { image, ...data } = sample;
    const imagePath = `frames/${String(sample.sequence).padStart(6, "0")}.png`;
    files[imagePath] = new Uint8Array(await image.arrayBuffer());
    metadata.push({ ...data, imagePath });
  }
  files["capture.json"] = strToU8(JSON.stringify({ ...session, timing: { source: "video-currentTime", estimateOnly: true, compositorDelayMeasured: false }, samples: metadata }, null, 2));
  files["README.txt"] = strToU8("Long Tieng - Milestone 1\nPNG crops + source video clock in capture.json. No OCR or SRT yet.\nTimestamps are estimates: tab capture compositor delay is not calibrated.\nInspect events for pause, seek, sampling gaps and region changes.\n");
  const data = zipSync(files, { level: 0 });
  const blob = new Blob([data], { type: "application/zip" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = `long-tieng-capture-${session.id}.zip`;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
}
