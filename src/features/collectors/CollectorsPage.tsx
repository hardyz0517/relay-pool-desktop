import { useEffect, useMemo, useState } from "react";
import {
  Copy,
  Database,
  Radar,
  RefreshCcw,
  ShieldCheck,
} from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, EmptyState, SectionCard, StatusBadge } from "@/components/ui";
import {
  collectStationInfo,
  detectStationInfo,
  getLatestCollectorSnapshot,
  listCollectorSnapshots,
} from "@/lib/api/collector";
import { listStations } from "@/lib/api/stations";
import type {
  CollectorEndpointResult,
  CollectorSnapshot,
  CollectorSummary,
} from "@/lib/types/collector";
import { stationTypeLabels, type Station } from "@/lib/types/stations";
import { cn } from "@/lib/utils";

type TaskStatus = "idle" | "detecting" | "collecting" | "success" | "failed";

export function CollectorsPage() {
  const [stations, setStations] = useState<Station[]>([]);
  const [selectedStationId, setSelectedStationId] = useState<string>("");
  const [latestSnapshot, setLatestSnapshot] = useState<CollectorSnapshot | null>(null);
  const [history, setHistory] = useState<CollectorSnapshot[]>([]);
  const [loading, setLoading] = useState(true);
  const [taskStatus, setTaskStatus] = useState<TaskStatus>("idle");
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const selectedStation = useMemo(
    () => stations.find((station) => station.id === selectedStationId) ?? stations[0] ?? null,
    [selectedStationId, stations],
  );
  const summary = toCollectorSummary(latestSnapshot?.summaryJson);
  const recognized = summary.recognized;
  const endpointResults = summary.endpointResults ?? [];

  useEffect(() => {
    void refreshStations();
  }, []);

  useEffect(() => {
    if (!selectedStation) {
      setLatestSnapshot(null);
      setHistory([]);
      return;
    }
    void refreshSnapshot(selectedStation.id);
  }, [selectedStation?.id]);

  async function refreshStations() {
    setLoading(true);
    setError(null);
    try {
      const nextStations = await listStations();
      setStations(nextStations);
      setSelectedStationId((current) => {
        if (current && nextStations.some((station) => station.id === current)) {
          return current;
        }
        return nextStations[0]?.id ?? "";
      });
    } catch (requestError) {
      setError(readError(requestError));
    } finally {
      setLoading(false);
    }
  }

  async function refreshSnapshot(stationId: string) {
    try {
      const [nextSnapshot, nextHistory] = await Promise.all([
        getLatestCollectorSnapshot(stationId),
        listCollectorSnapshots(stationId),
      ]);
      setLatestSnapshot(nextSnapshot);
      setHistory(nextHistory.slice(0, 6));
    } catch (requestError) {
      setError(readError(requestError));
    }
  }

  async function handleDetect() {
    if (!selectedStation) return;
    setTaskStatus("detecting");
    setError(null);
    setMessage(null);
    try {
      const result = await detectStationInfo(selectedStation.id);
      setLatestSnapshot(result.snapshot);
      await Promise.all([refreshStations(), refreshSnapshot(selectedStation.id)]);
      setTaskStatus("success");
      setMessage("站点探测完成。");
    } catch (requestError) {
      setTaskStatus("failed");
      setError(shortError(readError(requestError)));
    }
  }

  async function handleCollect() {
    if (!selectedStation) return;
    setTaskStatus("collecting");
    setError(null);
    setMessage(null);
    try {
      const result = await collectStationInfo(selectedStation.id);
      setLatestSnapshot(result.snapshot);
      await Promise.all([refreshStations(), refreshSnapshot(selectedStation.id)]);
      setTaskStatus("success");
      setMessage("采集快照已保存。");
    } catch (requestError) {
      setTaskStatus("failed");
      setError(shortError(readError(requestError)));
    }
  }

  async function handleCopyDeveloperJson() {
    const text = buildDeveloperJson(latestSnapshot);
    if (!text) return;
    await navigator.clipboard.writeText(text);
    setMessage("已复制脱敏 JSON。");
  }

  const actionBusy = taskStatus === "detecting" || taskStatus === "collecting";

  return (
    <PageScaffold
      title="信息采集"
      description="统一探测中转站账号、余额、分组、倍率和接口能力；Sub2API / NewAPI 采集器会按站点类型自动选择。"
      actions={
        <div className="flex items-center gap-2">
          <select className={selectClassName} value={selectedStationId} onChange={(event) => setSelectedStationId(event.target.value)}>
            {stations.map((station) => (
              <option key={station.id} value={station.id}>{station.name}</option>
            ))}
          </select>
          <Button variant="secondary" onClick={handleDetect} disabled={actionBusy || !selectedStation}>
            <Radar className="h-4 w-4" />
            {taskStatus === "detecting" ? "探测中" : "探测"}
          </Button>
          <Button variant="secondary" onClick={handleCollect} disabled={actionBusy || !selectedStation}>
            <Database className="h-4 w-4" />
            {taskStatus === "collecting" ? "采集中" : "采集"}
          </Button>
          <Button variant="secondary" onClick={() => selectedStation && void refreshSnapshot(selectedStation.id)} disabled={!selectedStation || actionBusy}>
            <RefreshCcw className="h-4 w-4" />
            刷新
          </Button>
        </div>
      }
    >
      {loading ? (
        <div className="rounded-2xl border border-cyan-100 bg-white/85 px-4 py-5 text-sm text-muted-foreground">正在读取站点和采集快照...</div>
      ) : !selectedStation ? (
        <EmptyState title="还没有可采集的站点" description="先在中转池添加一个站点账号，再回到这里探测余额、分组、倍率和接口能力。" />
      ) : (
        <div className="grid gap-3 xl:grid-cols-[minmax(0,1fr)_360px]">
          <div className="space-y-3">
            <SectionCard
              title="采集结论"
              description={`${selectedStation.name} · ${stationTypeLabels[selectedStation.stationType]} · ${selectedStation.baseUrl}`}
              action={<StatusBadge tone={toneForConclusion(conclusionLabel(summary, latestSnapshot))}>{conclusionLabel(summary, latestSnapshot)}</StatusBadge>}
            >
              <div className="grid gap-3 lg:grid-cols-[1.1fr_0.9fr]">
                <div className="rounded-2xl border border-cyan-100 bg-cyan-50/45 p-3">
                  <div className="flex items-start gap-3">
                    <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-2xl border border-teal-100 bg-white text-teal-700">
                      <ShieldCheck className="h-5 w-5" />
                    </div>
                    <div className="min-w-0">
                      <div className="text-sm font-semibold text-slate-800">{summary.message ?? fallbackMessage(latestSnapshot)}</div>
                      <div className="mt-1 text-xs leading-5 text-muted-foreground">
                        采集器：{summary.adapter ?? adapterForStation(selectedStation)} · 识别类型：{summary.detectedType ?? "Unknown"}
                      </div>
                    </div>
                  </div>
                </div>
                <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-1">
                  <CompactFact label="最近采集" value={formatDateTime(latestSnapshot?.fetchedAt)} />
                  <CompactFact label="当前任务" value={taskStatusLabel(taskStatus, summary)} />
                </div>
              </div>
              {latestSnapshot?.errorMessage && (
                <div className="mt-3 rounded-2xl border border-amber-200 bg-amber-50/80 px-3 py-2 text-xs leading-5 text-amber-800">
                  {shortError(latestSnapshot.errorMessage)}
                </div>
              )}
            </SectionCard>

            <SectionCard title="识别结果" description="只展示已经识别出的业务信息，空值会保留为待采集状态。">
              <div className="grid gap-2 md:grid-cols-5">
                <ResultTile label="余额" value={displayValue(recognized?.balanceLabel)} />
                <ResultTile label="分组" value={countValue(recognized?.groupCount)} />
                <ResultTile label="倍率" value={countValue(recognized?.rateCount)} />
                <ResultTile label="Keys" value={countValue(recognized?.keyCount)} />
                <ResultTile label="字段" value={countValue(recognized?.matchedFieldCount)} />
              </div>
              <div className="mt-3 rounded-2xl border border-slate-200/70 bg-slate-50/70 px-3 py-2 text-xs leading-5 text-muted-foreground">
                {recognized && recognized.matchedFieldCount > 0
                  ? "这些字段来自接口探测的候选结果，后续 P4 会接入 WebView 登录捕获以提高准确率。"
                  : summary.webviewRequired
                    ? "未识别到可直接读取的余额、分组或倍率；可能需要登录，等待 P4 WebView 捕获。"
                    : "等待探测或采集。"}
              </div>
            </SectionCard>

            <SectionCard title="接口探测结果" description="404 代表该站点未开放对应接口，不等于采集失败。">
              {endpointResults.length > 0 ? (
                <div className="overflow-hidden rounded-2xl border border-cyan-100 bg-white/80">
                  {endpointResults.map((endpoint) => (
                    <EndpointRow key={`${endpoint.path}-${endpoint.result}`} endpoint={endpoint} />
                  ))}
                </div>
              ) : (
                <EmptyState title="暂无接口探测结果" description="点击“探测”后会显示常见接口的访问结果。" />
              )}
            </SectionCard>
          </div>

          <aside className="space-y-3">
            <SectionCard title="采集上下文" description="当前站点与 adapter。">
              <div className="space-y-2">
                <CompactFact label="站点账号" value={selectedStation.name} />
                <CompactFact label="站点类型" value={stationTypeLabels[selectedStation.stationType]} />
                <CompactFact label="API Keys" value={`${selectedStation.keyCount} keys`} />
                <CompactFact label="采集器" value={summary.adapter ?? adapterForStation(selectedStation)} />
              </div>
              <div className="mt-3 rounded-2xl border border-amber-200 bg-amber-50/80 p-3 text-xs leading-5 text-amber-800">
                WebView 登录捕获将在 P4 接入。当前仅进行非登录态 / 半登录态接口探测与脱敏快照保存。
              </div>
            </SectionCard>

            <SectionCard title="历史快照" description="最近保存的采集记录。">
              <div className="space-y-2">
                {history.length > 0 ? (
                  history.map((snapshot) => {
                    const itemSummary = toCollectorSummary(snapshot.summaryJson);
                    return (
                      <button
                        key={snapshot.id}
                        type="button"
                        className="w-full rounded-2xl border border-cyan-100 bg-cyan-50/45 px-3 py-2 text-left transition hover:border-teal-200 hover:bg-white"
                        onClick={() => setLatestSnapshot(snapshot)}
                      >
                        <div className="flex items-center justify-between gap-2">
                          <span className="truncate text-sm font-medium text-slate-800">{itemSummary.adapter ?? sourceLabel(snapshot.source)}</span>
                          <StatusBadge tone={toneForConclusion(conclusionLabel(itemSummary, snapshot))}>{conclusionLabel(itemSummary, snapshot)}</StatusBadge>
                        </div>
                        <div className="mt-1 truncate text-xs text-muted-foreground">{formatDateTime(snapshot.fetchedAt)} · {itemSummary.message ?? "暂无摘要"}</div>
                      </button>
                    );
                  })
                ) : (
                  <div className="rounded-2xl border border-dashed border-cyan-100 bg-cyan-50/45 px-3 py-4 text-sm text-muted-foreground">暂无历史快照。</div>
                )}
              </div>
            </SectionCard>

            <SectionCard title="开发者详情" description="默认收起，仅用于排查采集器。">
              <details className="group rounded-2xl border border-slate-200 bg-slate-50/70">
                <summary className="flex cursor-pointer list-none items-center justify-between gap-2 px-3 py-2 text-sm font-medium text-slate-700">
                  脱敏 snapshot JSON
                  <span className="text-xs text-muted-foreground group-open:hidden">展开</span>
                  <span className="hidden text-xs text-muted-foreground group-open:inline">收起</span>
                </summary>
                <div className="border-t border-slate-200 p-3">
                  <div className="mb-2 flex justify-end">
                    <Button variant="outline" className="h-7 px-2 text-xs" onClick={handleCopyDeveloperJson} disabled={!latestSnapshot}>
                      <Copy className="h-3.5 w-3.5" />
                      复制脱敏 JSON
                    </Button>
                  </div>
                  <pre className="max-h-72 overflow-auto rounded-xl bg-white p-3 text-[11px] leading-5 text-slate-600">{buildDeveloperJson(latestSnapshot) || "暂无 snapshot。"}</pre>
                </div>
              </details>
            </SectionCard>
          </aside>
        </div>
      )}

      {(message || error || actionBusy) && (
        <div className={cn("fixed bottom-4 right-4 z-40 rounded-2xl border px-4 py-3 text-sm shadow-[0_12px_30px_rgba(33,79,88,0.12)]", error ? "border-rose-200 bg-rose-50 text-rose-700" : actionBusy ? "border-cyan-200 bg-cyan-50 text-cyan-700" : "border-emerald-200 bg-emerald-50 text-emerald-700")}>
          {error ?? (actionBusy ? taskStatusLabel(taskStatus, summary) : message)}
        </div>
      )}
    </PageScaffold>
  );
}

