import type {
  ModelAliasDto,
  PricingGroupTypeDto,
  RecentRouteDecisionsInputDto,
  RecentRouteDecisionsPageDto,
  RequestDecisionTraceDto,
  RouteEndpointKindDto,
  RouteSimulationInputDto,
  RouteSimulationResultDto,
  RoutingGroupFilterDto,
  RoutingCandidateParticipationReasonDto,
  RoutingCandidateParticipationStatusDto,
  RoutingPlannerEvaluationStatusDto,
  RoutingScoreStatusDto,
  RoutingRuntimeOverlayDto,
  RoutingRuntimeCandidateOverlayDto,
  RoutingProtectionStatusDto,
  RoutingProtectionStatusInputDto,
  RoutingWorkspaceCandidateDto,
  RoutingWorkspaceAggregatesDto,
  RoutingWorkspaceSnapshotDto,
  RoutingWorkspaceSnapshotInputDto,
  StationKeyCapabilitiesDto,
  UpdateStationKeyCapabilitiesInputDto,
  UpsertModelAliasInputDto,
  RoutingPolicyConfigV3Dto,
  RoutingPolicyPublicationStatusDto,
  RoutingPolicyPublicationStatusInputDto,
  RoutingPolicySnapshotDto,
  ApplyRoutingPolicyDocumentInputDto,
} from "@/lib/bridge/generated";

/**
 * Describes how a persisted policy revision reached (or will reach) the
 * process runtime.  These fields are optional while older desktop binaries
 * are still able to answer the routing IPC commands; callers must treat a
 * missing value as the legacy generation lane.
 */
export type RoutingPolicyActivationPath = "fast" | "generation" | "persisted_only";

/** Runtime publication state exposed by the routing read model. */
export type RoutingPolicyRuntimeStatus =
  | "active"
  | "staged"
  | "ready"
  | "waiting_latest_input"
  | "failed"
  | "expired";

/**
 * Low-cardinality reason for a fast-activation fallback.  Keep this as a
 * string at the UI boundary so a newer backend can add a reason without
 * breaking an older frontend; presentation maps known values to safe copy
 * and deliberately never renders an arbitrary backend detail.
 */
export type RoutingPolicyFallbackReason = string;

export type RoutingPolicyPublicationMetadata = {
  activationPath?: RoutingPolicyActivationPath | null;
  runtimeStatus?: RoutingPolicyRuntimeStatus | null;
  activeRevision?: number | null;
  fallbackReason?: RoutingPolicyFallbackReason | null;
};

export type RoutingPolicyConfigV3 = RoutingPolicyConfigV3Dto;
export type RoutingPolicySnapshot = Omit<RoutingPolicySnapshotDto, keyof RoutingPolicyPublicationMetadata> &
  RoutingPolicyPublicationMetadata;
export type ApplyRoutingPolicyDocumentInput = ApplyRoutingPolicyDocumentInputDto;
export type RoutingPolicyPublicationStatusInput = RoutingPolicyPublicationStatusInputDto;
export type RoutingPolicyPublicationStatus = Omit<RoutingPolicyPublicationStatusDto, "status" | keyof RoutingPolicyPublicationMetadata> &
  RoutingPolicyPublicationMetadata & {
    status: RoutingPolicyRuntimeStatus;
  };
export type RouteEndpointKind = RouteEndpointKindDto;
export type PricingGroupType = PricingGroupTypeDto;
export type RoutingGroupFilter = RoutingGroupFilterDto;

export type StationKeyCapabilities = StationKeyCapabilitiesDto;
export type UpdateStationKeyCapabilitiesInput = UpdateStationKeyCapabilitiesInputDto;

export type ModelAlias = ModelAliasDto;
export type UpsertModelAliasInput = UpsertModelAliasInputDto;

export type RouteSimulationInput = Omit<
  RouteSimulationInputDto,
  "maxRateMultiplier" | "routingGroupFilter" | "sessionHash" | "previousResponseId"
> & {
  maxRateMultiplier?: RouteSimulationInputDto["maxRateMultiplier"];
  routingGroupFilter?: RouteSimulationInputDto["routingGroupFilter"];
  sessionHash?: RouteSimulationInputDto["sessionHash"];
  previousResponseId?: RouteSimulationInputDto["previousResponseId"];
};
export type RouteSimulationResult = RouteSimulationResultDto;

export type RoutingWorkspaceSnapshotInput = RoutingWorkspaceSnapshotInputDto;
export type RoutingPlannerEvaluationStatus = RoutingPlannerEvaluationStatusDto;
export type RoutingScoreStatus = RoutingScoreStatusDto;
export type RoutingCandidateParticipationStatus = RoutingCandidateParticipationStatusDto;
export type RoutingCandidateParticipationReason = RoutingCandidateParticipationReasonDto;
export type RoutingWorkspaceCandidate = RoutingWorkspaceCandidateDto;
export type RoutingWorkspaceAggregates = RoutingWorkspaceAggregatesDto;
export type RoutingWorkspaceSnapshot = RoutingWorkspaceSnapshotDto;

export type RoutingRuntimeOverlay = Omit<RoutingRuntimeOverlayDto, "candidates"> & {
  candidates: Array<Omit<RoutingRuntimeCandidateOverlayDto, "healthState" | "cooldownUntil">>;
};
export type RoutingProtectionStatus = RoutingProtectionStatusDto;
export type RoutingProtectionStatusInput = RoutingProtectionStatusInputDto;
export type RecentRouteDecisionsInput = RecentRouteDecisionsInputDto;
export type RecentRouteDecisionsPage = RecentRouteDecisionsPageDto;

export type RequestDecisionTrace = RequestDecisionTraceDto;
