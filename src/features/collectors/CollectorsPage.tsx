import { useEffect, useMemo, useState } from "react";
import {
  ChevronDown,
  Activity,
  Copy,
  Database,
  Radar,
  RefreshCcw,
  ShieldCheck,
} from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, EmptyState, InspectorPanel, ObjectRow, SectionCard, SelectControl, StatusBadge, useToast } from "@/components/ui";
import {
  collectStationInfo,
  clearCaptureSession,
  detectStationInfo,
  closeCaptureSession,
  finishCaptureSession,
  getCaptureSessionStatus,
  getLatestCollectorSnapshot,
  listCollectorSnapshots,
  startCaptureSession,
  testStationLogin,
} from "@/lib/api/collector";
import { listStations } from "@/lib/api/stations";
import type {
  CaptureSessionStatus,
  CollectorEndpointResult,
  CollectorSnapshot,
  CollectorSummary,
} from "@/lib/types/collector";
import { stationTypeLabels, type Station } from "@/lib/types/stations";
import { cn } from "@/lib/utils";

type TaskStatus =
  | "idle"
  | "testingLogin"
  | "collecting"
  | "detecting"
  | "capturing"
  | "finishingCapture"
  | "success"
  | "failed";

export function CollectorsPage() {
  const toast = useToast();
  const [stations, setStations] = useState<Station[]>([]);
  const [selectedStationId, setSelectedStationId] = useState<string>("");
  const [latestSnapshot, setLatestSnapshot] = useState<CollectorSnapshot | null>(null);
  const [history, setHistory] = useState<CollectorSnapshot[]>([]);
  const [loading, setLoading] = useState(true);
  const [taskStatus, setTaskStatus] = useState<TaskStatus>("idle");
  const [captureStatus, setCaptureStatus] = useState<CaptureSessionStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  const selectedStation = useMemo(
    () => stations.find((station) => station.id === selectedStationId) ?? stations[0] ?? null,
    [selectedStationId, stations],
  );
  const summary = toCollectorSummary(latestSnapshot?.summaryJson);
  const recognized = summary.recognized;
  const endpointResults = summary.endpointResults ?? [];
  const normalized = latestSnapshot?.normalizedJson ?? {};
  const modelCount = Array.isArray(normalized.models) ? normalized.models.length : 0;

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
    void refreshCaptureStatus(selectedStation.id);
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
      const message = readError(requestError);
      setError(message);
      toast.error("读取站点失败", message);
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
      toast.error("刷新采集快照失败", readError(requestError));
    }
  }

  async function refreshCaptureStatus(stationId: string) {
    try {
      setCaptureStatus(await getCaptureSessionStatus(stationId));
    } catch {
      setCaptureStatus(null);
    }
  }

  async function handleCollect() {
    if (!selectedStation) return;
    setTaskStatus("collecting");
    setError(null);
    try {
      const result = await collectStationInfo(selectedStation.id);
      setLatestSnapshot(result.snapshot);
      await Promise.all([refreshStations(), refreshSnapshot(selectedStation.id)]);
      setTaskStatus("success");
      toast.success("采集信息已完成");
    } catch (requestError) {
      setTaskStatus("failed");
      toast.error("采集信息失败", shortError(readError(requestError)));
    }
  }

  async function handleTestLogin() {
    if (!selectedStation) return;
    setTaskStatus("testingLogin");
    setError(null);
    try {
      const result = await testStationLogin(selectedStation.id);
      setLatestSnapshot(result.snapshot);
      await Promise.all([refreshStations(), refreshSnapshot(selectedStation.id)]);
      setTaskStatus("success");
      toast.success("登录测试已完成");
    } catch (requestError) {
      setTaskStatus("failed");
      toast.error("登录测试失败", shortError(readError(requestError)));
    }
  }

  async function handleDetect() {
    if (!selectedStation) return;
    setTaskStatus("detecting");
    setError(null);
    try {
      const result = await detectStationInfo(selectedStation.id);
      setLatestSnapshot(result.snapshot);
      await Promise.all([refreshStations(), refreshSnapshot(selectedStation.id)]);
      setTaskStatus("success");
      toast.success("高级探测已完成");
    } catch (requestError) {
      setTaskStatus("failed");
      toast.error("高级探测失败", shortError(readError(requestError)));
    }
  }

  async function handleStartCapture() {
    if (!selectedStation) return;
    setTaskStatus("capturing");
    setError(null);
    try {
      const nextStatus = await startCaptureSession(selectedStation.id);
      setCaptureStatus(nextStatus);
      toast.success("实验性网页登录捕获已打开");
    } catch (requestError) {
      setTaskStatus("failed");
      toast.error("打开网页登录捕获失败", shortError(readError(requestError)));
    }
  }

  async function handleFinishCapture() {
    if (!selectedStation) return;
    setTaskStatus("finishingCapture");
    setError(null);
    try {
      const result = await finishCaptureSession(selectedStation.id);
      setLatestSnapshot(result.snapshot);
      await Promise.all([refreshStations(), refreshSnapshot(selectedStation.id), refreshCaptureStatus(selectedStation.id)]);
      setTaskStatus("success");
      toast.success("网页登录捕获快照已保存");
    } catch (requestError) {
      setTaskStatus("failed");
      toast.error("保存网页登录捕获失败", shortError(readError(requestError)));
    }
  }

  async function handleClearCapture() {
    if (!selectedStation) return;
    setError(null);
    try {
      const nextStatus = await clearCaptureSession(selectedStation.id);
      setCaptureStatus(nextStatus);
      setTaskStatus("idle");
      toast.success("捕获状态已清除");
    } catch (requestError) {
      toast.error("清除捕获状态失败", shortError(readError(requestError)));
    }
  }

  async function handleCloseCapture() {
    if (!selectedStation) return;
    setError(null);
    try {
      const nextStatus = await closeCaptureSession(selectedStation.id);
      setCaptureStatus(nextStatus);
      setTaskStatus("idle");
      toast.success("网页登录捕获窗口已关闭");
    } catch (requestError) {
      toast.error("关闭捕获窗口失败", shortError(readError(requestError)));
    }
  }

  async function handleCopyDeveloperJson() {
    const text = buildDeveloperJson(latestSnapshot);
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      toast.success("已复制脱敏 JSON");
    } catch (copyError) {
      toast.error("复制失败", readError(copyError));
    }
  }

  const actionBusy = taskStatus === "testingLogin" || taskStatus === "collecting" || taskStatus === "detecting" || taskStatus === "finishingCapture";
  const captureActive = captureStatus?.status === "capturing" || taskStatus === "capturing";
  const conclusion = conclusionLabel(summary, latestSnapshot);

  return (
    <PageScaffold
      title="信息采集"
      description="填写中转站账号密码后，先做登录态采集；高级选项里保留接口探测和网页登录捕获兜底。"
      actions={
        <div className="flex items-center gap-2">
          <SelectControl
            ariaLabel="选择采集中转站"
            className={selectClassName}
            disabled={stations.length === 0}
            placeholder="暂无中转站"
            value={selectedStationId}
            options={stations.map((station) => ({ value: station.id, label: station.name }))}
            onChange={setSelectedStationId}
          />
          <Button variant="secondary" onClick={handleCollect} disabled={actionBusy || !selectedStation}>
            <Database className="h-4 w-4" />
            {taskStatus === "collecting" ? "采集中" : "采集信息"}
          </Button>
          <Button variant="secondary" onClick={handleTestLogin} disabled={actionBusy || !selectedStation}>
            <ShieldCheck className="h-4 w-4" />
            {taskStatus === "testingLogin" ? "测试中" : "测试登录"}
          </Button>
          <Button
            variant="secondary"
            onClick={() => selectedStation && void refreshSnapshot(selectedStation.id)}
            disabled={!selectedStation || actionBusy}
          >
            <RefreshCcw className="h-4 w-4" />
            刷新
          </Button>
        </div>
      }
    >
      {loading ? (
        <div className="rounded-[var(--surface-radius)] border border-border bg-white px-4 py-5 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">
          正在读取站点和采集快照...
        </div>
      ) : !selectedStation ? (
        <EmptyState
          title="还没有可采集的站点"
          description="先在中转站添加一个站点账号，再回到这里做登录态采集。"
        />
      ) : (
        <div className="grid gap-[var(--shell-page-gap)]">
          <div className="space-y-3">
            <SectionCard
              title="采集结论"
              description={`${selectedStation.name} · ${stationTypeLabels[selectedStation.stationType]} · ${selectedStation.baseUrl}`}
              action={<StatusBadge tone={toneForConclusion(conclusion)}>{conclusion}</StatusBadge>}
            >
              <div className="grid gap-3">
                <div className="rounded-[var(--surface-radius)] border border-border bg-white p-3 shadow-[var(--surface-shadow)]">
                  <div className="flex items-start gap-3">
                    <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-[var(--surface-radius)] border border-border bg-white text-teal-700">
                      <ShieldCheck className="h-5 w-5" />
                    </div>
                    <div className="min-w-0">
                      <div className="text-sm font-semibold text-slate-800">
                        {summary.message ?? fallbackMessage(latestSnapshot)}
                      </div>
                      <div className="mt-1 text-xs leading-5 text-muted-foreground">
                        采集器：{summary.adapter ?? adapterForStation(selectedStation)} · 识别类型：
                        {summary.detectedType ?? "Unknown"}
                      </div>
                    </div>
                  </div>
                </div>
                <div className="grid gap-2 sm:grid-cols-2">
                  <CompactFact label="最近采集" value={formatDateTime(latestSnapshot?.fetchedAt)} />
                  <CompactFact label="登录状态" value={summary.loginStatus ?? loginStateLabel(latestSnapshot?.status)} />
                </div>
              </div>

              <div className="mt-3 grid gap-2 md:grid-cols-4">
                <CompactFact label="余额" value={displayValue(recognized?.balanceLabel)} />
                <CompactFact label="分组" value={countValue(recognized?.groupCount)} />
                <CompactFact label="倍率" value={countValue(recognized?.rateCount)} />
                <CompactFact label="Keys" value={countValue(recognized?.keyCount)} />
              </div>
              <div className="mt-3 grid gap-2 md:grid-cols-2">
                <CompactFact label="模型" value={countValue(modelCount)} />
                <CompactFact label="字段" value={countValue(recognized?.matchedFieldCount)} />
              </div>
              <div className="mt-3 rounded-[var(--surface-radius)] border border-border bg-white px-3 py-2 text-xs leading-5 text-muted-foreground shadow-[var(--surface-shadow)]">
                {summary.diagnosis ??
                  summary.nextStep ??
                  (summary.loginRequired
                    ? "未采集到有效登录态信息，建议先测试登录。"
                    : "采集已完成，若结果不完整可到高级选项里做进一步探测。")}
              </div>
            </SectionCard>

            <SectionCard title="采集摘要" description="主界面只展示用户看得懂的结果。">
              <div className="grid gap-2 md:grid-cols-3">
                <CompactFact label="站点账号" value={selectedStation.name} />
                <CompactFact label="站点类型" value={stationTypeLabels[selectedStation.stationType]} />
                <CompactFact label="API Keys" value={`${selectedStation.keyCount} keys`} />
              </div>
              <div className="mt-3 rounded-[var(--surface-radius)] border border-border bg-white px-3 py-2 text-sm text-slate-700 shadow-[var(--surface-shadow)]">
                {summary.loginRequired
                  ? "这个站点当前更像需要登录后才能拿到完整信息。先测试登录，再做采集。"
                  : "已尽量使用登录态接口读取余额、分组、倍率、key 和模型信息。"}
              </div>
            </SectionCard>
          </div>

          <div className="space-y-3">
            <InspectorPanel title="高级选项" description="接口探测与网页登录捕获都放这里。">
              <details className="group rounded-[var(--surface-radius)] border border-border bg-white shadow-[var(--surface-shadow)]">
                <summary className="flex cursor-pointer list-none items-center justify-between gap-2 px-3 py-2 text-sm font-medium text-slate-700">
                  高级功能
                  <ChevronDown className="h-4 w-4 text-muted-foreground transition group-open:rotate-180" />
                </summary>
                <div className="border-t border-slate-200 p-3">
                  <div className="space-y-2">
                    <Button
                      variant="outline"
                      className="w-full justify-start"
                      onClick={handleDetect}
                      disabled={actionBusy || !selectedStation}
                    >
                      <Radar className="h-4 w-4" />
                      {taskStatus === "detecting" ? "高级探测中" : "重新探测接口"}
                    </Button>
                    <div className="rounded-[var(--surface-radius)] border border-border bg-white px-3 py-2 text-xs leading-5 text-slate-700 shadow-[var(--surface-shadow)]">
                      实验功能：用于验证码、2FA 或魔改站兜底，当前需要技术验证，不保证能捕获所有请求。
                    </div>
                    {captureActive ? (
                      <div className="space-y-2">
                        <Button
                          variant="outline"
                          className="w-full justify-start"
                          onClick={handleFinishCapture}
                          disabled={actionBusy || !selectedStation}
                        >
                          <ShieldCheck className="h-4 w-4" />
                          {taskStatus === "finishingCapture" ? "保存中" : "完成采集"}
                        </Button>
                        <div className="grid gap-2">
                          <CompactFact label="捕获状态" value={captureStatusLabel(captureStatus)} />
                          <CompactFact label="接口数量" value={String(captureStatus?.captureCount ?? 0)} />
                          <CompactFact label="候选字段" value={String(captureStatus?.recognizedFieldCount ?? 0)} />
                          <CompactFact label="待确认" value={String(captureStatus?.pendingConfirmationCount ?? 0)} />
                        </div>
                        <div className="flex gap-2">
                          <Button variant="outline" className="h-7 flex-1 px-2 text-xs" onClick={handleClearCapture} disabled={actionBusy}>
                            清除状态
                          </Button>
                          <Button variant="outline" className="h-7 flex-1 px-2 text-xs" onClick={handleCloseCapture} disabled={actionBusy}>
                            关闭窗口
                          </Button>
                        </div>
                      </div>
                    ) : (
                      <Button
                        variant="outline"
                        className="w-full justify-start"
                        onClick={handleStartCapture}
                        disabled={actionBusy || !selectedStation}
                      >
                        <ShieldCheck className="h-4 w-4" />
                        网页登录捕获（实验）
                      </Button>
                    )}
                  </div>
                </div>
              </details>
            </InspectorPanel>

            <InspectorPanel title="历史快照" description="最近保存的采集记录。">
              <div className="space-y-2">
                {history.length > 0 ? (
                  history.map((snapshot) => {
                    const itemSummary = toCollectorSummary(snapshot.summaryJson);
                    return (
                      <ObjectRow
                        key={snapshot.id}
                        icon={<Activity className="h-4 w-4" />}
                        title={itemSummary.adapter ?? sourceLabel(snapshot.source)}
                        subtitle={`${formatDateTime(snapshot.fetchedAt)} · ${itemSummary.message ?? "暂无摘要"}`}
                        badges={
                          <StatusBadge tone={toneForConclusion(conclusionLabel(itemSummary, snapshot))}>
                            {conclusionLabel(itemSummary, snapshot)}
                          </StatusBadge>
                        }
                        metrics={[
                          { label: "来源", value: sourceLabel(snapshot.source) },
                          { label: "状态", value: snapshot.status },
                          { label: "字段", value: countValue(toCollectorSummary(snapshot.summaryJson).recognized?.matchedFieldCount) },
                        ]}
                        selected={snapshot.id === latestSnapshot?.id}
                        onClick={() => setLatestSnapshot(snapshot)}
                      />
                    );
                  })
                ) : (
                  <div className="rounded-[var(--surface-radius)] border border-dashed border-border bg-white px-3 py-4 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">
                    暂无历史快照。
                  </div>
                )}
              </div>
            </InspectorPanel>

            <InspectorPanel title="开发者详情" description="默认收起，仅用于排查采集器。">
              <details className="group rounded-[var(--surface-radius)] border border-border bg-white shadow-[var(--surface-shadow)]">
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
                  <pre className="max-h-72 overflow-auto rounded-xl bg-white p-3 text-[11px] leading-5 text-slate-600">
                    {buildDeveloperJson(latestSnapshot) || "暂无 snapshot。"}
                  </pre>
                </div>
              </details>
            </InspectorPanel>
          </div>
        </div>
      )}

      {actionBusy && (
        <div
          className={cn(
            "fixed bottom-4 right-4 z-40 rounded-[var(--surface-radius)] border px-4 py-3 text-sm shadow-[var(--surface-shadow)]",
            "border-cyan-200 bg-cyan-50 text-cyan-700",
          )}
        >
          {taskStatusLabel(taskStatus, summary)}
        </div>
      )}
    </PageScaffold>
  );
}

