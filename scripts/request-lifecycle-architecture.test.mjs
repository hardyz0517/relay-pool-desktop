import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import path from "node:path";

const root = process.cwd();
const proxyDir = path.join(root, "src-tauri", "src", "services", "proxy");

async function read(relativePath) {
  return readFile(path.join(root, relativePath), "utf8");
}

async function filesUnder(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const fullPath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...(await filesUnder(fullPath)));
    } else if (entry.isFile()) {
      files.push(fullPath);
    }
  }
  return files;
}

function relative(file) {
  return path.relative(root, file).replaceAll(path.sep, "/");
}

function stripTestModules(source) {
  return source.replaceAll(/#\[cfg\(test\)\][\s\S]*?(?=\n(?:pub|pub\(crate\)|mod|use|const|fn|struct|enum|impl)\b|$)/g, "");
}

function assertNoMatch(source, pattern, label) {
  assert.doesNotMatch(source, pattern, `${label} must not match ${pattern}`);
}

const productionProxyFiles = (await filesUnder(proxyDir))
  .filter((file) => file.endsWith(".rs"))
  .filter((file) => !relative(file).includes("test_support.rs"))
  .filter((file) => !relative(file).includes("_tests.rs"));

for (const file of productionProxyFiles) {
  const label = relative(file);
  const source = stripTestModules(await readFile(file, "utf8"));
  assertNoMatch(source, /\bFinalRequestOutcome\b/, label);
  assertNoMatch(source, /\bCandidateFeedback\b/, label);
  assertNoMatch(source, /\bFailedRequestContext\b/, label);
  assertNoMatch(source, /\bResponseMode\b/, label);
  assertNoMatch(source, /\bFinalizationDispatcher\b/, label);
  assertNoMatch(source, /\bFinalizingStream\b/, label);
  assertNoMatch(source, /\bAttemptTrace\b/, label);
  assertNoMatch(source, /\bProxyRuntimeMode\b/, label);
  assertNoMatch(source, /\bfrom_environment_for_dev\b/, label);
  assertNoMatch(source, /RELAY_POOL_PROXY_RUNTIME/, label);
  assertNoMatch(source, /success\("queued"\)/, label);
  assertNoMatch(source, /serialize_attempt_traces/, label);
  assertNoMatch(source, /attempts_json\s*=\s*Some\(/, label);
  assertNoMatch(source, /attempts_json:\s*Some\(/, label);
  assertNoMatch(source, /\.attempts_json\s*=\s*(?!\s*None\s*;)[^;]+;/, label);
}

const proxyModule = await read("src-tauri/src/services/proxy/mod.rs");
assert.doesNotMatch(
  proxyModule,
  /\blegacy_runtime\b/,
  "legacy runtime must not be exported in production or tests"
);
const legacyRuntime = await read("src-tauri/src/services/proxy/legacy_runtime.rs").catch((error) => {
  assert.equal(error.code, "ENOENT", "legacy runtime read should fail only because the file is absent");
  return null;
});
assert.equal(legacyRuntime, null, "legacy runtime source must remain deleted");

const request = await read("src-tauri/src/services/proxy/request.rs");
assert.match(request, /pub struct ProxyHttpResponse \{\s*pub status: StatusCode,\s*pub headers: HeaderMap,\s*pub payload: ProxyResponsePayload,\s*\}/s);
assert.doesNotMatch(request, /\boutcome\s*:/);

const responseBody = await read("src-tauri/src/services/proxy/response_body.rs");
assert.match(responseBody, /\bstruct LifecycleBody\b/);
assert.doesNotMatch(responseBody, /\bstruct FinalizingStream\b/);
assert.doesNotMatch(responseBody, /AppDatabase|RequestLogStore|rusqlite|sqlx/i);

const execution = await read("src-tauri/src/services/proxy/execution.rs");
assert.doesNotMatch(execution, /CreateRequestLogInput|insert_request_log|finish_request\(/);
assert.doesNotMatch(execution, /std::net::TcpStream|httparse|ureq/);

const error = await read("src-tauri/src/services/proxy/error.rs");
assert.doesNotMatch(
  error,
  /pub struct ProxyFailure\s*\{[\s\S]*\battempts_json\b[\s\S]*\}/,
  "ProxyFailure must not carry legacy attempts_json projection state"
);

const protocolFiles = productionProxyFiles.filter((file) => relative(file).includes("src-tauri/src/services/proxy/protocol/"));
for (const file of protocolFiles) {
  const label = relative(file);
  const source = stripTestModules(await readFile(file, "utf8"));
  assertNoMatch(source, /database|router|scheduler|RequestLogStore|AppDatabase/i, label);
}

const lifecycleFiles = productionProxyFiles.filter((file) => relative(file).includes("src-tauri/src/services/proxy/lifecycle/"));
for (const file of lifecycleFiles) {
  const label = relative(file);
  const source = stripTestModules(await readFile(file, "utf8"));
  assertNoMatch(source, /reqwest|axum|rusqlite|sqlx|AppDatabase|RequestLogStore/i, label);
  assertNoMatch(source, /mpsc::unbounded|unbounded_channel/, label);
}

console.log("request lifecycle architecture fitness gate passed");
