import path from "node:path";
import { defineConfig } from "vitest/config";

export default defineConfig({
  resolve: {
    alias: {
      "@": path.resolve(import.meta.dirname, "../../../src"),
    },
  },
  test: {
    include: ["scripts/architecture/scale-baseline/frontend-scale-baseline.test.tsx"],
    environment: "jsdom",
    fileParallelism: false,
  },
});
