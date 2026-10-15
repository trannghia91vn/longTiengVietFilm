import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import { readFile, access } from "node:fs/promises";
import { constants } from "node:fs";
import path from "node:path";

const root = fileURLToPath(new URL("../", import.meta.url));
if (process.platform !== "darwin") throw new Error("Local release currently supports macOS only.");
const engines = path.join(root, "src-tauri/engines");
for (const name of ["tesseract", "vision-ocr", "audiocpp_cli", "ffmpeg", "ffprobe", "libsea_g2p_rs.dylib", "sea_g2p.bin"]) {
  try { await access(path.join(engines, name), name === "sea_g2p.bin" || name.endsWith(".dylib") ? constants.R_OK : constants.R_OK | constants.X_OK); }
  catch { throw new Error(`Missing OCR/WAV engine ${name}. Prepare engines before building the local app.`); }
}
const missing = [];
for (const name of ["whisper-cli", "llama-cli", "llama-server"]) {
  try { await access(path.join(engines, name), constants.R_OK | constants.X_OK); }
  catch { missing.push(name); }
}
if (missing.length) console.warn(`Warning: OCR/WAV build only; general transcription/AI translation engines not bundled: ${missing.join(", ")}. Run scripts/prepare-engines-macos.sh to prepare those features.`);
for (const args of [["run", "extension:build"], ["run", "native:build"], ["run", "tauri", "build", "--", "--bundles", "app"]]) {
  const code = await new Promise((resolve, reject) => {
    const child = spawn("npm", args, { cwd: root, stdio: "inherit" });
    child.on("error", reject); child.on("exit", code => resolve(code ?? 1));
  });
  if (code !== 0) { process.exitCode = code; break; }
}
if (!process.exitCode) {
  const config = JSON.parse(await readFile(path.join(root, "src-tauri/tauri.conf.json"), "utf8"));
  console.log(`\nApp: ${path.join(root, "src-tauri/target/release/bundle/macos", `${config.productName}.app`)}`);
  console.log(`Extension: ${path.join(root, "browser-extension/dist")}`);
  console.log("Build only: no native registration, Chrome reload, app launch or model download was performed.");
}
