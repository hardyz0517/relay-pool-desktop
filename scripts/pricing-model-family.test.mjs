import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";

await assert.rejects(
  access("src/features/pricing/officialModelCatalog.ts"),
  /ENOENT/,
  "pricing comparison should not keep an official model catalog with model-level prices",
);

const pageSource = await readFile("src/features/pricing/PricingPage.tsx", "utf8");
const viewModelSource = await readFile("src/features/pricing/pricingComparisonViewModel.ts", "utf8");
const groupCategoriesSource = await readFile("src/lib/groupCategories.ts", "utf8");

assert.ok(
  !pageSource.includes("enabledOfficialModelCatalog") && !viewModelSource.includes("OfficialModelProvider"),
  "pricing comparison should not depend on official model providers",
);
assert.ok(
  viewModelSource.includes("export type PricingGroupType = StationGroupCategory") &&
    viewModelSource.includes("groupCategoryDefinitions.map((definition)") &&
    groupCategoriesSource.includes('value: "image_generation"') &&
    groupCategoriesSource.includes('value: "embedding"') &&
    groupCategoriesSource.includes('value: "rerank"'),
  "pricing comparison should classify rows by the shared supported group categories",
);
assert.ok(
  viewModelSource.includes('candidate.currentFact.effectiveGroupCategory !== "unknown"') &&
    groupCategoriesSource.includes("isImageGenerationGroupName(groupName)") &&
    groupCategoriesSource.indexOf("isImageGenerationGroupName(groupName)") <
      groupCategoriesSource.indexOf("groupCategoryFromPlatform(platform)"),
  "image-named groups should be classified by shared category evidence before platform matching",
);

await import("./pricing-group-comparison-view-model.test.mjs");
