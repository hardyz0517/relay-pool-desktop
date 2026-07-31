import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const server = await readFile("src-tauri/src/services/proxy/server.rs", "utf8");
const execution = await readFile("src-tauri/src/services/proxy/execution.rs", "utf8");
const endpointAdapter = await readFile("src-tauri/src/services/proxy/endpoint_adapter.rs", "utf8");
const runtime = await readFile("src-tauri/src/services/proxy/runtime.rs", "utf8");
const startup = await readFile("src-tauri/src/services/proxy/startup.rs", "utf8");
const facade = await readFile("src-tauri/src/application/command_facades/local_proxy.rs", "utf8");
const productionCompositionTest = await readFile(
  "src-tauri/tests/routing_production_composition.rs",
  "utf8",
);
const productionStartupShutdownTest = await readFile(
  "src-tauri/tests/routing_production_startup_shutdown.rs",
  "utf8",
);

assert.doesNotMatch(server, /std::net::TcpListener|thread::spawn|httparse|ureq/);
assert.doesNotMatch(execution, /TcpStream|httparse|ureq/);
assert.doesNotMatch(endpointAdapter, /record_station_key|insert_request_log|finalize_request_log/);
assert.match(runtime, /V2ProxyExecutor/);
assert.match(runtime, /LifecycleWriter::start/);
assert.doesNotMatch(runtime, /RequestFinalizationService/);
assert.match(startup, /services\.request_finalization\.clone\(\)/);
assert.match(startup, /Arc<dyn RequestLifecycleStore>/);
assert.match(
  startup,
  /reconcile_startup_interrupted_request_lifecycle\(\)/,
  "auto-start must reconcile interrupted request lifecycle before proxy admission",
);
assert.match(
  facade,
  /reconcile_startup_interrupted_request_lifecycle\(\)/,
  "manual start/restart/import facade must reconcile interrupted request lifecycle before proxy admission",
);
assert.match(
  runtime,
  /finalization_mode:\s*ProxyFinalizationMode::DualTerminal/,
  "default-v2 production startup config must use dual-terminal finalization",
);
assert.doesNotMatch(runtime, /ProxyRuntimeMode/);
assert.ok(
  productionCompositionTest.includes("start_proxy_with_production_startup") &&
    productionCompositionTest.includes("attempt_cost_count") &&
    productionCompositionTest.includes("cost_aggregate_summary"),
  "production composition test must prove the real startup path writes typed dual-terminal outcomes",
);
assert.ok(
  productionStartupShutdownTest.includes("start_proxy_with_command_facade") &&
    productionStartupShutdownTest.includes("startup_reconciliation_requests_interrupted") &&
    productionStartupShutdownTest.includes("active_requests"),
  "production startup/shutdown test must prove reconciliation-before-admission and quiescent stop",
);

console.log("local proxy v2 boundary contract passed");
