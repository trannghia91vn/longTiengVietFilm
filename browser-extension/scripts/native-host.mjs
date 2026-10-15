import { spawn, execFileSync } from "node:child_process";
import { copyFile, mkdir, writeFile, rename, chmod, readFile, lstat } from "node:fs/promises";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";
import path from "node:path";
import os from "node:os";

const root = fileURLToPath(new URL("../../", import.meta.url));
const crate = path.join(root, "browser-extension/native-host");
const mode = process.argv[2];
if (mode === "build" || mode === "test") {
  const args = [mode, "--locked", "--manifest-path", path.join(crate, "Cargo.toml"), ...(mode === "build" ? ["--release"] : [])];
  process.exitCode = await new Promise(resolve => spawn("cargo", args, { stdio: "inherit" }).on("exit", resolve));
} else if (mode === "process") {
  const id=process.argv[3];
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(id??"")) throw new Error("Usage: npm run ocr:process -- <session UUID>");
  process.exitCode=await new Promise(resolve=>spawn(path.join(crate,"target/release/long-tieng-capture-host"),["--ocr-process",id],{stdio:"inherit"}).on("exit",resolve));
} else if (mode === "register") {
  if (process.platform !== "darwin") throw new Error("Native registration milestone 2A supports macOS only.");
  const extensionId = process.argv[3];
  if (!/^[a-p]{32}$/.test(extensionId ?? "")) throw new Error("Usage: npm run native:register -- <actual Chrome extension ID>");
  const source = path.join(crate, "target/release/long-tieng-capture-host");
  const config = JSON.parse(await readFile(path.join(root, "src-tauri/tauri.conf.json"), "utf8"));
  const data = execFileSync(source, ["--app-data-dir"], { encoding: "utf8" }).trim();
  const expected = path.join(os.homedir(), "Library/Application Support", config.identifier);
  if (data !== expected) throw new Error(`App data resolver mismatch: ${data} vs ${expected}`);
  const installation = path.join(data, "native-host");
  const manifests = path.join(os.homedir(), "Library/Application Support/Google/Chrome/NativeMessagingHosts");
  const engine=path.join(root,"src-tauri/engines/tesseract");
  const vision=path.join(root,"src-tauri/engines/vision-ocr");
  const model=await readFile(path.join(crate,"runtime/vie.traineddata"));
  const modelHash=createHash("sha256").update(model).digest("hex");
  if(modelHash!=="79df64caf7bcfb2a27df5042ecb6121e196eada34da774956995747636d5bfa1") throw new Error("OCR model checksum mismatch; run ocr:prepare");
  const modelDir=path.join(data,"models/ocr-vie-fast");
  const modelPath=path.join(modelDir,"vie.traineddata");
  try {const current=await readFile(modelPath);if(createHash("sha256").update(current).digest("hex")!==modelHash)throw new Error("Existing OCR model differs; repair it explicitly in Model Manager");}
  catch(error){if(error.code!=="ENOENT")throw error;}
  const licenses=["tesseract","leptonica","libpng"].map(name=>path.join(root,`src-tauri/engines/LICENSE.${name}.txt`));
  for(const file of [engine,vision,...licenses]) await readFile(file);
  for(const file of [modelDir,modelPath,path.join(data,"models"),... ["long-tieng-capture-host","tesseract","vision-ocr","host-config.json"].flatMap(name=>[path.join(installation,name),path.join(installation,`${name}.tmp`)])]) {
    try {if((await lstat(file)).isSymbolicLink())throw new Error("Refusing symlink native installation path");}catch(error){if(error.code!=="ENOENT")throw error;}
  }
  for (const directory of [installation, manifests]) {
    try { if ((await lstat(directory)).isSymbolicLink()) throw new Error("Refusing symlink installation directory"); } catch (error) { if (error.code !== "ENOENT") throw error; }
    await mkdir(directory, { recursive: true });
  }
  const destination = path.join(installation, "long-tieng-capture-host");
  await copyFile(source, `${destination}.tmp`);
  await chmod(`${destination}.tmp`, 0o700);
  await rename(`${destination}.tmp`, destination);
  await copyFile(engine,path.join(installation,"tesseract.tmp"));
  await chmod(path.join(installation,"tesseract.tmp"),0o700);
  await rename(path.join(installation,"tesseract.tmp"),path.join(installation,"tesseract"));
  await copyFile(vision,path.join(installation,"vision-ocr.tmp"));
  await chmod(path.join(installation,"vision-ocr.tmp"),0o700);
  await rename(path.join(installation,"vision-ocr.tmp"),path.join(installation,"vision-ocr"));
  try {if((await lstat(modelDir)).isSymbolicLink()) throw new Error("Refusing symlink model directory");}catch(error){if(error.code!=="ENOENT")throw error;}
  await mkdir(modelDir,{recursive:true});
  try {const current=await readFile(modelPath);if(createHash("sha256").update(current).digest("hex")!==createHash("sha256").update(model).digest("hex")) throw new Error("Existing OCR model differs; repair it explicitly in Model Manager");}
  catch(error){if(error.code!=="ENOENT")throw error;await writeFile(`${modelPath}.tmp`,model,{mode:0o600});await rename(`${modelPath}.tmp`,modelPath);}
  for(const name of ["tesseract","leptonica","libpng"]) await copyFile(path.join(root,`src-tauri/engines/LICENSE.${name}.txt`),path.join(installation,`LICENSE.${name}.txt`));
  await copyFile(path.join(root,"src-tauri/engines/LICENSE.tesseract.txt"),path.join(modelDir,"LICENSE.txt"));
  const origin = `chrome-extension://${extensionId}/`;
  for (const [filePath, value] of [
    [path.join(installation, "host-config.json"), { allowedOrigin: origin, autoOcr: true }],
    [path.join(manifests, "com.basushi.longtieng.capture.json"), { name: "com.basushi.longtieng.capture", description: "Long Tieng local capture storage", path: destination, type: "stdio", allowed_origins: [origin] }],
  ]) {
    await writeFile(`${filePath}.tmp`, JSON.stringify(value, null, 2), { mode: 0o600 });
    await rename(`${filePath}.tmp`, filePath);
  }
  console.log(`Registered native helper for ${extensionId}\nBinary: ${destination}\nApp data: ${data}\nReload the extension, then connect the helper.`);
} else { throw new Error("Expected build, test or register"); }