function CompactFact({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-[var(--surface-radius)] border border-border bg-white px-3 py-2 shadow-[var(--surface-shadow)]">
      <div className="text-[11px] text-muted-foreground">{label}</div>
      <div className="mt-0.5 truncate text-sm font-semibold text-slate-800">{value}</div>
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
    loginStatus: readString(value.loginStatus),
    loginRequired: typeof value.loginRequired === "boolean" ? value.loginRequired : undefined,
    nextStep: readString(value.nextStep),
    diagnosis: readString(value.diagnosis),
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
  if (status === "missing_credentials") return "缺少账号";
  return "已检查";
}

function loginStateLabel(status: string | undefined) {
  if (!status) return "未识别";
  if (status === "success") return "已登录";
  if (status === "manual_required") return "需要登录";
  if (status === "missing_credentials") return "缺少账号密码";
  return statusLabel(status) ?? "未识别";
}

function taskStatusLabel(status: TaskStatus, summary: CollectorSummary) {
  if (status === "testingLogin") return "正在测试登录...";
  if (status === "detecting") return "正在执行高级探测...";
  if (status === "collecting") return "正在采集登录态信息...";
  if (status === "capturing") return "网页登录捕获窗口已打开";
  if (status === "finishingCapture") return "正在保存网页登录捕获快照...";
  if (status === "success") return summary.message ?? "任务完成";
  if (status === "failed") return "任务失败";
  return "空闲";
}

function captureStatusLabel(status: CaptureSessionStatus | null) {
  if (!status || status.status === "idle") return "未开始";
  if (status.status === "capturing") return "捕获中";
  if (status.status === "failed") return "失败";
  return status.status;
}

function fallbackMessage(snapshot: CollectorSnapshot | null) {
  if (!snapshot) return "尚未采集。请选择站点后点击采集信息。";
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
  if (station.stationType === "sub2api") return "Login State Adapter";
  if (station.stationType === "newapi") return "NewAPI Adapter（待接入）";
  if (station.stationType === "openai-compatible") return "OpenAI-compatible Adapter（基础探测）";
  return "Auto Detect";
}

function sourceLabel(source: string) {
  if (source.includes("detect")) return "接口探测";
  if (source.includes("collect")) return "信息采集";
  if (source.includes("login-state")) return "登录态采集";
  return "采集快照";
}

function toneForConclusion(value: string) {
  if (value === "可用" || value === "已采集") return "healthy" as const;
  if (value === "需要登录" || value === "未识别" || value === "已检查" || value === "缺少账号") return "warning" as const;
  if (value === "失败") return "error" as const;
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

const selectClassName =
  "h-8 rounded-xl border border-cyan-100 bg-cyan-50/45 px-3 text-sm text-slate-800 outline-none transition focus:border-teal-300 focus:bg-white focus:ring-2 focus:ring-teal-100";
