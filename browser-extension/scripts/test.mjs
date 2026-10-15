import { build } from "esbuild";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

const temporary = await mkdtemp(path.join(tmpdir(), "long-tieng-extension-tests-"));
try {
  const names = ["core", "storage", "native", "bridge", "smart", "geometry", "png"];
  await build({ entryPoints: names.map(name => fileURLToPath(new URL(`../tests/${name}.test.ts`, import.meta.url))), bundle: true, platform: "node", format: "esm", outdir: temporary, outExtension: { ".js": ".mjs" } });
  const exit = await new Promise(resolve => spawn(process.execPath, ["--test", ...names.map(name => path.join(temporary, `${name}.test.mjs`))], { stdio: "inherit" }).on("exit", resolve));
  process.exitCode = exit ?? 1;
} finally { await rm(temporary, { recursive: true, force: true }); }
