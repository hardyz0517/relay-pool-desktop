import { readFile } from "node:fs/promises";

const commandsSource = await readFile("src-tauri/src/commands/mod.rs", "utf8");
const libSource = await readFile("src-tauri/src/lib.rs", "utf8");
const apiSource = await readFile("src/lib/api/economics.ts", "utf8");
const typesSource = await readFile("src/lib/types/economics.ts", "utf8");
const pricingSource = await readFile("src-tauri/src/services/pricing/mod.rs", "utf8");

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

assert(
  commandsSource.includes("resolve_station_key_pricing_context") &&
    commandsSource.includes("ResolvedPricingContext"),
  "commands module should expose a station-key pricing context diagnostic command",
);

assert(
  libSource.includes("commands::resolve_station_key_pricing_context"),
  "diagnostic command should be registered in the Tauri invoke handler",
);

assert(
  apiSource.includes("resolveStationKeyPricingContext") &&
    apiSource.includes('invoke<ResolvedPricingContext>("resolve_station_key_pricing_context"'),
  "frontend economics API should expose the diagnostic command",
);

assert(
  typesSource.includes("export type ResolvedPricingContext") &&
    typesSource.includes("sourceChain: string[]") &&
    typesSource.includes("pricingStatus: PricingStatus"),
  "frontend economics types should include resolved pricing context and source chain",
);

assert(
  pricingSource.includes("pricing_context_from_pricing_parts"),
  "pricing service should expose the same pricing-context builder used by runtime paths",
);
