import type { RoutingGroupFilter } from "@/lib/types/routing";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import type { RoutingCandidateView } from "@/lib/types/routingWorkspace";

function fallbackCandidate(
  item: KeyPoolItem,
  routingGroupScope: RoutingGroupFilter,
): RoutingCandidateView {
  const plannerExclusionCodes = [
    ...(!item.enabled ? ["candidate_disabled"] : []),
    ...(!item.schedulable ? ["candidate_unschedulable"] : []),
  ];

  return {
    stationKeyId: item.id,
    stationId: item.stationId,
    stationName: item.stationName,
    keyName: item.name,
    endpoint: "chat_completions",
    priority: item.priority,
    enabled: item.enabled,
    schedulable: item.schedulable,
    participationStatus: "unavailable",
    participationReason: "circuit_persistence_unavailable",
    score: null,
    scoreDetails: null,
    currentConcurrency: null,
    lastSuccessAt: null,
    lastFailureAt: null,
    routingGroupScope,
    routingGroupMatch: true,
    scoreStatus: plannerExclusionCodes.length === 0 ? "unavailable" : "excluded",
    plannerExclusionCodes,
    assessmentSnapshotId: null,
    assessmentDurableRevision: null,
    assessmentRequestContextFingerprint: null,
    facts: [],
  };
}

/**
 * 密钥池 is the write model for ordering. Workspace candidates remain a
 * status projection and must never determine the persisted sequence.
 */
export function buildEditableRoutingCandidates(
  keyPoolItems: readonly KeyPoolItem[],
  workspaceCandidates: readonly RoutingCandidateView[],
  routingGroupScope: RoutingGroupFilter,
): RoutingCandidateView[] {
  const workspaceById = new Map(
    workspaceCandidates.map((candidate) => [candidate.stationKeyId, candidate]),
  );

  return keyPoolItems.map((item) => {
    const workspaceCandidate = workspaceById.get(item.id);
    if (!workspaceCandidate) {
      return fallbackCandidate(item, routingGroupScope);
    }

    return {
      ...workspaceCandidate,
      priority: item.priority,
      enabled: item.enabled,
    };
  });
}