function CompactFact({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-2xl border border-cyan-100 bg-white/85 px-3 py-2">
      <div className="text-[11px] text-muted-foreground">{label}</div>
      <div className="mt-0.5 truncate text-sm font-semibold text-slate-800">{value}</div>
    </div>
  );
}

function ResultTile({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-2xl border border-cyan-100 bg-white/85 px-3 py-2.5">
      <div className="text-[11px] text-muted-foreground">{label}</div>
      <div className="mt-1 truncate text-lg font-semibold text-slate-800">{value}</div>
    </div>
  );
}

function EndpointRow({ endpoint }: { endpoint: CollectorEndpointResult }) {
  return (
    <div className="grid min-h-12 grid-cols-[150px_92px_minmax(0,1fr)] items-center gap-3 border-b border-cyan-100 px-3 py-2 last:border-b-0">
      <div className="truncate font-mono text-xs text-slate-700">{endpoint.path}</div>
      <StatusBadge tone={toneForEndpoint(endpoint.result)}>{endpoint.result}</StatusBadge>
      <div className="truncate text-xs text-muted-foreground">{endpoint.detail}</div>
    </div>
  );
}

function toCollectorSummary(value: Record<string, unknown> | undefined): CollectorSummary {
  if (!value) return {};
  return {
    adapter: readString(value.adapter),
    detectedType: readString(value.detectedType),
    conclusion: readString(value.conclusion),
    message: readString(value.message),
    endpointResults: Array.isArray(value.endpointResults)
      ? value.endpointResults.map(toEndpointResult).filter(isEndpointResult)
      : [],
    recognized: toRecognizedSummary(value.recognized),
    webviewRequired: typeof value.webviewRequired === "boolean" ? value.webviewRequired : undefined,
    webviewNote: readString(value.webviewNote),
  };
}

