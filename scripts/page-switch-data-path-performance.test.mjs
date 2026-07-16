import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const readSource = (path) =>
  readFile(path, "utf8").catch((error) => {
    if (error?.code === "ENOENT") return "";
    throw error;
  });

const [channelQuerySource, channelPageSource, pricingQuerySource, pricingPageSource, resourceSource] =
  await Promise.all([
    readSource("src/lib/queries/channelQueries.ts"),
    readSource("src/features/channels/ChannelStatusTab.tsx"),
    readSource("src/lib/queries/pricingQueries.ts"),
    readSource("src/features/pricing/PricingPage.tsx"),
    readSource("src/lib/query/resourceQueries.ts"),
  ]);

function escapeRegExp(value) {
  return value.replace(/[|\\{}()[\]^$+*?.-]/g, "\\$&");
}

function countBackendInvokes(source, command) {
  const pattern = new RegExp(
    "\\binvoke\\s*(?:<[^>]{1,200}>)?\\s*\\(\\s*[\"']" + escapeRegExp(command) + "[\"']",
    "g",
  );
  return source.match(pattern)?.length ?? 0;
}

function findRegisteredQueryOptions(source, prefix, queryFunction) {
  const pattern = new RegExp(
    "\\bexport\\s+const\\s+(" +
      escapeRegExp(prefix) +
      "[A-Za-z0-9_$]*QueryOptions)\\s*=[\\s\\S]{0,600}?\\bqueryFn\\s*:\\s*" +
      escapeRegExp(queryFunction) +
      "\\b",
  );
  return source.match(pattern);
}

function importsNamedFrom(source, name, modulePath) {
  const pattern = new RegExp(
    "\\bimport\\s*\\{[^}]*\\b" +
      escapeRegExp(name) +
      "\\b[^}]*\\}\\s*from\\s*[\"']" +
      escapeRegExp(modulePath) +
      "[\"']",
    "s",
  );
  return pattern.test(source);
}

function findActivityQuery(source, queryOptionsName) {
  if (!queryOptionsName) return null;
  const pattern = new RegExp(
    "\\bconst\\s+([A-Za-z_$][\\w$]*)\\s*=\\s*useActivityQuery\\s*\\(\\s*refreshEnabled\\s*,\\s*" +
      escapeRegExp(queryOptionsName) +
      "\\s*\\(",
  );
  return source.match(pattern);
}

function consumesQueryData(source, activityQueryMatch) {
  const queryVariable = activityQueryMatch?.[1];
  return Boolean(
    queryVariable &&
      new RegExp("\\b" + escapeRegExp(queryVariable) + "\\.data\\b").test(source),
  );
}

assert.match(
  channelQuerySource,
  /\bexport\s+(?:async\s+)?function\s+loadChannelStatusWorkspace\s*\(/,
  "channel status query service should expose the shared workspace boundary",
);

assert.equal(
  countBackendInvokes(channelQuerySource, "load_channel_status_workspace"),
  1,
  "channel status query service should invoke load_channel_status_workspace exactly once before using any browser-only fallback",
);

const channelRawReads = [
  "listKeyPoolItems",
  "listRequestLogs",
  "listStationKeyHealth",
  "listChannelStatusSummaries",
];
assert.ok(
  channelRawReads.every(
    (functionName) =>
      !new RegExp("\\b" + escapeRegExp(functionName) + "\\s*\\(").test(channelPageSource),
  ),
  "channel status page must not orchestrate the four raw workspace reads",
);

const channelResourceQuery = findRegisteredQueryOptions(
  resourceSource,
  "channel",
  "loadChannelStatusWorkspace",
);
assert.ok(
  importsNamedFrom(
    resourceSource,
    "loadChannelStatusWorkspace",
    "@/lib/queries/channelQueries",
  ) && channelResourceQuery,
  "resource query options should register the channel status workspace query function",
);

const channelActivityQuery = findActivityQuery(channelPageSource, channelResourceQuery?.[1]);
assert.ok(
  channelActivityQuery && consumesQueryData(channelPageSource, channelActivityQuery),
  "channel status page should assign the activity query and derive its workspace from query data",
);

assert.doesNotMatch(
  channelPageSource,
  /\bloadChannelStatusWorkspace\b/,
  "channel status page must not import or call the workspace loader directly",
);

assert.doesNotMatch(
  channelPageSource,
  /\busePageActivation\b/,
  "channel status page must not use page activation for an extra manual workspace load",
);

assert.match(
  pricingQuerySource,
  /\bexport\s+(?:async\s+)?function\s+loadPricingComparisonWorkspace\s*\(/,
  "pricing query service should expose the shared comparison workspace boundary",
);

assert.equal(
  countBackendInvokes(pricingQuerySource, "load_pricing_comparison_workspace"),
  1,
  "pricing query service should invoke load_pricing_comparison_workspace exactly once before using any browser-only fallback",
);

const pricingResourceQuery = findRegisteredQueryOptions(
  resourceSource,
  "pricing",
  "loadPricingComparisonWorkspace",
);
assert.ok(
  importsNamedFrom(
    resourceSource,
    "loadPricingComparisonWorkspace",
    "@/lib/queries/pricingQueries",
  ) && pricingResourceQuery,
  "resource query options should register the pricing comparison workspace query function",
);

const pricingActivityQuery = findActivityQuery(pricingPageSource, pricingResourceQuery?.[1]);
assert.ok(
  pricingActivityQuery && consumesQueryData(pricingPageSource, pricingActivityQuery),
  "pricing page should assign the activity query and derive its shared workspace from query data",
);

const pricingStationFanoutReads = [
  "listStationGroupBindings",
  "listGroupRateRecords",
  "listStationKeys",
];
assert.ok(
  pricingStationFanoutReads.every(
    (functionName) =>
      !new RegExp("\\b" + escapeRegExp(functionName) + "\\s*\\(").test(pricingPageSource),
  ),
  "pricing page must not map stations into binding, rate-record, or key reads",
);
