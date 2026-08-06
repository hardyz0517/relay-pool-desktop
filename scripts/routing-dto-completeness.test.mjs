import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const requestSource = await readFile("src-tauri/src/application/routing_engine/request.rs", "utf8");
const candidateProjectorSource = await readFile(
  "src-tauri/src/application/operational_facts/candidate_projector.rs",
  "utf8",
);
const routeCandidateProjectionTest = await readFile(
  "src-tauri/tests/route_candidate_projection.rs",
  "utf8",
);

function structBody(source, name) {
  const match = source.match(new RegExp(`struct ${name} \\{([\\s\\S]*?)\\n\\}`));
  assert.ok(match, `missing struct ${name}`);
  return match[1];
}

const requestFactsBody = structBody(requestSource, "RouteRequestFacts");
for (const forbidden of [
  "actual_attempt_exclusions",
  "remaining",
  "round_clock",
  "ordinal",
  "attempt_count",
  "snapshot_rebuild_count",
  "runtime_rebuild_count",
]) {
  assert.ok(
    !requestFactsBody.includes(forbidden),
    `RouteRequestFacts must not own mutable progress field ${forbidden}`,
  );
}

const progressBody = structBody(requestSource, "RouteProgress");
for (const required of [
  "ordinal",
  "actual_attempt_exclusions",
  "deadline_ms",
  "attempt_count",
  "snapshot_rebuild_count",
  "runtime_rebuild_count",
]) {
  assert.ok(progressBody.includes(required), `RouteProgress should own ${required}`);
}

assert.match(
  requestSource,
  /pub(?:\(crate\))? fn classify\([\s\S]*?request: CanonicalRouteRequest[\s\S]*?settings: ValidatedLocalRouteSettings/,
  "RouteRequestClassifier should combine canonical request and validated local settings",
);
const classifyBody = requestSource.match(
  /pub(?:\(crate\))? fn classify\([\s\S]*?\) -> RouteRequestFacts \{([\s\S]*?)\n    \}/,
)?.[1];
assert.ok(classifyBody, "missing RouteRequestClassifier::classify body");
assert.ok(
  !classifyBody.includes("untrusted_headers"),
  "untrusted headers must not feed local ordering profile",
);

const projectionBody = structBody(candidateProjectorSource, "RouteCandidateProjection");
const requiredProjectionFields = [
  "identity",
  "route_kind",
  "requested_model",
  "resolved_model",
  "policy",
  "group",
  "multiplier",
  "pricing",
  "balance",
  "capability",
  "health",
  "capacity",
  "provenance",
  "hard_rejection_codes",
];
for (const field of requiredProjectionFields) {
  assert.ok(projectionBody.includes(field), `RouteCandidateProjection missing ${field}`);
  assert.match(
    routeCandidateProjectionTest,
    new RegExp(`projection\\.${field}|${field}:`),
    `route_candidate_projection test should explicitly cover ${field}`,
  );
}

assert.ok(
  !candidateProjectorSource.includes("Default for RouteCandidateProjection") &&
    !routeCandidateProjectionTest.includes("Default::default()"),
  "candidate projection must not use silent Default::default fixtures",
);

for (const forbidden of [
  "api_key",
  "encrypted",
  "upstream_base_url",
  "SecretManager",
  "registry",
  "reqwest",
  "sqlx::",
]) {
  assert.ok(
    !candidateProjectorSource.includes(forbidden),
    `RouteCandidateProjector must not depend on secret/io/mutable owner: ${forbidden}`,
  );
}

assert.match(
  candidateProjectorSource,
  /ROUTE_CANDIDATE_PROJECTION_VERSION/,
  "candidate projection should carry an explicit projector version",
);
assert.match(
  candidateProjectorSource,
  /pricing_not_applicable_for_inference/,
  "inference must not accept NotApplicable pricing",
);
assert.match(
  candidateProjectorSource,
  /RouteKind::ModelCatalog/,
  "catalog request kind should stay distinct from inference",
);

console.log("routing dto completeness contract passed");
