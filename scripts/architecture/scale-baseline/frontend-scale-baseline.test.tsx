// @vitest-environment jsdom

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { QueryClient, QueryClientProvider, useQuery } from "@tanstack/react-query";
import { Profiler, useEffect, type ProfilerOnRenderCallback } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, test } from "vitest";
import { setActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { BackendClient } from "@/lib/bridge/BackendClient";
import {
  currentStationBalanceSnapshotsQueryOptions,
  stationAssetsQueryOptions,
  stationsQueryOptions,
} from "@/lib/query/resourceQueries";
import { alertingCurrentQueryOptions } from "@/lib/queries/alertingQueries";
import { canonicalJson, DATASET_SIZES, generateDataset, sha256 } from "./dataset.mjs";

const WARMUP_RUNS = 5;
const SAMPLE_RUNS = 30;
const reportPath = process.env.ARCHITECTURE_SCALE_REPORT;
if (!reportPath) throw new Error("ARCHITECTURE_SCALE_REPORT is required");

type Dataset = ReturnType<typeof generateDataset>;
type QueryEvent = { type: string; status: string; fetchStatus: string; at: number };
type CommandCall = { command: string; projected_response_json_bytes: number };

const mounted: Array<ReturnType<typeof createRoot>> = [];
afterEach(() => {
  while (mounted.length) mounted.pop()?.unmount();
  setActiveBackendClient(null);
  document.body.replaceChildren();
});

function CurrentStationsQueryTopology({ enabled, onReady }: { enabled: boolean; onReady: () => void }) {
  const stationsQuery = useQuery({ ...stationsQueryOptions(), enabled, subscribed: enabled });
  const balancesQuery = useQuery({ ...currentStationBalanceSnapshotsQueryOptions(), enabled, subscribed: enabled });
  const incidentsQuery = useQuery({ ...alertingCurrentQueryOptions(), enabled, subscribed: enabled });
  const stations = stationsQuery.data ?? [];
  const stationIds = stations.map((station) => station.id);
  const stationAssetsQuery = useQuery({ ...stationAssetsQueryOptions(stationIds), subscribed: enabled });
  const ready = enabled && stationsQuery.isSuccess && balancesQuery.isSuccess && incidentsQuery.isSuccess && stationAssetsQuery.isSuccess;
  useEffect(() => {
    if (ready) onReady();
  }, [onReady, ready]);
  if (!ready) return <div data-state={enabled ? "loading" : "hidden"} data-row-count="0" />;
  return <div data-state="ready" data-row-count={stations.length}>{stations.map((station) => <span key={station.id}>{station.name}</span>)}</div>;
}

function installCurrentBackendMock(dataset: Dataset, calls: CommandCall[]) {
  function record<T>(command: string, response: T): T {
    calls.push({ command, projected_response_json_bytes: Buffer.byteLength(JSON.stringify(response)) });
    return JSON.parse(JSON.stringify(response));
  }
  setActiveBackendClient({
    mode: "desktop",
    stations: {
      listStations: async () => record("list_stations", dataset.stations) as never,
    },
    economics: {
      listCurrentStationBalanceSnapshots: async () => record("list_current_station_balance_snapshots", []) as never,
    },
    alerting: {
      loadWorkspace: async () => record("load_alerting_workspace", {}) as never,
      getSettings: async () => record("get_alerting_settings", {}) as never,
      updateSettings: async () => record("update_alerting_settings", {}) as never,
      listPolicies: async () => record("list_alert_policies", []) as never,
      upsertPolicy: async () => record("upsert_alert_policy", {}) as never,
      deletePolicy: async () => undefined,
      listCurrentIncidents: async () => record("list_current_alerting_incidents", { items: [], nextCursor: null, totalApprox: 0 }) as never,
      getIncident: async () => record("get_alerting_incident", {}) as never,
      listOccurrences: async () => record("list_alerting_occurrences", { items: [], nextCursor: null }) as never,
      listDeliveries: async () => record("list_alerting_deliveries", { items: [], nextCursor: null }) as never,
      markSeen: async () => undefined,
      acknowledge: async () => undefined,
      snooze: async () => undefined,
      sendTestNotification: async () => undefined,
      getDesktopNotificationPermission: async () => "unavailable",
      requestDesktopNotificationPermission: async () => "unavailable",
    },
    collectors: {
      listLatestCollectorSnapshots: async () => record("list_latest_collector_snapshots", []) as never,
    },
    settings: {} as BackendClient["settings"],
    stationKeys: {} as BackendClient["stationKeys"],
    collectorRuns: {} as BackendClient["collectorRuns"],
    proxy: {} as BackendClient["proxy"],
    localRouting: {} as BackendClient["localRouting"],
    dataRecovery: {} as BackendClient["dataRecovery"],
    groupFacts: {} as BackendClient["groupFacts"],
    pricing: {} as BackendClient["pricing"],
    routing: {} as BackendClient["routing"],
    channels: {} as BackendClient["channels"],
    updater: {} as BackendClient["updater"],
    handshake: async () => ({}) as never,
  } as BackendClient);
}

async function oneRun(dataset: Dataset, enabled: boolean) {
  const commandCalls: CommandCall[] = [];
  const queryEvents: QueryEvent[] = [];
  const profilerCommits: Array<{ phase: string; actual_duration_ms: number; commit_time_ms: number }> = [];
  installCurrentBackendMock(dataset, commandCalls);
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false, gcTime: 0 } } });
  const unsubscribe = queryClient.getQueryCache().subscribe((event) => {
    if (!event?.query) return;
    queryEvents.push({ type: event.type, status: event.query.state.status, fetchStatus: event.query.state.fetchStatus, at: performance.now() });
  });
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  mounted.push(root);
  let resolveReady!: () => void;
  const ready = new Promise<void>((resolve) => { resolveReady = resolve; });
  const started = performance.now();
  const onRender: ProfilerOnRenderCallback = (_id, phase, actualDuration, _baseDuration, _startTime, commitTime) => {
    profilerCommits.push({ phase, actual_duration_ms: actualDuration, commit_time_ms: commitTime });
  };
  root.render(
    <QueryClientProvider client={queryClient}>
      <Profiler id="current-stations-query-topology" onRender={onRender}>
        <CurrentStationsQueryTopology enabled={enabled} onReady={resolveReady} />
      </Profiler>
    </QueryClientProvider>,
  );
  if (enabled) await ready;
  else await new Promise((resolve) => setTimeout(resolve, 0));
  const dataReadyMs = enabled ? performance.now() - started : null;
  const renderedRowCount = Number(host.querySelector("[data-row-count]")?.getAttribute("data-row-count") ?? -1);
  if (enabled) expect(renderedRowCount).toBe(dataset.size);
  else expect(commandCalls).toHaveLength(0);
  unsubscribe();
  root.unmount();
  mounted.pop();
  queryClient.clear();
  host.remove();
  setActiveBackendClient(null);
  return {
    invoke_count: commandCalls.length,
    commands: commandCalls,
    projected_response_json_bytes: commandCalls.reduce((total, call) => total + call.projected_response_json_bytes, 0),
    query_lifecycle: queryEvents,
    react_profiler_commits: profilerCommits,
    rendered_row_count: renderedRowCount,
    data_ready_commit_ms: dataReadyMs,
  };
}

