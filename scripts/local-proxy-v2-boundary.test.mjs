import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const server = await readFile("src-tauri/src/services/proxy/server.rs", "utf8");
const execution = await readFile("src-tauri/src/services/proxy/execution.rs", "utf8");
const endpointAdapter = await readFile("src-tauri/src/services/proxy/endpoint_adapter.rs", "utf8");
const runtime = await readFile("src-tauri/src/services/proxy/runtime.rs", "utf8");
const startup = await readFile("src-tauri/src/services/proxy/startup.rs", "utf8");

assert.doesNotMatch(server, /std::net::TcpListener|thread::spawn|httparse|ureq/);
assert.doesNotMatch(execution, /TcpStream|httparse|ureq/);
assert.doesNotMatch(endpointAdapter, /record_station_key|insert_request_log|finalize_request_log/);
assert.match(runtime, /V2ProxyExecutor/);
assert.match(runtime, /LifecycleWriter::start/);
assert.doesNotMatch(runtime, /RequestFinalizationService/);
assert.match(startup, /services\.request_finalization\.clone\(\)/);
assert.match(startup, /Arc<dyn RequestLifecycleStore>/);
assert.doesNotMatch(runtime, /ProxyRuntimeMode/);

console.log("local proxy v2 boundary contract passed");
