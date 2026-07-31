import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import path from "node:path";

const root = process.cwd();
const failures = [];

const files = {
  execution: read("src-tauri/src/services/proxy/execution.rs"),
  runtime: read("src-tauri/src/services/proxy/runtime.rs"),
  repository: read("src-tauri/src/services/proxy/routing_repository.rs"),
  upstream: read("src-tauri/src/services/proxy/upstream.rs"),
  endpointAdapter: read("src-tauri/src/services/proxy/endpoint_adapter.rs"),
  routingTypes: read("src-tauri/src/application/routing_engine/routing_types.rs"),
  schedulerMod: read("src-tauri/src/application/routing_engine/scheduler/mod.rs"),
};

checkDefaultV2ExecutionHasOneSelectorOwner();
checkDefaultV2ExecutionUsesLeasedController();
checkCredentialAndEndpointResolveLate();
checkNoSimulatedCapacityInDefaultV2();
checkFrontendDoesNotOwnRoutingTruth();

if (failures.length > 0) {
  console.error(failures.join("\n"));
  process.exit(1);
}

console.log("routing single-owner production gate passed");

function read(relativePath) {
  const absolute = path.join(root, ...relativePath.split("/"));
  assert.ok(existsSync(absolute), `${relativePath} must exist`);
  return readFileSync(absolute, "utf8");
}

function fail(relativePath, message) {
  failures.push(`${relativePath}: ${message}`);
}

function reject(source, relativePath, pattern, message) {
  if (pattern.test(stripRustTests(source))) {
    fail(relativePath, message);
  }
}

function require(source, relativePath, pattern, message) {
  if (!pattern.test(stripRustTests(source))) {
    fail(relativePath, message);
  }
}

function stripRustTests(source) {
  let stripped = "";
  let cursor = 0;
  const testModule = /#\[cfg\(test\)\]\s*mod\s+\w+\s*\{/g;
  for (let match = testModule.exec(source); match; match = testModule.exec(source)) {
    const start = match.index;
    const openBrace = testModule.lastIndex - 1;
    const end = findMatchingBrace(source, openBrace);
    if (end === -1) {
      break;
    }
    stripped += source.slice(cursor, start);
    cursor = end + 1;
    testModule.lastIndex = cursor;
  }
  stripped += source.slice(cursor);
  return stripped.replaceAll(
    /#\[cfg\(test\)\][\s\S]*?(?=\n(?:pub|pub\(crate\)|mod|use|const|fn|struct|enum|impl)\b|$)/g,
    "",
  );
}

function findMatchingBrace(source, openBrace) {
  let depth = 0;
  for (let index = openBrace; index < source.length; index += 1) {
    const char = source[index];
    if (char === "{") {
      depth += 1;
    } else if (char === "}") {
      depth -= 1;
      if (depth === 0) {
        return index;
      }
    }
  }
  return -1;
}

function checkDefaultV2ExecutionHasOneSelectorOwner() {
  const file = "src-tauri/src/services/proxy/execution.rs";
  reject(
    files.execution,
    file,
    /\bSchedulerRuntimeState\b|select_route_candidates_with_scheduler|router::select_route_candidates/u,
    "default-v2 execution must not import or call the legacy scheduler/router selector",
  );
  reject(
    files.execution,
    file,
    /\.load_runtime_candidates\(\)|\bRichRouteCandidate\b|\bRouteSelection\b/u,
    "default-v2 execution must consume the operational projection/controller input, not legacy RichRouteCandidate lists",
  );
  reject(
    files.execution,
    file,
    /\bselection\.accepted\b|for\s*\([^)]*candidate[^)]*\)\s+in\s+candidates\s*\.iter\(\)/u,
    "fallback must be controller-driven replan, not static traversal over an accepted candidate list",
  );
  require(
    files.execution,
    file,
    /\bRouteAdmissionController\b|\bControllerDecision\b/u,
    "default-v2 execution must use the RouteAdmissionController as the production selection owner",
  );
}

function checkDefaultV2ExecutionUsesLeasedController() {
  const file = "src-tauri/src/services/proxy/execution.rs";
  require(
    files.execution,
    file,
    /\bCompositeCapacityRegistry\b/u,
    "default-v2 execution must own real composite capacity leases",
  );
  require(
    files.execution,
    file,
    /\bSelectedRoute\b|\bLeasedSelectedTarget\b/u,
    "selected attempts must carry a controller/capacity lease into target resolution",
  );
  reject(
    files.execution,
    file,
    /max_concurrency:\s*candidate\.candidate\.max_concurrency|load_factor:\s*candidate\.candidate\.load_factor/u,
    "legacy max_concurrency/load_factor cannot be the production execution authority",
  );
}

function checkCredentialAndEndpointResolveLate() {
  reject(
    files.repository,
    "src-tauri/src/services/proxy/routing_repository.rs",
    /\bdecrypt_secret\b|\bruntime_candidate_api_key\b|EncryptedPayload|general_purpose::STANDARD/u,
    "routing repository must not decrypt station key credentials during candidate construction",
  );
  reject(
    files.repository,
    "src-tauri/src/services/proxy/routing_repository.rs",
    /\bRichRouteCandidate\b|\bRouteCandidate\b/u,
    "default-v2 repository must not assemble executable RouteCandidate DTOs",
  );
  reject(
    files.routingTypes,
    "src-tauri/src/application/routing_engine/routing_types.rs",
    /pub\(crate\)\s+api_key:\s*String|pub\(crate\)\s+upstream_base_url:\s*String/u,
    "routing engine executable candidate types must not carry plaintext credentials or full endpoint URLs",
  );
  require(
    files.execution + "\n" + files.runtime,
    "src-tauri/src/services/proxy/{execution.rs,runtime.rs}",
    /\bExecutionTargetResolver\b|\bExecutionTargetRef\b/u,
    "production proxy must resolve execution targets after controller selection",
  );
  require(
    files.execution + "\n" + files.runtime,
    "src-tauri/src/services/proxy/{execution.rs,runtime.rs}",
    /\bExecutionCredentialResolver\b|CredentialService\b/u,
    "production proxy must receive a narrow credential resolver for late secret resolution",
  );
}

function checkNoSimulatedCapacityInDefaultV2() {
  reject(
    files.schedulerMod,
    "src-tauri/src/application/routing_engine/scheduler/mod.rs",
    /acquired_simulated|slot_unavailable/u,
    "simulated scheduler capacity must be deleted or moved to an isolated non-default legacy owner",
  );
}

function checkFrontendDoesNotOwnRoutingTruth() {
  const frontendTruthFiles = [
    "src/lib/projections/pricingFacts.ts",
    "src/lib/projections/groupFacts.ts",
  ];
  for (const file of frontendTruthFiles) {
    const source = read(file);
    if (
      /\bfirstMatchingPricingRule\b|\bbuildPricingGroupCandidates\b|\bbuildCurrentStationGroupFacts\b/u.test(
        source,
      ) &&
      !/RPD_ROUTING_BOUNDARY:display-only-routing-truth-compat/u.test(source)
    ) {
      fail(
        file,
        "frontend projection matcher must either be deleted or explicitly marked display-only with a boundary owner",
      );
    }
  }
}