function percentile(values: number[], quantile: number) {
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.max(0, Math.ceil(sorted.length * quantile) - 1)];
}

function summarize(samples: Awaited<ReturnType<typeof oneRun>>[]) {
  const durations = samples.map((sample) => sample.data_ready_commit_ms).filter((value): value is number => value !== null);
  const payloads = samples.map((sample) => sample.projected_response_json_bytes);
  const invokes = samples.map((sample) => sample.invoke_count);
  return {
    invoke_count: { min: Math.min(...invokes), max: Math.max(...invokes) },
    projected_response_json_bytes: { p50: percentile(payloads, 0.5), p95: percentile(payloads, 0.95) },
    data_ready_commit_ms: { p50: percentile(durations, 0.5), p95: percentile(durations, 0.95) },
  };
}

function blockedMetric(ownerTask: 11 | 26, reason: string) {
  return { value: null, qualification: "blocked", owner: `Task ${ownerTask}`, owner_task: ownerTask, release_gate: 26, reason } as const;
}

describe("deterministic current frontend scale baseline", () => {
  test("records 10/100/500 raw samples without fabricating native metrics", async () => {
    const datasets: Record<string, unknown> = {};
    for (const size of DATASET_SIZES) {
      const dataset = generateDataset(size);
      for (let index = 0; index < WARMUP_RUNS; index += 1) await oneRun(dataset, true);
      const hiddenProbe = await oneRun(dataset, false);
      const samples = [];
      for (let index = 0; index < SAMPLE_RUNS; index += 1) samples.push(await oneRun(dataset, true));
      datasets[String(size)] = {
        fixture_sha256: sha256(canonicalJson(dataset)),
        fixture_json_bytes: Buffer.byteLength(canonicalJson(dataset)),
        warmup_runs: WARMUP_RUNS,
        sample_runs: SAMPLE_RUNS,
        hidden_query_start_count: hiddenProbe.invoke_count,
        summary: summarize(samples),
        samples,
      };
    }
    const report = {
      schema_version: 1,
      qualification: "frontend-jsdom-current-query-topology-baseline-only",
      provenance: {
        source_revision: process.env.ARCHITECTURE_SOURCE_REVISION,
        profile: "vitest-jsdom",
        node: process.version,
        platform: `${process.platform}-${process.arch}`,
        machine: `${os.cpus()[0]?.model ?? "unknown"}|${os.totalmem()}`,
        generated_at: new Date().toISOString(),
      },
      method: {
        warmup_runs: WARMUP_RUNS,
        sample_runs: SAMPLE_RUNS,
        clock: "performance.now",
        renderer: "react-profiler-jsdom",
        topology: "production query factories and Tauri invoke wrappers with deterministic mock IPC",
      },
      datasets,
      blocked_metrics: {
        backend_read_port_round_trips: blockedMetric(11, "backend read-port instrumentation is introduced by the aggregate read shard"),
        backend_sql_statement_count_runtime: blockedMetric(11, "runtime SQL instrumentation is not owned by Stage 0"),
        backend_query_duration_ms: blockedMetric(11, "backend query timing requires the aggregate query owner"),
        real_tauri_ipc_payload_bytes: blockedMetric(26, "real packaged Tauri IPC qualification is a release gate"),
        real_tauri_command_duration_ms: blockedMetric(26, "real packaged Tauri command timing is a release gate"),
        webview2_page_commit_ms: blockedMetric(26, "WebView2 timeline evidence requires the packaged release runtime"),
      },
    };
    fs.mkdirSync(path.dirname(reportPath), { recursive: true });
    fs.writeFileSync(reportPath, JSON.stringify(report, null, 2) + "\n", "utf8");
  }, 120_000);
});
