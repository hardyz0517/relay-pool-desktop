import { invoke } from "@tauri-apps/api/core";
import { getSettings } from "@/lib/api/settings";
import type { LocalRoutingWorkspace, ReorderLocalRoutingKeysInput } from "@/lib/types/localRouting";
import type { AppSettings } from "@/lib/types/settings";

export function loadLocalRoutingWorkspaceApi() {
  return invoke<LocalRoutingWorkspace>("load_local_routing_workspace").catch(async (error) => {
    if (isInvokeUnavailable(error)) {
      return previewWorkspace(await getSettings());
    }
    throw error;
  });
}

export function reorderLocalRoutingKeys(input: ReorderLocalRoutingKeysInput) {
  return invoke<LocalRoutingWorkspace>("reorder_local_routing_keys", { input }).catch(async (error) => {
    if (isInvokeUnavailable(error)) {
      return previewWorkspace(await getSettings());
    }
    throw error;
  });
}

function previewWorkspace(settings: AppSettings): LocalRoutingWorkspace {
  return {
    proxyStatus: {
      running: false,
      lifecycle: "stopped",
      bindAddr: "127.0.0.1",
      port: settings.localProxyPort,
      startedAt: null,
      lastError: null,
      activeRequests: 0,
      requestCount: 0,
    },
    settings: {
      enabled: false,
      bindAddr: "127.0.0.1",
      port: settings.localProxyPort,
      endpoint: "chat_completions",
      policy: "automatic_balanced",
      maxRateMultiplier: settings.maxRateMultiplier,
      routingGroupFilter: settings.defaultRoutingGroupFilter,
      fallbackEnabled: true,
      previewKind: "baseline_eligibility",
    },
    summary: {
      candidateCount: 0,
      previewEligibleCandidateCount: 0,
      previewExcludedCandidateCount: 0,
      cooldownCandidateCount: 0,
      lastDecisionAt: null,
    },
    candidates: [],
    latestDecision: null,
    recentEvents: [],
  };
}

function isInvokeUnavailable(error: unknown) {
  return error instanceof Error && /invoke|__TAURI__/i.test(error.message);
}
