import { readFileSync } from "node:fs";
import { strict as assert } from "node:assert";

const source = readFileSync("src/features/stations/providerPresets.ts", "utf8");

const kamiApiName = "\u5361\u7c73API";

assert.doesNotMatch(source, /kamiapi/);
assert.doesNotMatch(source, new RegExp(kamiApiName));
assert.doesNotMatch(source, /https:\/\/www\.kamiapi\.top/);

console.log("kamiapi provider preset removal guard passed");
