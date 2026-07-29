import path from "node:path";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  clearScreen: false,
  build: {
    outDir: "dist-demo",
    rollupOptions: {
      input: path.resolve(__dirname, "demo.html"),
    },
  },
  server: {
    port: 1431,
    strictPort: true,
    watch: {
      ignored: [
        "**/src-tauri/target/**",
        "**/output/**",
        "**/.worktrees/**",
        "**/dist/**",
        "**/dist-demo/**",
        "**/.pnpm-store/**",
      ],
    },
  },
});
