import { readFileSync } from "node:fs";
import { strict as assert } from "node:assert";

const collectorsPage = readFileSync("src/features/collectors/CollectorsPage.tsx", "utf8");
const collectorApi = readFileSync("src/lib/api/collector.ts", "utf8");

const webAuthorizationLabel = "\u7f51\u9875\u767b\u5f55\u6388\u6743";
const oldExperimentalLabel = "\u7f51\u9875\u767b\u5f55\u6355\u83b7\uff08\u5b9e\u9a8c\uff09";
const oldOpenedToast = "\u5b9e\u9a8c\u6027\u7f51\u9875\u767b\u5f55\u6355\u83b7\u5df2\u6253\u5f00";

assert.match(collectorApi, /finishWebAuthorizationSession/);
assert.match(collectorsPage, new RegExp(webAuthorizationLabel));
assert.doesNotMatch(collectorsPage, new RegExp(oldExperimentalLabel));
assert.doesNotMatch(collectorsPage, new RegExp(oldOpenedToast));

console.log("web authorization session UI source guard passed");
