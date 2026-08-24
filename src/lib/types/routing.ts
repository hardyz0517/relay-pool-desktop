import type {
  ModelAliasDto,
  OperationalDetailFactDto,
  PricingGroupTypeDto,
  RecentRouteDecisionsInputDto,
  RecentRouteDecisionsPageDto,
  RecentRouteDecisionSummaryDto,
  RequestDecisionTraceDto,
  RouteCandidateExplanationDto,
  RouteEndpointKindDto,
  RouteSimulationInputDto,
  RouteSimulationResultDto,
  RoutingCandidateCapacitySnapshotDto,
  RoutingCandidateSourceRefsDto,
  RoutingCapacityReadModeDto,
  RoutingCapabilitySummaryDto,
  RoutingGroupFilterDto,
  RoutingPolicyDto,
  RoutingReadModelStatusDto,
  RoutingReadPageDto,
  RoutingRuntimeCandidateOverlayDto,
  RoutingRuntimeOverlayDto,
  RoutingProtectionStatusDto,
  RoutingProtectionStatusInputDto,
  ErrorRateHistoryInputDto,
  ErrorRateHistoryPageDto,
  RoutingWorkspaceCandidateDto,
  RoutingWorkspaceSnapshotDto,
  RoutingWorkspaceSnapshotInputDto,
  DispatchAlgorithmSettingsDto,
  StationKeyCapabilitiesDto,
  StationKeyHealthDto,
  StationKeyOperationalDetailDto,
  UpdateStationKeyCapabilitiesInputDto,
  UpsertModelAliasInputDto,
  RoutingPolicyConfigV1Dto,
  RoutingPolicyConfigV2Dto,
  RoutingPolicySnapshotDto,
  ApplyRoutingPolicyDocumentInputDto,
} from "@/lib/bridge/generated";

export type RoutingPolicy = RoutingPolicyDto;
export type RoutingPolicyConfigV1 = RoutingPolicyConfigV1Dto;
export type RoutingPolicyConfigV2 = RoutingPolicyConfigV2Dto;
export type RoutingPolicySnapshot = RoutingPolicySnapshotDto;
export type ApplyRoutingPolicyDocumentInput = ApplyRoutingPolicyDocumentInputDto;
export type RouteEndpointKind = RouteEndpointKindDto;
export type PricingGroupType = PricingGroupTypeDto;
export type RoutingGroupFilter = RoutingGroupFilterDto;
export type DispatchAlgorithmSettings = DispatchAlgorithmSettingsDto;

export type StationKeyCapabilities = StationKeyCapabilitiesDto;
export type UpdateStationKeyCapabilitiesInput = UpdateStationKeyCapabilitiesInputDto;

export type ModelAlias = ModelAliasDto;
export type UpsertModelAliasInput = UpsertModelAliasInputDto;

export type StationKeyHealth = StationKeyHealthDto;

export type RouteSimulationInput = Omit<
  RouteSimulationInputDto,
  "maxRateMultiplier" | "routingGroupFilter" | "sessionHash" | "previousResponseId"
> & {
  maxRateMultiplier?: RouteSimulationInputDto["maxRateMultiplier"];
  routingGroupFilter?: RouteSimulationInputDto["routingGroupFilter"];
  sessionHash?: RouteSimulationInputDto["sessionHash"];
  previousResponseId?: RouteSimulationInputDto["previousResponseId"];
};
export type RouteCandidateExplanation = RouteCandidateExplanationDto;
export type RouteSimulationResult = RouteSimulationResultDto;

export type RoutingWorkspaceSnapshotInput = RoutingWorkspaceSnapshotInputDto;
export type RoutingCapacityReadMode = RoutingCapacityReadModeDto;
export type RoutingReadModelStatus = RoutingReadModelStatusDto;
export type RoutingCapabilitySummary = RoutingCapabilitySummaryDto;
export type RoutingCandidateCapacitySnapshot = RoutingCandidateCapacitySnapshotDto;
export type RoutingCandidateSourceRefs = RoutingCandidateSourceRefsDto;
export type RoutingWorkspaceCandidate = RoutingWorkspaceCandidateDto;
export type RoutingReadPage = RoutingReadPageDto;
export type RoutingWorkspaceSnapshot = RoutingWorkspaceSnapshotDto;

export type RoutingRuntimeCandidateOverlay = RoutingRuntimeCandidateOverlayDto;
export type RoutingRuntimeOverlay = RoutingRuntimeOverlayDto;
export type RoutingProtectionStatus = RoutingProtectionStatusDto;
export type RoutingProtectionStatusInput = RoutingProtectionStatusInputDto;
export type ErrorRateHistoryInput = ErrorRateHistoryInputDto;
export type ErrorRateHistoryPage = ErrorRateHistoryPageDto;

export type RecentRouteDecisionsInput = RecentRouteDecisionsInputDto;
export type RouteDecisionSummary = RecentRouteDecisionSummaryDto;
export type RecentRouteDecisionsPage = RecentRouteDecisionsPageDto;

export type OperationalDetailFact = OperationalDetailFactDto;
export type StationKeyOperationalDetail = StationKeyOperationalDetailDto;
export type RequestDecisionTrace = RequestDecisionTraceDto;
