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
  routingRuntimeQuery: read("src-tauri/src/application/queries/routing_runtime.rs"),
  requestDecisionTrace: read("src-tauri/src/application/queries/request_decision_trace.rs"),
  policyDocumentRunner: read("src-tauri/src/background_tasks/policy_document_runner.rs"),
  routingPolicyControlPlane: read(
    "src-tauri/src/application/routing_policy_control_plane.rs",
  ),
  routingCommandFacade: read("src-tauri/src/application/command_facades/routing.rs"),
  modelMappingService: read("src-tauri/src/application/model_mapping_service.rs"),
  operationalFactsQuery: read(
    "src-tauri/src/persistence/stores/operational_facts/queries.rs",
  ),
  operationalPlanning: read(
    "src-tauri/src/application/operational_facts/planning_snapshot.rs",
  ),
  enginePlanning: read(
    "src-tauri/src/application/routing_engine/planning_snapshot.rs",
  ),
  candidatePlan: read(
    "src-tauri/src/application/routing_engine/candidate_plan.rs",
  ),
  admission: read("src-tauri/src/application/routing_engine/admission.rs"),
  targetResolver: read(
    "src-tauri/src/application/operational_facts/target_resolver.rs",
  ),
  openAiAdapter: read("src-tauri/src/services/proxy/adapters/openai.rs"),
  routingStore: read("src-tauri/src/persistence/stores/routing_store.rs"),
  routingProtectionQuery: read(
    "src-tauri/src/application/queries/routing_protection.rs",
  ),
  mainWindowAcl: read("src-tauri/permissions/main-window.toml"),
  compiledAcl: read("src-tauri/gen/schemas/acl-manifests.json"),
  ipcRegistry: read("src-tauri/src/ipc/registry.rs"),
  generatedBridge: read("src/lib/bridge/generated.ts"),
  backendClient: read("src/lib/bridge/BackendClient.ts"),
  desktopBackend: read("src/lib/bridge/DesktopBackend.ts"),
  demoBackend: read("src/lib/bridge/DemoBackend.ts"),
  stationsApi: read("src/lib/api/stations.ts"),
  stationsTypes: read("src/lib/types/stations.ts"),
  observationIngestion: read("src-tauri/src/application/observation_ingestion.rs"),
  monitoringService: read("src-tauri/src/application/monitoring/service.rs"),
  monitoringWritePath: read("src-tauri/src/application/monitoring/write_path.rs"),
  requestFinalization: read("src-tauri/src/application/request_finalization/mod.rs"),
};

checkDefaultV2ExecutionHasOneSelectorOwner();
checkApplicationRoutingCommandsUseHierarchicalPreview();
checkDefaultV2ExecutionUsesLeasedController();
checkCredentialAndEndpointResolveLate();
checkLegacyRoutingSchedulerDeleted();
checkRoutingWorkspaceUsesCanonicalFacts();
checkDecisionTraceUsesDurableDecisions();
checkRuntimeOverlayUsesNarrowFacts();
checkSimulationDtoUsesPlannerProjectionLanguage();
checkRetryActionTraceCutover();
checkFrontendDoesNotOwnRoutingTruth();
checkPolicyMutationControlPlane();
checkExecutionBridgeBoundary();
checkModelMappingOwner();
checkRetiredProtectionAndCapacityDomainStayOutOfProductionRouting();

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
    /\bplan_snapshot(?:_with_budget)?\b/u,
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

