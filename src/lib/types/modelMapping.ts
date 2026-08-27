import type {
  ApplyModelMappingDocumentInputDto,
  ModelMappingActionDto,
  ModelMappingBindingDto,
  ModelMappingDiagnosticDto,
  ModelMappingDocumentDto,
  ModelMappingLegacyReviewDto,
  ModelMappingProfileDto,
  ModelMappingRuleDto,
  ModelMappingSimulationResultDto,
  ModelMappingTraceDto,
  ModelMappingTargetRefDto,
  ModelMappingValidationResultDto,
  ModelMappingWorkspaceDto,
  RestoreModelMappingRevisionInputDto,
  SimulateModelMappingInputDto,
  ValidateModelMappingDocumentInputDto,
} from "@/lib/bridge/generated";

export type {
  ApplyModelMappingDocumentInputDto,
  ModelMappingActionDto,
  ModelMappingBindingDto,
  ModelMappingDiagnosticDto,
  ModelMappingDocumentDto,
  ModelMappingLegacyReviewDto,
  ModelMappingProfileDto,
  ModelMappingRuleDto,
  ModelMappingSimulationResultDto,
  ModelMappingTraceDto,
  ModelMappingTargetRefDto,
  ModelMappingValidationResultDto,
  ModelMappingWorkspaceDto,
  RestoreModelMappingRevisionInputDto,
  SimulateModelMappingInputDto,
  ValidateModelMappingDocumentInputDto,
};

/**
 * Rebuild a document with only the fields accepted by the apply IPC DTO.
 * Workspace responses may outlive the frontend contract during an app update;
 * forwarding those response objects verbatim would be rejected by Rust's
 * `deny_unknown_fields` decoder.
 */
export function toModelMappingApplyDocument(
  document: ModelMappingDocumentDto,
): ModelMappingDocumentDto {
  return {
    formatVersion: document.formatVersion,
    baseRevision: document.baseRevision,
    policy: {
      unmatchedModelBehavior: document.policy.unmatchedModelBehavior,
    },
    rules: document.rules.map((rule) => ({
      id: rule.id,
      priority: rule.priority,
      enabled: rule.enabled,
      matcher: normalizeMatcher(rule.matcher),
      conditions: {
        endpointKinds: [...rule.conditions.endpointKinds],
        stream: rule.conditions.stream,
        tools: rule.conditions.tools,
        vision: rule.conditions.vision,
        reasoning: rule.conditions.reasoning,
      },
      action: normalizeAction(rule.action),
      note: rule.note,
      revision: rule.revision,
      createdAtMs: rule.createdAtMs,
      updatedAtMs: rule.updatedAtMs,
    })),
    profiles: document.profiles.map((profile) => ({
      id: profile.id,
      canonicalModel: profile.canonicalModel,
      displayName: profile.displayName,
      defaultUpstreamModel: profile.defaultUpstreamModel,
      status: profile.status,
      note: profile.note,
      revision: profile.revision,
      createdAtMs: profile.createdAtMs,
      updatedAtMs: profile.updatedAtMs,
    })),
    bindings: document.bindings.map((binding) => ({
      id: binding.id,
      modelProfileId: binding.modelProfileId,
      stationId: binding.stationId,
      stationKeyId: binding.stationKeyId,
      upstreamModel: binding.upstreamModel,
      source: binding.source,
      enabled: binding.enabled,
      note: binding.note,
      revision: binding.revision,
      createdAtMs: binding.createdAtMs,
      updatedAtMs: binding.updatedAtMs,
    })),
  };
}

function normalizeMatcher(
  matcher: ModelMappingRuleDto["matcher"],
): ModelMappingRuleDto["matcher"] {
  switch (matcher.kind) {
    case "exact":
      return { kind: "exact", model: matcher.model };
    case "glob":
      return { kind: "glob", pattern: matcher.pattern };
    case "default":
      return { kind: "default" };
  }
}

function normalizeTarget(
  target: ModelMappingTargetRefDto,
): ModelMappingTargetRefDto {
  return target.kind === "literal"
    ? { kind: "literal", upstreamModel: target.upstreamModel }
    : { kind: "model_profile", modelProfileId: target.modelProfileId };
}

function normalizeAction(
  action: ModelMappingActionDto,
): ModelMappingActionDto {
  switch (action.kind) {
    case "map_fixed":
      return { kind: "map_fixed", target: normalizeTarget(action.target) };
    case "preserve":
      return { kind: "preserve" };
    case "reject":
      return {
        kind: "reject",
        rejectionKind: action.rejectionKind,
        message: action.message,
      };
    case "map_fallback_chain":
      return {
        kind: "map_fallback_chain",
        targets: action.targets.map(normalizeTarget),
        fallbackTrigger: action.fallbackTrigger,
      };
  }
}
