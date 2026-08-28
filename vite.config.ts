import path from "node:path";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  // Tauri serves the bundled frontend from its local asset protocol.
  // Relative URLs keep release assets resolvable outside the Vite dev server.
  base: "./",
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  clearScreen: false,
  server: {
    port: 1430,
    strictPort: true,
    watch: {
      ignored: [
        "**/src-tauri/target*/**",
        "**/output/**",
        "**/.worktrees/**",
        "**/dist/**",
        "**/.pnpm-store/**",
      ],
    },
  },
});
