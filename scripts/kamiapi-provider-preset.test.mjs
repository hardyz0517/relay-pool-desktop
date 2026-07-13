import { readFileSync } from "node:fs";
import { strict as assert } from "node:assert";

const source = readFileSync("src/features/stations/providerPresets.ts", "utf8");

const kamiApiName = "\u5361\u7c73API";

assert.match(source, /kamiapi/);
assert.match(source, new RegExp(kamiApiName));
assert.match(source, /https:\/\/www\.kamiapi\.top/);
assert.match(source, /stationType:\s*"newapi"/);

console.log("kamiapi provider preset source guard passed");
