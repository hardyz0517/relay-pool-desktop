import { invoke } from "@tauri-apps/api/core";
import type { CollectorRunResult, CollectorSnapshot } from "@/lib/types/collector";

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

function createMemoryRun(stationId: string, source: string, status: string): CollectorRunResult {
  const now = new Date().toISOString();
  const snapshot: CollectorSnapshot = {
    id: `snapshot-${Date.now()}`,
    stationId,
    source,
    status,
    fetchedAt: now,
    summaryJson: {
      mode: source.includes("detect") ? "detect" : "collect",
      adapter: "Auto Detect",
      detectedType: "Unknown",
      conclusion: "未识别",
      message: "普通浏览器环境没有 Tauri invoke；桌面窗口会使用真实 SQLite 快照。",
      endpointResults: [],
      recognized: {
        balanceLabel: "未识别",
        groupCount: 0,
        rateCount: 0,
        keyCount: 0,
        matchedFieldCount: 0,
      },
      webviewNote: "WebView 登录捕获将在 P4 接入。",
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
