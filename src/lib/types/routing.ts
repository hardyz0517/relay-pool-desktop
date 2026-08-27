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
  RoutingPlannerEvaluationStatusDto,
  RoutingScoreStatusDto,
  RoutingRuntimeOverlayDto,
  RoutingProtectionStatusDto,
  RoutingProtectionStatusInputDto,
  RoutingWorkspaceCandidateDto,
  RoutingWorkspaceSnapshotDto,
  RoutingWorkspaceSnapshotInputDto,
  DispatchAlgorithmSettingsDto,
  StationKeyCapabilitiesDto,
  StationKeyHealthDto,
  StationKeyOperationalDetailDto,
  UpdateStationKeyCapabilitiesInputDto,
  UpsertModelAliasInputDto,
  RoutingPolicyConfigV2Dto,
  RoutingPolicySnapshotDto,
  ApplyRoutingPolicyDocumentInputDto,
} from "@/lib/bridge/generated";

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
export type RouteSimulationResult = RouteSimulationResultDto;

export type RoutingWorkspaceSnapshotInput = RoutingWorkspaceSnapshotInputDto;
export type RoutingPlannerEvaluationStatus = RoutingPlannerEvaluationStatusDto;
export type RoutingScoreStatus = RoutingScoreStatusDto;
export type RoutingWorkspaceCandidate = RoutingWorkspaceCandidateDto;
export type RoutingWorkspaceSnapshot = RoutingWorkspaceSnapshotDto;

export type RoutingRuntimeOverlay = RoutingRuntimeOverlayDto;
export type RoutingProtectionStatus = RoutingProtectionStatusDto;
export type RoutingProtectionStatusInput = RoutingProtectionStatusInputDto;
export type RecentRouteDecisionsInput = RecentRouteDecisionsInputDto;
export type RecentRouteDecisionsPage = RecentRouteDecisionsPageDto;

export type StationKeyOperationalDetail = StationKeyOperationalDetailDto;
export type RequestDecisionTrace = RequestDecisionTraceDto;
