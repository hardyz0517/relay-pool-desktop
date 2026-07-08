import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";

await assert.rejects(
  access("src/features/pricing/officialModelCatalog.ts"),
  /ENOENT/,
  "pricing comparison should not keep an official model catalog with model-level prices",
);

const pageSource = await readFile("src/features/pricing/PricingPage.tsx", "utf8");
const viewModelSource = await readFile("src/features/pricing/pricingComparisonViewModel.ts", "utf8");

assert.ok(
  !pageSource.includes("enabledOfficialModelCatalog") && !viewModelSource.includes("OfficialModelProvider"),
  "pricing comparison should not depend on official model providers",
);
assert.ok(
  viewModelSource.includes('PricingGroupType = "gpt" | "claude" | "gemini" | "grok" | "image_generation"'),
  "pricing comparison should classify rows by supported group types",
);
assert.ok(
  viewModelSource.includes('return "image_generation"') &&
    viewModelSource.includes("isImageGenerationGroupName(candidate.groupName)"),
  "image-named groups should be classified before GPT/OpenAI platform matching",
);

await import("./pricing-group-comparison-view-model.test.mjs");
