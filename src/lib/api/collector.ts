import {
  clearCaptureSession as clearCaptureSessionGenerated,
  collectStationInfo as collectStationInfoGenerated,
  collectStationTask as collectStationTaskGenerated,
  collectSub2apiStation as collectSub2apiStationGenerated,
  closeCaptureSession as closeCaptureSessionGenerated,
  detectStationInfo as detectStationInfoGenerated,
  detectSub2apiStation as detectSub2apiStationGenerated,
  finishCaptureSession as finishCaptureSessionGenerated,
  finishWebAuthorizationSession as finishWebAuthorizationSessionGenerated,
  getCaptureSessionStatus as getCaptureSessionStatusGenerated,
  getLatestCollectorSnapshot as getLatestCollectorSnapshotGenerated,
  listCollectorSnapshots as listCollectorSnapshotsGenerated,
  startCaptureSession as startCaptureSessionGenerated,
  testStationLogin as testStationLoginGenerated,
  testStationLoginInput as testStationLoginInputGenerated,
} from "@/lib/bridge/generated";
import { isTauriInvokeUnavailable } from "@/lib/tauriErrors";
import type {
  CollectorRunResult,
  CollectorSnapshot,
  CollectorTaskType,
  StationLoginTestInput,
} from "@/lib/types/collector";

const memorySnapshots = new Map<string, CollectorSnapshot>();

export function detectSub2apiStation(stationId: string) {
  return detectSub2apiStationGenerated({ stationId }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return createMemoryRun(stationId, "station-info-detect", "checked");
    }
    throw error;
  });
}

export function collectSub2apiStation(stationId: string) {
  return collectSub2apiStationGenerated({ stationId }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return createMemoryRun(stationId, "station-info-collect", "checked");
    }
    throw error;
  });
}

export function detectStationInfo(stationId: string) {
  return detectStationInfoGenerated({ stationId }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return createMemoryRun(stationId, "station-info-detect", "checked");
    }
    throw error;
  });
}

export function collectStationInfo(stationId: string) {
  return collectStationInfoGenerated({ stationId }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return createMemoryRun(stationId, "station-info-collect", "checked");
    }
    throw error;
  });
}

export function collectStationTask(stationId: string, taskType: CollectorTaskType) {
  return collectStationTaskGenerated({ stationId, taskType }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return createMemoryRun(stationId, `station-${taskType}`, "checked");
    }
    throw error;
  });
}

export function testStationLogin(stationId: string) {
  return testStationLoginGenerated({ stationId }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return createMemoryRun(stationId, "login-state-test", "manual_required");
    }
    throw error;
  });
}

export function testStationLoginInput(input: StationLoginTestInput) {
  return testStationLoginInputGenerated({
    stationType: input.stationType === "newapi" ? "newapi" : "sub2api",
    websiteUrl: input.websiteUrl,
    loginUsername: input.loginUsername,
    loginPassword: input.loginPassword,
  }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return {
        status: "manual_required",
        message: "普通浏览器环境无法执行真实连通性测试。",
        diagnosis: "请在 Tauri 桌面窗口中测试。",
        tokenPresent: false,
      };
    }
    throw error;
  });
}

export function listCollectorSnapshots(stationId: string) {
  return listCollectorSnapshotsGenerated({ stationId }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return memorySnapshots.get(stationId) ? [memorySnapshots.get(stationId)!] : [];
    }
    throw error;
  });
}

export function getLatestCollectorSnapshot(stationId: string) {
  return getLatestCollectorSnapshotGenerated({ stationId }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return memorySnapshots.get(stationId) ?? null;
    }
    throw error;
  });
}

export function startCaptureSession(stationId: string) {
  return startCaptureSessionGenerated({ stationId }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return {
        stationId,
        status: "capturing",
        captureCount: 0,
        recognizedFieldCount: 0,
        pendingConfirmationCount: 0,
        webAuthorizationCandidate: false,
        lastError: null,
      };
    }
    throw error;
  });
}

export function startManualAuthorization(stationId: string) {
  return startCaptureSession(stationId);
}

export function getCaptureSessionStatus(stationId: string) {
  return getCaptureSessionStatusGenerated({ stationId }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return {
        stationId,
        status: "idle",
        captureCount: 0,
        recognizedFieldCount: 0,
        pendingConfirmationCount: 0,
        webAuthorizationCandidate: false,
        lastError: null,
      };
    }
    throw error;
  });
}

export function finishCaptureSession(stationId: string) {
  return finishCaptureSessionGenerated({ stationId }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return createMemoryRun(stationId, "webview-capture", "manual_required");
    }
    throw error;
  });
}

export function finishWebAuthorizationSession(stationId: string) {
  return finishWebAuthorizationSessionGenerated({ stationId }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return createMemoryRun(stationId, "webview-capture", "manual_required");
    }
    throw error;
  });
}

export function clearCaptureSession(stationId: string) {
  return clearCaptureSessionGenerated({ stationId }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return {
        stationId,
        status: "idle",
        captureCount: 0,
        recognizedFieldCount: 0,
        pendingConfirmationCount: 0,
        webAuthorizationCandidate: false,
        lastError: null,
      };
    }
    throw error;
  });
}

export function closeCaptureSession(stationId: string) {
  return closeCaptureSessionGenerated({ stationId }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return {
        stationId,
        status: "idle",
        captureCount: 0,
        recognizedFieldCount: 0,
        pendingConfirmationCount: 0,
        webAuthorizationCandidate: false,
        lastError: null,
      };
    }
    throw error;
  });
}

function createMemoryRun(stationId: string, source: string, status: string): CollectorRunResult {
  const now = new Date().toISOString();
  const snapshot: CollectorSnapshot = {
    id: `snapshot-${Date.now()}`,
    stationId,
    endpointRevision: 1,
    source,
    status,
    fetchedAt: now,
    summaryJson: {
      mode: source.includes("login-state") ? "login-state" : source.includes("detect") ? "detect" : "collect",
      adapter: source.includes("login-state") ? "Login State Adapter" : "Auto Detect",
      detectedType: source.includes("login-state") ? "Login State" : "Unknown",
      conclusion: source.includes("login-state") ? "需要登录" : "未识别",
      message: source.includes("login-state")
        ? "登录态采集主流程已切换到账号密码测试。"
        : "普通浏览器环境没有 Tauri invoke；桌面窗口会使用真实 SQLite 快照。",
      endpointResults: [],
      recognized: {
        balanceLabel: "未识别",
        groupCount: 0,
        rateCount: 0,
        keyCount: 0,
        matchedFieldCount: 0,
      },
      webviewRequired: source.includes("login-state"),
      webviewNote: "WebView 登录捕获仍保留为高级兜底功能。",
    },
    normalizedJson: {
      balance: null,
      groups: [],
      rateMultipliers: [],
      keys: [],
      matchedFields: [],
    },
    rawJsonRedacted: {
      stationId,
      note: "Browser fallback only; Tauri commands persist real snapshots.",
    },
    errorMessage: "普通浏览器环境没有 Tauri invoke；桌面窗口会使用真实 SQLite 快照。",
    createdAt: now,
  };
  memorySnapshots.set(stationId, snapshot);
  return {
    snapshot,
    events: [
      {
        eventType: "fallback",
        message: "Tauri invoke unavailable in browser preview.",
        status: "checked",
      },
    ],
  };
}
