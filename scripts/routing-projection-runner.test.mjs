import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const runner = readFileSync("src-tauri/src/background_tasks/routing_projection_runner.rs", "utf8");
const observationStore = readFileSync("src-tauri/src/persistence/stores/routing_observation_store.rs", "utf8");
const qualityProjection = readFileSync("src-tauri/src/application/quality_projection.rs", "utf8");
const startup = readFileSync("src-tauri/src/lib.rs", "utf8");

assert.match(runner, /MAX_ROUTING_PROJECTION_BATCH/);
assert.match(runner, /CancellationToken/);
assert.match(runner, /TaskSpec::new/);
assert.match(runner, /with_shutdown_timeout/);
assert.match(runner, /ROUTING_PROJECTION_CURSOR_SCOPE/);
assert.match(runner, /load_checkpoint_cursor/);
assert.match(runner, /ingestion_cursor/);
assert.match(observationStore, /id > \?2/);
assert.match(observationStore, /MAX\(ingested_at_ms\)/);
assert.match(observationStore, /list_for_scope/);
assert.match(runner, /rebuild_quality_summary_with_checkpoint/);
assert.match(runner, /observation_store\s*\.\s*list_for_scope/);
assert.match(qualityProjection, /routing_quality_v3/);
assert.doesNotMatch(runner, /list_after\(read\.connection\(\), 0,/);
assert.match(startup, /register_routing_projection_task/);
assert.match(startup, /start\(&routing_projection_task\)/);
console.log("routing projection runner lifecycle contract passed");
