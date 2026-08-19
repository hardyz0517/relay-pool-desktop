import { describe, expect, it } from "vitest";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import type { RoutingCandidateView } from "@/lib/types/routingWorkspace";
import { buildEditableRoutingCandidates } from "./editableRoutingCandidates";

describe("buildEditableRoutingCandidates", () => {
  it("uses 密钥池 order even when the status projection is dynamically sorted", () => {
    const candidates = buildEditableRoutingCandidates(
      [keyPoolItem("key-3", 0), keyPoolItem("key-1", 1), keyPoolItem("key-2", 2)],
      [workspaceCandidate("key-1"), workspaceCandidate("key-2")],
      "all_groups",
    );

    expect(candidates.map((candidate) => candidate.stationKeyId)).toEqual(["key-3", "key-1", "key-2"]);
    expect(candidates[0]).toMatchObject({ keyName: "key-3", priority: 0 });
  });
});

function keyPoolItem(id: string, priority: number): KeyPoolItem {
  return {
    id,
    stationId: "station-1",
    stationName: "Station",
    name: id,
    enabled: true,
    priority,
    schedulable: true,
    cooldownUntil: null,
    consecutiveFailures: 0,
  } as KeyPoolItem;
}

function workspaceCandidate(stationKeyId: string): RoutingCandidateView {
  return {
    stationKeyId,
    stationId: "station-1",
    stationName: "Station",
    keyName: stationKeyId,
    endpoint: "chat_completions",
    priority: 99,
    enabled: true,
    schedulable: true,
    healthState: "ready",
    score: null,
    scoreDetails: null,
    currentConcurrency: null,
    lastSuccessAt: null,
    lastFailureAt: null,
    cooldownUntil: null,
    routingGroupScope: "all_groups",
    routingGroupMatch: true,
    previewEligible: true,
    previewRejectReasons: [],
    facts: [],
  };
}
