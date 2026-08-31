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

export type RoutingPolicyConfigV3 = RoutingPolicyConfigV3Dto;
export type RoutingPolicySnapshot = RoutingPolicySnapshotDto;
export type ApplyRoutingPolicyDocumentInput = ApplyRoutingPolicyDocumentInputDto;
export type RoutingPolicyPublicationStatusInput = RoutingPolicyPublicationStatusInputDto;
export type RoutingPolicyPublicationStatus = RoutingPolicyPublicationStatusDto;
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