function toEndpointResult(value: unknown): CollectorEndpointResult | null {
  if (!value || typeof value !== "object") return null;
  const record = value as Record<string, unknown>;
  return {
    path: readString(record.path) ?? "/",
    result: readString(record.result) ?? "已检查",
    detail: readString(record.detail) ?? "暂无说明",
    statusCode: typeof record.statusCode === "number" ? record.statusCode : null,
  };
}

function isEndpointResult(value: CollectorEndpointResult | null): value is CollectorEndpointResult {
  return value !== null;
}

function toRecognizedSummary(value: unknown) {
  if (!value || typeof value !== "object") return undefined;
  const record = value as Record<string, unknown>;
  return {
    balanceLabel: record.balanceLabel,
    groupCount: readNumber(record.groupCount),
    rateCount: readNumber(record.rateCount),
    keyCount: readNumber(record.keyCount),
    matchedFieldCount: readNumber(record.matchedFieldCount),
  };
}

function buildDeveloperJson(snapshot: CollectorSnapshot | null) {
  if (!snapshot) return "";
  const text = JSON.stringify(
    {
      summary: snapshot.summaryJson,
      normalized: snapshot.normalizedJson,
      rawRedacted: snapshot.rawJsonRedacted,
      error: snapshot.errorMessage ? shortError(snapshot.errorMessage) : null,
    },
    null,
    2,
  );
  return text.length > 5000 ? `${text.slice(0, 5000)}\n... 已截断` : text;
}

