import { spawnSync } from "node:child_process";
import { copyFile, mkdir, readFile, writeFile } from "node:fs/promises";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";
import path from "node:path";
import os from "node:os";

const root = fileURLToPath(new URL("../../", import.meta.url));
if (process.platform !== "darwin") throw new Error("OCR sidecar preparation currently supports macOS.");
const build = path.join(os.tmpdir(), "long-tieng-ocr-build");
const prefix = path.join(build, "install");
const runtime = path.join(root, "browser-extension/native-host/runtime");
const engines = path.join(root, "src-tauri/engines");
await mkdir(build, { recursive: true });
await mkdir(runtime, { recursive: true });
await mkdir(engines, { recursive: true });
const sources = [
  ["png.tar.gz", "https://codeload.github.com/pnggroup/libpng/tar.gz/refs/tags/v1.6.44", "0ef5b633d0c65f780c4fced27ff832998e71478c13b45dfb6e94f23a82f64f7c"],
  ["lept.tar.gz", "https://codeload.github.com/DanBloomberg/leptonica/tar.gz/refs/tags/1.85.0", "c01376bce0379d4ea4bc2ec5d5cbddaa49e2f06f88242619ab8c059e21adf233"],
  ["tess.tar.gz", "https://codeload.github.com/tesseract-ocr/tesseract/tar.gz/refs/tags/5.5.0", "f2fb34ca035b6d087a42875a35a7a5c4155fa9979c6132365b1e5a28ebc3fc11"],
  ["vie.traineddata", "https://raw.githubusercontent.com/tesseract-ocr/tessdata_fast/87416418657359cb625c412a48b6e1d6d41c29bd/vie.traineddata", "79df64caf7bcfb2a27df5042ecb6121e196eada34da774956995747636d5bfa1"],
];
for (const [name, url, expected] of sources) {
  const destination = path.join(build, name);
  let bytes = await readFile(destination).catch(() => null);
  if (!bytes || createHash("sha256").update(bytes).digest("hex") !== expected) {
    const response = await fetch(url);
    if (!response.ok) throw new Error(`${url}: ${response.status}`);
    bytes = Buffer.from(await response.arrayBuffer());
    if (createHash("sha256").update(bytes).digest("hex") !== expected) throw new Error(`Checksum mismatch: ${name}`);
    await writeFile(destination, bytes);
  }
}
function run(program, args) {
  const result = spawnSync(program, args, { stdio: "inherit" });
  if (result.error || result.status !== 0) throw new Error(`${program} failed: ${result.error ?? result.status}`);
}
const sdk = spawnSync("xcrun", ["--show-sdk-path"], { encoding: "utf8" });
if (sdk.status !== 0) throw new Error("macOS SDK not available");
const common = ["-DCMAKE_BUILD_TYPE=Release", "-DBUILD_SHARED_LIBS=OFF", `-DCMAKE_INSTALL_PREFIX=${prefix}`, `-DCMAKE_PREFIX_PATH=${prefix}`, "-DCMAKE_OSX_DEPLOYMENT_TARGET=11.0", `-DZLIB_LIBRARY=${sdk.stdout.trim()}/usr/lib/libz.tbd`];
for (const name of ["png", "lept", "tess"]) run("tar", ["-xzf", path.join(build, `${name}.tar.gz`), "-C", build]);
const definitions = [
  ["libpng-1.6.44", ["-DPNG_SHARED=OFF", "-DPNG_FRAMEWORK=OFF", "-DPNG_STATIC=ON", "-DPNG_TESTS=OFF", "-DPNG_TOOLS=OFF"]],
  ["leptonica-1.85.0", ["-DBUILD_PROG=OFF", "-DENABLE_GIF=OFF", "-DENABLE_JPEG=OFF", "-DENABLE_TIFF=OFF", "-DENABLE_WEBP=OFF", "-DENABLE_OPENJPEG=OFF", `-DPNG_LIBRARY=${prefix}/lib/libpng16.a`]],
  ["tesseract-5.5.0", ["-DBUILD_TRAINING_TOOLS=OFF", "-DBUILD_TESTS=OFF", "-DDISABLE_TIFF=ON", "-DDISABLE_ARCHIVE=ON", "-DDISABLE_CURL=ON", "-DGRAPHICS_DISABLED=ON", "-DOPENMP_BUILD=OFF", "-DENABLE_NATIVE=OFF", "-DSW_BUILD=OFF"]],
];
for (const [source, options] of definitions) {
  const directory = path.join(build, source);
  run("cmake", ["-S", directory, "-B", `${directory}/build`, ...common, ...options]);
  run("cmake", ["--build", `${directory}/build`, "-j", String(Math.min(os.availableParallelism(), 8))]);
  run("cmake", ["--install", `${directory}/build`]);
}
const binary = path.join(prefix, "bin/tesseract");
const dependencies = spawnSync("otool", ["-L", binary], { encoding: "utf8" });
if (dependencies.status !== 0 || /\/opt\/homebrew|\/usr\/local|\/private\/tmp|\/var\/folders/.test(dependencies.stdout.split("\n").slice(1).join("\n"))) throw new Error(`Non-portable OCR sidecar:\n${dependencies.stdout}`);
await copyFile(binary, path.join(engines, "tesseract"));
await copyFile(path.join(build, "vie.traineddata"), path.join(runtime, "vie.traineddata"));
await copyFile(path.join(build, "tesseract-5.5.0/LICENSE"), path.join(engines, "LICENSE.tesseract.txt"));
await copyFile(path.join(build, "leptonica-1.85.0/leptonica-license.txt"), path.join(engines, "LICENSE.leptonica.txt"));
await copyFile(path.join(build, "libpng-1.6.44/LICENSE"), path.join(engines, "LICENSE.libpng.txt"));
console.log(`Portable OCR engine: ${engines}/tesseract\nVerified Vietnamese model: ${runtime}/vie.traineddata\nModel is installed into app data by native:register; Model Manager can download it separately.`);
run("swiftc", ["-O", "-module-cache-path", path.join(build,"swift-cache"), "-target", `${process.arch === "arm64" ? "arm64" : "x86_64"}-apple-macosx13.0`, "-framework", "Vision", path.join(root,"browser-extension/native-host/vision-ocr/main.swift"), "-o", path.join(engines,"vision-ocr")]);
console.log("Native Vision adapter ready; it is selected only when Vietnamese support is reported by the host OS.");
