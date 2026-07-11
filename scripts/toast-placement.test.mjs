import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const toastProviderSource = await readFile("src/components/ui/ToastProvider.tsx", "utf8");

assert.ok(
  toastProviderSource.includes("fixed top-4 left-1/2") &&
    toastProviderSource.includes("-translate-x-1/2") &&
    !toastProviderSource.includes("bottom-4 right-4"),
  "global toast container should be positioned at the top center",
);
