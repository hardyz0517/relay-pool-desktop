import type { RoutingCandidateView } from "@/lib/types/routingWorkspace";

/**
 * Returns the score the planner uses when ordering candidates.  The
 * workspace's `score` field is the unadjusted/base score; the planner may
 * apply a bounded affinity correction and exposes that result as
 * `diagnostics.effectiveScore`.
 */
export function getRoutingEffectiveScore(
  candidate: Pick<RoutingCandidateView, "diagnostics" | "score">,
) {
  const value = candidate.diagnostics?.effectiveScore ?? candidate.score;
  return value != null && Number.isFinite(value) ? value : null;
}
