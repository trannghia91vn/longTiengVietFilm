import { build as bundle } from "esbuild";
import { build } from "vite";
import react from "@vitejs/plugin-react";
import { copyFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = fileURLToPath(new URL("../", import.meta.url));
await build({
  configFile: false,
  root,
  base: "./",
  plugins: [react()],
  build: {
    outDir: "dist",
    target: "chrome116",
    rollupOptions: { input: { recorder: path.join(root, "recorder.html"), offscreen: path.join(root, "offscreen.html") } },
  },
});
await bundle({ entryPoints: [path.join(root, "src/service-worker.ts")], bundle: true, format: "esm", target: "chrome116", outfile: path.join(root, "dist/service-worker.js") });
await bundle({ entryPoints: [path.join(root, "src/content.ts")], bundle: true, format: "iife", target: "chrome116", outfile: path.join(root, "dist/content.js") });
await copyFile(path.join(root, "manifest.json"), path.join(root, "dist/manifest.json"));
console.log(`Load unpacked: ${path.join(root, "dist")}`);
