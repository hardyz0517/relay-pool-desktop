import { invoke } from "@tauri-apps/api/core";
import type { LocalRoutingWorkspace } from "@/lib/types/localRouting";

export function loadLocalRoutingWorkspaceApi() {
  return invoke<LocalRoutingWorkspace>("load_local_routing_workspace").catch((error) => {
    if (isInvokeUnavailable(error)) {
      return previewWorkspace();
    }
    throw error;
  });
}

function previewWorkspace(): LocalRoutingWorkspace {
  return {
    proxyStatus: {
      running: false,
      bindAddr: "127.0.0.1",
      port: 8787,
      startedAt: null,
      lastError: null,
      activeRequests: 0,
      requestCount: 0,
    },
    settings: {
      enabled: false,
      bindAddr: "127.0.0.1",
      port: 8787,
      endpoint: "chat_completions",
      policy: "priority_fallback",
      fallbackEnabled: true,
    },
    summary: {
      enabledCandidateCount: 0,
      healthyCandidateCount: 0,
      degradedCandidateCount: 0,
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
