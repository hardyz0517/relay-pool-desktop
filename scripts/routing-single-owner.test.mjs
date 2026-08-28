import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import path from "node:path";

const root = process.cwd();
const failures = [];

const files = {
  applicationRouting: read("src-tauri/src/application/routing.rs"),
  execution: read("src-tauri/src/services/proxy/execution.rs"),
  runtime: read("src-tauri/src/services/proxy/runtime.rs"),
  repository: read("src-tauri/src/services/proxy/routing_repository.rs"),
  executionReader: read("src-tauri/src/application/routing_execution_reader.rs"),
  upstream: read("src-tauri/src/services/proxy/upstream.rs"),
  endpointAdapter: read("src-tauri/src/services/proxy/endpoint_adapter.rs"),
  routingModels: read("src-tauri/src/models/routing.rs"),
  routingHealthTypescript: read("src-tauri/src/ipc/dto/routing_health_reads.typescript.txt"),
  routingEngineMod: read("src-tauri/src/application/routing_engine/mod.rs"),
  routingWorkspaceQuery: read("src-tauri/src/application/queries/routing_workspace.rs"),
  operationalDetailQuery: read("src-tauri/src/application/queries/operational_detail.rs"),
  routingRuntimeQuery: read("src-tauri/src/application/queries/routing_runtime.rs"),
  requestDecisionTrace: read("src-tauri/src/application/queries/request_decision_trace.rs"),
  policyDocumentRunner: read("src-tauri/src/background_tasks/policy_document_runner.rs"),
  routingPolicyControlPlane: read(
    "src-tauri/src/application/routing_policy_control_plane.rs",
  ),
  routingCommandFacade: read("src-tauri/src/application/command_facades/routing.rs"),
  modelMappingService: read("src-tauri/src/application/model_mapping_service.rs"),
};

checkDefaultV2ExecutionHasOneSelectorOwner();
checkApplicationRoutingCommandsUseHierarchicalPreview();
checkDefaultV2ExecutionUsesLeasedController();
checkCredentialAndEndpointResolveLate();
checkLegacyRoutingSchedulerDeleted();
checkRoutingWorkspaceUsesCanonicalFacts();
checkOperationalDetailUsesProjectionFacts();
checkDecisionTraceUsesDurableDecisions();
checkRuntimeOverlayUsesNarrowFacts();
checkSimulationDtoUsesPlannerProjectionLanguage();
checkFrontendDoesNotOwnRoutingTruth();
checkPolicyMutationControlPlane();
checkExecutionBridgeBoundary();
checkModelMappingOwner();

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
  // Only remove test modules. A `#[cfg(test)]` attribute can also guard a
  // single production helper or expression; consuming text until the next
  // `pub`/`fn` made this gate erase adjacent production methods and produce
  // false negatives.
  return stripped;
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
    /\bRouteAdmissionCoordinator\b|\bAdmissionDecision\b/u,
    "default-v2 execution must use the admission coordinator as the production selection owner",
  );
}

