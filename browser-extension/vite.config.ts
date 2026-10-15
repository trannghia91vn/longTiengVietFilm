import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  root: "browser-extension",
  plugins: [react()],
  server: { host: "127.0.0.1", port: 1431, strictPort: true },
});