function formatDateTime(value: string | undefined | null) {
  if (!value) return "未采集";
  const numeric = Number(value);
  const date = Number.isFinite(numeric) && value.trim() !== "" ? new Date(numeric) : new Date(value);
  if (Number.isNaN(date.getTime())) return "未采集";
  const pad = (next: number) => String(next).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

function conclusionLabel(summary: CollectorSummary, snapshot: CollectorSnapshot | null) {
  return summary.conclusion ?? statusLabel(snapshot?.status) ?? "未采集";
}

function statusLabel(status: string | undefined) {
  if (!status) return undefined;
  if (status === "success") return "已采集";
  if (status === "manual_required") return "需要登录";
  if (status === "partial") return "未识别";
  if (status === "failed") return "失败";
  return "已检查";
}

function taskStatusLabel(status: TaskStatus, summary: CollectorSummary) {
  if (status === "detecting") return "正在探测站点接口...";
  if (status === "collecting") return "正在采集并保存快照...";
  if (status === "success") return summary.message ?? "任务完成";
  if (status === "failed") return "任务失败";
  return "空闲";
}

function fallbackMessage(snapshot: CollectorSnapshot | null) {
  if (!snapshot) return "尚未采集。请选择站点后点击探测或采集。";
  if (snapshot.errorMessage) return shortError(snapshot.errorMessage);
  return "快照已保存，但暂未识别到可展示的业务字段。";
}

function displayValue(value: unknown) {
  if (value === null || value === undefined) return "未识别";
  if (Array.isArray(value)) return value.length ? `${value.length} 项` : "未识别";
  if (typeof value === "object") return "已识别";
  const text = String(value);
  return text.trim() ? text : "未识别";
}

function countValue(value: number | undefined) {
  return typeof value === "number" && value > 0 ? String(value) : "未识别";
}

function readString(value: unknown) {
  return typeof value === "string" && value.trim() ? value : undefined;
}

function readNumber(value: unknown) {
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

function adapterForStation(station: Station) {
  if (station.stationType === "sub2api") return "Sub2API Adapter";
  if (station.stationType === "newapi") return "NewAPI Adapter（待接入）";
  if (station.stationType === "openai-compatible") return "OpenAI-compatible Adapter（基础探测）";
  return "Auto Detect";
}

function sourceLabel(source: string) {
  if (source.includes("detect")) return "接口探测";
  if (source.includes("collect")) return "信息采集";
  return "采集快照";
}

function toneForConclusion(value: string) {
  if (value === "可用" || value === "已采集") return "healthy" as const;
  if (value === "需要登录" || value === "未识别" || value === "已检查") return "warning" as const;
  if (value === "失败") return "error" as const;
  return "info" as const;
}

function toneForEndpoint(value: string) {
  if (value === "成功") return "healthy" as const;
  if (value === "需要登录" || value === "404" || value === "限流" || value === "已检查") return "warning" as const;
  if (value === "超时" || value === "请求失败" || value === "站点异常") return "error" as const;
  return "info" as const;
}

function shortError(error: string) {
  if (/timeout|timed out/i.test(error)) return "站点请求超时";
  if (/resolve|dns/i.test(error)) return "域名解析失败";
  if (/connection refused/i.test(error)) return "站点拒绝连接";
  if (/certificate|tls/i.test(error)) return "TLS 证书或连接失败";
  return error.length > 120 ? `${error.slice(0, 120)}...` : error;
}

function readError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

const selectClassName = "h-8 rounded-xl border border-cyan-100 bg-cyan-50/45 px-3 text-sm text-slate-800 outline-none transition focus:border-teal-300 focus:bg-white focus:ring-2 focus:ring-teal-100";
