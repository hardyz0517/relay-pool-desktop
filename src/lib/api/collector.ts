import { invoke } from "@tauri-apps/api/core";
import type {
  CaptureSessionStatus,
  CollectorRunResult,
  CollectorSnapshot,
  CollectorTaskType,
  StationLoginTestInput,
  StationLoginTestResult,
} from "@/lib/types/collector";

const memorySnapshots = new Map<string, CollectorSnapshot>();

export function detectSub2apiStation(stationId: string) {
  return detectStationInfo(stationId);
}

export function collectSub2apiStation(stationId: string) {
  return collectStationInfo(stationId);
}

export function detectStationInfo(stationId: string) {
  return invoke<CollectorRunResult>("detect_station_info", { stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return createMemoryRun(stationId, "station-info-detect", "checked");
    }
    throw error;
  });
}

export function collectStationInfo(stationId: string) {
  return invoke<CollectorRunResult>("collect_station_info", { stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return createMemoryRun(stationId, "station-info-collect", "checked");
    }
    throw error;
  });
}

export function collectStationTask(stationId: string, taskType: CollectorTaskType) {
  return invoke<CollectorRunResult>("collect_station_task", { stationId, taskType }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return createMemoryRun(stationId, `station-${taskType}`, "checked");
    }
    throw error;
  });
}

export function testStationLogin(stationId: string) {
  return invoke<CollectorRunResult>("test_station_login", { stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return createMemoryRun(stationId, "login-state-test", "manual_required");
    }
    throw error;
  });
}

export function testStationLoginInput(input: StationLoginTestInput) {
  return invoke<StationLoginTestResult>("test_station_login_input", { input }).catch((error) => {
    if (isInvokeUnavailable(error)) {
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
  return invoke<CollectorSnapshot[]>("list_collector_snapshots", { stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return memorySnapshots.get(stationId) ? [memorySnapshots.get(stationId)!] : [];
    }
    throw error;
  });
}

export function getLatestCollectorSnapshot(stationId: string) {
  return invoke<CollectorSnapshot | null>("get_latest_collector_snapshot", { stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return memorySnapshots.get(stationId) ?? null;
    }
    throw error;
  });
}

export function startCaptureSession(stationId: string) {
  return invoke<CaptureSessionStatus>("start_capture_session", { stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return {
        stationId,
        status: "capturing",
        captureCount: 0,
        recognizedFieldCount: 0,
        pendingConfirmationCount: 0,
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
  return invoke<CaptureSessionStatus>("get_capture_session_status", { stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return {
        stationId,
        status: "idle",
        captureCount: 0,
        recognizedFieldCount: 0,
        pendingConfirmationCount: 0,
        lastError: null,
      };
    }
    throw error;
  });
}

export function finishCaptureSession(stationId: string) {
  return invoke<CollectorRunResult>("finish_capture_session", { stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return createMemoryRun(stationId, "webview-capture", "manual_required");
    }
    throw error;
  });
}

export function clearCaptureSession(stationId: string) {
  return invoke<CaptureSessionStatus>("clear_capture_session", { stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return {
        stationId,
        status: "idle",
        captureCount: 0,
        recognizedFieldCount: 0,
        pendingConfirmationCount: 0,
        lastError: null,
      };
    }
    throw error;
  });
}

export function closeCaptureSession(stationId: string) {
  return invoke<CaptureSessionStatus>("close_capture_session", { stationId }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return {
        stationId,
        status: "idle",
        captureCount: 0,
        recognizedFieldCount: 0,
        pendingConfirmationCount: 0,
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

function isInvokeUnavailable(error: unknown) {
  return error instanceof Error && /invoke|__TAURI__/i.test(error.message);
}
