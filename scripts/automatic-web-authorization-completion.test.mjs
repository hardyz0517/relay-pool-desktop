import { readFileSync } from "node:fs";
import { strict as assert } from "node:assert";

const commands = readFileSync("src-tauri/src/commands/mod.rs", "utf8");
const collectorApi = readFileSync("src/lib/api/collector.ts", "utf8");
const capturePermission = readFileSync("src-tauri/permissions/record-capture-event.toml", "utf8");

assert.match(commands, /finish_web_authorization_session/);
assert.match(commands, /webAuthorizationCandidate/);
assert.match(commands, /__relayPoolAuthorizationFinishInFlight/);
assert.match(commands, /record_capture_event/);
assert.match(collectorApi, /finishWebAuthorizationSession/);
assert.match(capturePermission, /"record_capture_event"/);
assert.match(capturePermission, /"finish_web_authorization_session"/);

console.log("automatic web authorization completion source guard passed");