function checkRetryActionTraceCutover() {
  const executionFile = "src-tauri/src/services/proxy/execution.rs";
  require(
    files.execution,
    executionFile,
    /enum\s+RetryActionKind\s*\{[\s\S]*RetryCurrentKey,[\s\S]*TryNextKey,[\s\S]*StopRequest,[\s\S]*\}/u,
    "production retry actions must distinguish current-key retry, next-key failover and stop-request",
  );
  reject(
    files.execution,
    executionFile,
    /Self::(?:RetrySameTarget|WaitThenReplan|TryDifferentFailureDomain)\s*=>\s*"/u,
    "production trace producers must not emit pre-cutover retry action labels",
  );

  const traceFile = "src-tauri/src/application/queries/request_decision_trace.rs";
  require(
    files.requestDecisionTrace,
    traceFile,
    /RetryCurrentKey,[\s\S]*TryNextKey,[\s\S]*StopRequest,[\s\S]*Read-only compatibility values[\s\S]*LegacyRetrySameTarget[\s\S]*LegacyWaitThenReplan[\s\S]*LegacyTryDifferentFailureDomain/u,
    "trace reads must expose current-key and next-key actions while marking pre-cutover values as read-only compatibility",
  );
  require(
    files.routingHealthTypescript,
    "src-tauri/src/ipc/dto/routing_health_reads.typescript.txt",
    /RequestDecisionActionDto\s*=\s*"retry_current_key"\s*\|\s*"try_next_key"\s*\|\s*"stop_request"/u,
    "routing IPC must expose the production current-key, next-key and stop-request actions",
  );
  require(
    files.routingHealthTypescript,
    "src-tauri/src/ipc/dto/routing_health_reads.typescript.txt",
    /read-only compatibility for pre-v3 trace history[\s\S]*remainingPrecommitBudgetMs/u,
    "routing IPC must mark legacy actions as read-only and name the request deadline budget accurately",
  );
  reject(
    files.routingHealthTypescript,
    "src-tauri/src/ipc/dto/routing_health_reads.typescript.txt",
    /remainingWaitBudgetMs|failureDomain:\s*string \| null/u,
    "routing trace IPC must not expose the removed wait-action or failure-domain fields",
  );
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

function checkRetiredProtectionAndCapacityDomainStayOutOfProductionRouting() {
  const capacityDomainFreeFiles = [
    [
      "src-tauri/src/persistence/stores/operational_facts/queries.rs",
      files.operationalFactsQuery,
    ],
    [
      "src-tauri/src/application/operational_facts/planning_snapshot.rs",
      files.operationalPlanning,
    ],
    [
      "src-tauri/src/application/routing_engine/planning_snapshot.rs",
      files.enginePlanning,
    ],
    [
      "src-tauri/src/application/routing_engine/candidate_plan.rs",
      files.candidatePlan,
    ],
    ["src-tauri/src/application/routing_engine/admission.rs", files.admission],
    [
      "src-tauri/src/application/operational_facts/target_resolver.rs",
      files.targetResolver,
    ],
    ["src-tauri/src/services/proxy/routing_repository.rs", files.repository],
    ["src-tauri/src/services/proxy/upstream.rs", files.upstream],
  ];
  for (const [file, source] of capacityDomainFreeFiles) {
    reject(
      source,
      file,
      /\b(?:ProviderCapacityDomain|CapacityDomainCommitment|capacity_domain|expected_capacity_domain|trusted_capacity_domain_commitment)\b/u,
      "production planning, admission, target resolution and execution facts must not read or propagate capacity-domain identity",
    );
  }

  reject(
    files.execution,
    "src-tauri/src/services/proxy/execution.rs",
    /\b(?:ProviderCapacityDomain|CapacityDomainCommitment|expected_capacity_domain|trusted_capacity_domain_commitment|allow_cross_capacity_domain_fallback|candidate_health_scopes|acquire_health_probe|load_health_protection_statuses|begin_health_protection_probe|cancel_health_protection_probe)\b/u,
    "production execution must use only the station-key circuit and must not restore scoped health or capacity-domain fallback",
  );
  reject(
    files.openAiAdapter,
    "src-tauri/src/services/proxy/adapters/openai.rs",
    /\bcapacity_domain_commitment\b/u,
    "the upstream classifier must not accept a capacity-domain commitment",
  );
  reject(
    files.operationalPlanning,
    "src-tauri/src/application/operational_facts/planning_snapshot.rs",
    /candidate_scoped_admitted|candidate_model_scoped_admitted|scoped_subjects_for_planning|scoped_admission_verdict/u,
    "the production planner must not consume the retired scoped health or error-rate breaker",
  );

  const targetQuery = files.routingStore.match(
    /const OPERATIONAL_EXECUTION_TARGET_REFS_QUERY_PREFIX:[\s\S]*?impl RoutingStore/u,
  )?.[0];
  if (!targetQuery) {
    fail(
      "src-tauri/src/persistence/stores/routing_store.rs",
      "operational execution-target query boundary must remain discoverable",
    );
  } else if (/station_capacity_domains|capacity_domain/u.test(targetQuery)) {
    fail(
      "src-tauri/src/persistence/stores/routing_store.rs",
      "execution-target lookup must not join or select station capacity-domain identity",
    );
  }

  const runtimeCandidateQuery = files.routingStore.match(
    /pub\(crate\) async fn load_runtime_candidates[\s\S]*?\n    pub\(crate\) async fn /u,
  )?.[0];
  if (!runtimeCandidateQuery) {
    fail(
      "src-tauri/src/persistence/stores/routing_store.rs",
      "runtime-candidate query boundary must remain discoverable",
    );
  } else if (/station_capacity_domains|capacity_domain/u.test(runtimeCandidateQuery)) {
    fail(
      "src-tauri/src/persistence/stores/routing_store.rs",
      "runtime-candidate loading must not join, select or map station capacity-domain identity",
    );
  }

  const rejectionOwners = [
    [
      "src-tauri/src/application/operational_facts/candidate_projector.rs",
      read("src-tauri/src/application/operational_facts/candidate_projector.rs"),
    ],
    [
      "src-tauri/src/application/queries/routing_workspace.rs",
      files.routingWorkspaceQuery,
    ],
    [
      "src-tauri/src/application/queries/routing_protection.rs",
      files.routingProtectionQuery,
    ],
  ];
  for (const [file, source] of rejectionOwners) {
    reject(
      source,
      file,
      /["']capacity_unavailable["']/u,
      "production capacity-state failures must use capacity_state_unavailable; the old literal is compatibility-only",
    );
  }

  const retiredCommandPattern =
    /(?:get|upsert|clear)_station_capacity_domain|(?:get|upsert|clear)StationCapacityDomain/u;
  for (const [file, source] of [
    ["src-tauri/permissions/main-window.toml", files.mainWindowAcl],
    ["src-tauri/gen/schemas/acl-manifests.json", files.compiledAcl],
    ["src-tauri/src/ipc/registry.rs", files.ipcRegistry],
    ["src-tauri/generated/command-registry.json", read("src-tauri/generated/command-registry.json")],
    ["src/lib/bridge/generated.ts", files.generatedBridge],
    ["src/lib/bridge/BackendClient.ts", files.backendClient],
    ["src/lib/bridge/DesktopBackend.ts", files.desktopBackend],
    ["src/lib/bridge/DemoBackend.ts", files.demoBackend],
    ["src/lib/api/stations.ts", files.stationsApi],
    ["src/lib/types/stations.ts", files.stationsTypes],
  ]) {
    reject(
      source,
      file,
      retiredCommandPattern,
      "the retired capacity-domain control surface must not be reachable from the main renderer or generated API",
    );
  }

  const retiredErrorRateHistoryPattern =
    /list_error_rate_history|listErrorRateHistory|HealthProtectionScopeKindDto|ErrorRateHistory(?:Input|Page|Event)Dto|list_station_key_health|listStationKeyHealth|get_station_key_health|getStationKeyHealth|get_station_key_operational_detail|getStationKeyOperationalDetail/u;
  for (const [file, source] of [
    ["src-tauri/permissions/main-window.toml", files.mainWindowAcl],
    ["src-tauri/gen/schemas/acl-manifests.json", files.compiledAcl],
    ["src-tauri/src/ipc/registry.rs", files.ipcRegistry],
    ["src-tauri/generated/command-registry.json", read("src-tauri/generated/command-registry.json")],
    ["src-tauri/src/ipc/dto/routing_health_reads.typescript.txt", files.routingHealthTypescript],
    ["src/lib/bridge/generated.ts", files.generatedBridge],
    ["src/lib/bridge/BackendClient.ts", files.backendClient],
    ["src/lib/bridge/DesktopBackend.ts", files.desktopBackend],
    ["src/lib/bridge/DemoBackend.ts", files.demoBackend],
  ]) {
    reject(
      source,
      file,
      retiredErrorRateHistoryPattern,
      "retired routing diagnostics surfaces must not remain reachable from the main renderer or generated API",
    );
  }

  for (const [file, source] of [
    ["src-tauri/src/application/queries/routing_workspace.rs", files.routingWorkspaceQuery],
    ["src-tauri/src/application/queries/routing_protection.rs", files.routingProtectionQuery],
    ["src-tauri/src/application/routing.rs", files.applicationRouting],
    ["src-tauri/src/ipc/dto/routing_health_reads.typescript.txt", files.routingHealthTypescript],
    ["src/lib/bridge/generated.ts", files.generatedBridge],
  ]) {
    reject(
      source,
      file,
      /\b(?:RoutingFailureDomain|FailureDomainCandidateFact|ProviderCapacityDomain|failure_domains|failureDomain|capacity_provider_family|capacity_deployment_identity|capacity_region_identity|capacity_domain_revision)\b/u,
      "production workspace/protection DTOs must not expose capacity-domain identity",
    );
  }

  for (const [file, source] of [
    ["src-tauri/src/application/observation_ingestion.rs", files.observationIngestion],
    ["src-tauri/src/application/monitoring/service.rs", files.monitoringService],
    ["src-tauri/src/application/monitoring/write_path.rs", files.monitoringWritePath],
    ["src-tauri/src/application/request_finalization/mod.rs", files.requestFinalization],
  ]) {
    reject(
      source,
      file,
      /pub\(crate\) fn new\([^)]*\)[^{]*\{[\s\S]{0,400}(?:Self::new_with_error_rate|ObservationIngestion::with_error_rate)/u,
      "production constructors must not compose the retired ErrorRateProtectionService",
    );
  }

  require(
    files.routingWorkspaceQuery,
    "src-tauri/src/application/queries/routing_workspace.rs",
    /RoutingAvailabilityStatus[\s\S]*CapacityExhausted[\s\S]*CapacityStateUnavailable[\s\S]*AllKeysUnavailable/u,
    "workspace diagnostics must distinguish capacity exhaustion, capacity-state failure and all-keys-unavailable",
  );
  require(
    files.routingWorkspaceQuery + "\n" + files.routingHealthTypescript,
    "routing workspace Rust/TypeScript diagnostics",
    /RoutingMonitoringSourceStatus[\s\S]*(?:NoEvidence|no_evidence)[\s\S]*(?:Incomparable|incomparable)[\s\S]*(?:WeightZero|weight_zero)/u,
    "monitoring diagnostics must distinguish no evidence, incomparable evidence and zero source weight",
  );
}
