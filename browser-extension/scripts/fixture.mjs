import { createRequire } from "node:module";
import { spawn } from "node:child_process";
import { mkdir } from "node:fs/promises";
import { once } from "node:events";
import { fileURLToPath } from "node:url";
import path from "node:path";

const require = createRequire(path.join(process.argv[2] ?? process.cwd(), "package.json"));
const { createCanvas } = require("@napi-rs/canvas");
const directory = fileURLToPath(new URL("../tests/media/", import.meta.url));
await mkdir(directory, { recursive: true });
const encoder = spawn("ffmpeg", ["-hide_banner", "-loglevel", "error", "-f", "rawvideo", "-pixel_format", "rgba", "-video_size", "960x540", "-framerate", "10", "-i", "pipe:0", "-an", "-c:v", "libvpx-vp9", "-pix_fmt", "yuv420p", "-deadline", "realtime", "-cpu-used", "8", "-b:v", "400k", "-y", path.join(directory, "clock.webm")], { stdio: ["pipe", "inherit", "inherit"] });
const canvas = createCanvas(960, 540);
const ctx = canvas.getContext("2d");
const cues = [
  [1, 4, ["Xin chào, đây là phụ đề tiếng Việt."]],
  [5, 8, ["Một câu có dấu: ă â ê ô ơ ư đ.", "Dòng thứ hai giữ đúng thời gian."]],
  [9, 12, ["Câu mới sau khoảng lặng."]],
  [13, 16, ["Xin chào, đây là phụ đề tiếng Việt."]],
  [17, 19, ["Kết thúc đoạn kiểm thử."]],
];
for (let frame = 0; frame < 200; frame++) {
  const time = frame / 10;
  ctx.fillStyle = "#24342c"; ctx.fillRect(0, 0, 960, 540);
  ctx.fillStyle = "#549078"; ctx.fillRect((frame * 7) % 960 - 100, 90, 200, 300);
  ctx.fillStyle = "#c16455"; ctx.fillRect(670 - (frame * 3) % 670, 170, 110, 140);
  ctx.textAlign = "left"; ctx.font = "bold 40px Arial"; ctx.fillStyle = "white";
  ctx.fillText(`00:${String(Math.floor(time)).padStart(2, "0")}.${String((frame % 10) * 100).padStart(3, "0")}`, 35, 65);
  ctx.font = "20px Arial"; ctx.fillText(`FRAME ${String(frame).padStart(3, "0")}`, 35, 100);
  ctx.textAlign = "center"; ctx.font = "bold 32px Arial";
  const lines = cues.find(([start, end]) => time >= start && time < end)?.[2] ?? [];
  lines.forEach((line, index) => {
    const y = 486 - (lines.length - 1 - index) * 42;
    ctx.lineWidth = 5; ctx.strokeStyle = "black"; ctx.strokeText(line, 480, y);
    ctx.fillStyle = "white"; ctx.fillText(line, 480, y);
  });
  if (!encoder.stdin.write(Buffer.from(ctx.getImageData(0, 0, 960, 540).data))) await once(encoder.stdin, "drain");
}
encoder.stdin.end();
const [code] = await once(encoder, "exit");
if (code !== 0) throw new Error(`FFmpeg failed: ${code}`);
console.log(path.join(directory, "clock.webm"));
