import fs from "node:fs";
import path from "node:path";

const root = process.cwd();

function read(relativePath) {
  return fs.readFileSync(path.join(root, relativePath), "utf8");
}

function assertIncludes(source, expected, message) {
  if (!source.includes(expected)) {
    throw new Error(message);
  }
}

const app = read("src/app/App.tsx");
const pricingPage = read("src/features/pricing/PricingPage.tsx");
const settingsPage = read("src/features/settings/SettingsPage.tsx");
const economicsApi = read("src/lib/api/economics.ts");
const economicsTypes = read("src/lib/types/economics.ts");
const commands = read("src-tauri/src/commands/mod.rs");
const lib = read("src-tauri/src/lib.rs");

assertIncludes(app, "ModelBasePricesPage", "App should route to the model base prices page.");
assertIncludes(app, 'case "modelBasePrices"', "App should include a modelBasePrices route case.");
assertIncludes(
  pricingPage,
  "模型基准价格",
  "Pricing page should expose a top-right model base price entry.",
);
assertIncludes(
  settingsPage,
  "模型基准价格",
  "Settings should expose a model base price entry.",
);
assertIncludes(
  economicsApi,
  "listModelBasePrices",
  "Economics API should expose listModelBasePrices.",
);
assertIncludes(
  economicsApi,
  "upsertModelBasePrice",
  "Economics API should expose upsertModelBasePrice.",
);
assertIncludes(
  economicsApi,
  "resetModelBasePricesToBuiltins",
  "Economics API should expose resetModelBasePricesToBuiltins.",
);
assertIncludes(
  economicsTypes,
  "export type ModelBasePrice",
  "Economics types should include ModelBasePrice.",
);
assertIncludes(
  commands,
  "list_model_base_prices",
  "Tauri commands should expose list_model_base_prices.",
);
assertIncludes(
  commands,
  "upsert_model_base_price",
  "Tauri commands should expose upsert_model_base_price.",
);
assertIncludes(
  lib,
  "commands::list_model_base_prices",
  "Tauri invoke handler should register list_model_base_prices.",
);

console.log("model base prices page contract ok");
