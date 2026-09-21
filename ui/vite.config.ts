import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// The dev server is fixed to 6273 because src-tauri/tauri.conf.json points the
// desktop window at exactly that address; a port that silently moves would
// leave the window on a blank page.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    host: "127.0.0.1",
    port: 6273,
    strictPort: true,
  },
  build: {
    target: "es2022",
    sourcemap: true,
  },
});