function checkApplicationRoutingCommandsUseHierarchicalPreview() {
  const file = "src-tauri/src/application/routing.rs";
  reject(
    files.applicationRouting,
    file,
    /\bSchedulerRuntimeState\b|select_route_candidates_with_scheduler|router::select_route_candidates/u,
    "routing commands and simulation previews must use the canonical snapshot planner, not the legacy scheduler/router selector",
  );
  require(
    files.applicationRouting,
    file,
    /\bload_intelligent_planning_snapshot\b/u,
    "routing simulation must load the canonical planning snapshot",
  );
  require(
    files.applicationRouting,
    file,
    /\bplan_snapshot_with_budget\b/u,
    "routing simulation must invoke the intelligent planner",
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
    files.routingModels,
    "src-tauri/src/models/routing.rs",
    /\bCanonicalRoutingCandidate\b[\s\S]*\bupstream_base_url\b/u,
    "runtime routing candidate DTO must carry sanitized_origin, not a full endpoint URL",
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
  reject(
    files.applicationRouting + "\n" + files.routingEngineMod,
    "src-tauri/src/application/{routing.rs,routing_engine/mod.rs}",
    /scheduler_group_binding_id|scheduler_group_id_hash|scheduler_group_type/u,
    "routing read models must use projected group facts, not legacy scheduler_group compatibility fields",
  );
}

function checkLegacyRoutingSchedulerDeleted() {
  const deletedPaths = [
    "src-tauri/src/application/routing_engine/router.rs",
    "src-tauri/src/application/routing_engine/routing_policy.rs",
    "src-tauri/src/application/routing_engine/scheduler/mod.rs",
  ];
  for (const relativePath of deletedPaths) {
    if (existsSync(path.join(root, ...relativePath.split("/")))) {
      fail(
        relativePath,
        "legacy routing scheduler/router files must stay physically deleted from the production module tree",
      );
    }
  }
  reject(
    files.routingEngineMod,
    "src-tauri/src/application/routing_engine/mod.rs",
    /\bmod\s+(router|routing_policy|scheduler)\s*;/u,
    "routing_engine module tree must not re-export the legacy scheduler/router selector",
  );
}

function checkRoutingWorkspaceUsesCanonicalFacts() {
  const file = "src-tauri/src/application/queries/routing_workspace.rs";
  require(
    files.routingWorkspaceQuery,
    file,
    /\bCanonicalRoutingCandidate\b[\s\S]*\bworkspace_snapshot_from_canonical_candidates\b/u,
    "routing workspace read model must consume canonical candidate facts through its dedicated read-model builder",
  );
  reject(
    files.routingWorkspaceQuery,
    file,
    /\bRouteCandidateProjection\b|\bRoutingWorkspaceProjectionCandidate\b|\bworkspace_snapshot_from_projection_candidates\b/u,
    "routing workspace read model must not depend on the legacy RouteCandidateProjection compatibility chain",
  );
}

function checkOperationalDetailUsesProjectionFacts() {
  const file = "src-tauri/src/application/queries/operational_detail.rs";
  reject(
    files.operationalDetailQuery,
    file,
    /\bCanonicalRoutingCandidate\b|\boperational_detail_from_runtime_candidate\b|\brouting_store\.runtime_candidate\b/u,
    "operational detail read model must consume RouteCandidateProjection facts, not rebuild runtime-candidate compatibility facts",
  );
  require(
    files.operationalDetailQuery,
    file,
    /\boperational_detail_from_projection\b[\s\S]*\bRouteCandidateProjection\b/u,
    "operational detail read model must keep the canonical projection-backed adapter",
  );
}

function checkDecisionTraceUsesDurableDecisions() {
  const file = "src-tauri/src/application/queries/request_decision_trace.rs";
  reject(
    files.requestDecisionTrace,
    file,
    /\bRequestLog\b|list_recent|legacy request log|request[_ ]log scan/u,
    "decision trace read model must not scan request logs or retain a legacy trace adapter",
  );
  require(
    files.applicationRouting + "\n" + files.requestDecisionTrace,
    "src-tauri/src/application/{routing.rs,queries/request_decision_trace.rs}",
    /RoutingDecisionQueries|route_decisions/u,
    "decision trace must consume the durable route_decisions read model",
  );
}

function checkRuntimeOverlayUsesNarrowFacts() {
  const file = "src-tauri/src/application/queries/routing_runtime.rs";
  reject(
    files.routingRuntimeQuery,
    file,
    /\bCanonicalRoutingCandidate\b/u,
    "runtime overlay read model must consume narrow runtime facts, not executable legacy candidates",
  );
  require(
    files.routingRuntimeQuery,
    file,
    /RoutingRuntimeCandidateFact/u,
    "runtime overlay read model must declare its narrow runtime fact boundary",
  );
}

function checkSimulationDtoUsesPlannerProjectionLanguage() {
  const routingModelsFile = "src-tauri/src/models/routing.rs";
  reject(
    files.routingModels,
    routingModelsFile,
    /\bpub\s+score\s*:|\bpub\s+scheduler_score\s*:|\bpub\s+scheduler_factors\s*:|\bpub\s+effective_multiplier_source\s*:|\bpub\s+effective_multiplier_confidence\s*:|\bpub\s+scheduler_error_code\s*:/u,
    "route simulation DTO must expose planner/projection facts, not legacy scheduler score, factor, multiplier-source or error-code fields",
  );
  require(
    files.routingModels,
    routingModelsFile,
    /\bpub\s+planner_error_code\s*:/u,
    "route simulation DTO must name planner rejection code as planner_error_code",
  );

  const routingHealthTypescriptFile = "src-tauri/src/ipc/dto/routing_health_reads.typescript.txt";
  reject(
    files.routingHealthTypescript,
    routingHealthTypescriptFile,
    /schedulerScore|schedulerFactors|effectiveMultiplierSource|effectiveMultiplierConfidence|schedulerErrorCode/u,
    "routing IPC DTO contract must not reintroduce legacy simulation scheduler fields",
  );
  require(
    files.routingHealthTypescript,
    routingHealthTypescriptFile,
    /plannerErrorCode:\s*string \| null/u,
    "routing IPC DTO contract must expose plannerErrorCode",
  );
}

function checkFrontendDoesNotOwnRoutingTruth() {
  const routingWorkspacePanelFile = "src/features/routing/RoutingStatusDiagnosticsPanel.tsx";
  const routingWorkspacePanel = read(routingWorkspacePanelFile);
  reject(
    routingWorkspacePanel,
    routingWorkspacePanelFile,
    /capabilitySummary|priceBasis|pricing unavailable/u,
    "routing status diagnostics UI must render backend operational snapshots without deriving fallback truth from compatibility fields",
  );
  const statusCandidateRowFile = "src/features/routing/LocalRoutingStatusCandidateRow.tsx";
  reject(
    read(statusCandidateRowFile),
    statusCandidateRowFile,
    /effectiveMultiplier|effectiveMultiplierSource|effectiveMultiplierConfidence|previewRejectReasons|schedulerRejectReason|formatMultiplierSource/u,
    "local routing status rows must render backend facts through the view model, not rebuild multiplier or rejection truth from compatibility DTO fields",
  );

  const frontendTruthFiles = [
    "src/lib/projections/pricingFacts.ts",
    "src/lib/projections/groupFacts.ts",
  ];
  for (const file of frontendTruthFiles) {
    const source = read(file);
    if (
      /\bderivePricingGroupDisplayCandidates\b|\bderiveStationGroupDisplayFacts\b/u.test(
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

function checkPolicyMutationControlPlane() {
  const runnerFile = "src-tauri/src/background_tasks/policy_document_runner.rs";
  reject(
    files.policyDocumentRunner,
    runnerFile,
    /\bPersistenceHandle\b|\bRoutingService\b|apply_routing_policy_document_v2|publish_transport_policy/u,
    "managed policy reconciliation must use the application control plane, not persistence or proxy activation directly",
  );
  require(
    files.policyDocumentRunner,
    runnerFile,
    /RoutingPolicyMutationCoordinator/u,
    "managed policy reconciliation must depend on the routing policy mutation coordinator",
  );
  require(
    files.routingPolicyControlPlane,
    "src-tauri/src/application/routing_policy_control_plane.rs",
    /apply_ui[\s\S]*reconcile_external/u,
    "the routing policy control plane must expose explicit UI and managed-document mutation entry points",
  );
}

function checkExecutionBridgeBoundary() {
  const repositoryFile = "src-tauri/src/services/proxy/routing_repository.rs";
  reject(
    files.repository,
    repositoryFile,
    /\bRoutingService\b|PLANNING_DEADLINE_EXCEEDED/u,
    "proxy execution repository must depend on the narrow execution port and typed errors, not the broad routing service or a deadline magic string",
  );
  require(
    files.repository,
    repositoryFile,
    /(?=[\s\S]*RoutingExecutionReadPort)(?=[\s\S]*RoutingExecutionReadError)/u,
    "proxy execution repository must declare both its narrow application port and typed bridge error",
  );
  require(
    files.executionReader,
    "src-tauri/src/application/routing_execution_reader.rs",
    /RoutingExecutionReadError[\s\S]*DeadlineExceeded[\s\S]*Unavailable[\s\S]*InvalidState/u,
    "execution bridge must expose stable deadline, unavailable and invalid-state outcomes",
  );
}

function checkModelMappingOwner() {
  const routingFile = "src-tauri/src/application/routing.rs";
  reject(
    files.applicationRouting,
    routingFile,
    /(?:apply|restore|load|list|reconcile)_model_mapping/u,
    "routing aggregate must not own model-mapping document persistence or history wrappers",
  );
  require(
    files.routingCommandFacade,
    "src-tauri/src/application/command_facades/routing.rs",
    /model_mapping:\s*Arc<ModelMappingService>/u,
    "routing command facade must inject the dedicated model-mapping owner",
  );
  require(
    files.routingCommandFacade,
    "src-tauri/src/application/command_facades/routing.rs",
    /self\.model_mapping\.(?:apply_document|restore_document|load_history_document|list_legacy_reviews|reconcile_document_sync)/u,
    "model-mapping commands must call the dedicated model-mapping owner",
  );
  require(
    files.modelMappingService,
    "src-tauri/src/application/model_mapping_service.rs",
    /persist_document|persist_document_at_revision|reconcile_model_mapping_document_sync/u,
    "model-mapping persistence and document reconciliation must have one application owner",
  );
}
