import { useEffect, useMemo, useState, type ReactNode } from "react";
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
import { usePageActivation } from "@/components/shell/PageActivity";
import { Button, EmptyState, InspectorPanel, ObjectRow, SectionCard, SelectControl, StatusBadge, useToast } from "@/components/ui";
import { readError } from "@/lib/errors";
import {
  collectStationTask,
  clearCaptureSession,
  detectStationInfo,
  closeCaptureSession,
  finishWebAuthorizationSession,
  getCaptureSessionStatus,
  getLatestCollectorSnapshot,
  listCollectorSnapshots,
  startCaptureSession,
  testStationLogin,
} from "@/lib/api/collector";
import { listCollectorRuns } from "@/lib/api/collectorRuns";
import { updateStationSession } from "@/lib/api/stationKeys";
import { listStations } from "@/lib/api/stations";
import type {
  CaptureSessionStatus,
  CollectorEndpointResult,
  CollectorSnapshot,
  CollectorSummary,
} from "@/lib/types/collector";
import type { CollectorRun, CollectorTaskType } from "@/lib/types/collectorRuns";
import { stationTypeLabels, type Station } from "@/lib/types/stations";
import { cn } from "@/lib/utils";
import { CollectorAdvancedSettings } from "./CollectorAdvancedSettings";

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
  const [taskType, setTaskType] = useState<CollectorTaskType>("full");
  const [runs, setRuns] = useState<CollectorRun[]>([]);
  const [manualSession, setManualSession] = useState({
    accessToken: "",
    refreshToken: "",
    cookie: "",
    newapiUserId: "",
    tokenExpiresAt: "",
  });
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
  const groupDetails = Array.isArray(normalized.groups) ? normalized.groups : [];
  const rateMultiplierDetails = Array.isArray(normalized.rateMultipliers)
    ? normalized.rateMultipliers
    : [];

  usePageActivation(({ isInitial }) => {
    void refreshStations(isInitial);
    if (!isInitial && selectedStation) {
      void refreshSnapshot(selectedStation.id);
      void refreshCaptureStatus(selectedStation.id);
      void refreshRuns(selectedStation.id);
    }
  });

  useEffect(() => {
    if (!selectedStation) {
      setLatestSnapshot(null);
      setHistory([]);
      setRuns([]);
      return;
    }
    void refreshSnapshot(selectedStation.id);
    void refreshCaptureStatus(selectedStation.id);
    void refreshRuns(selectedStation.id);
  }, [selectedStation?.id]);

  async function refreshStations(showLoading = true) {
    if (showLoading) {
      setLoading(true);
    }
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
      if (showLoading) {
        setLoading(false);
      }
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

  async function refreshRuns(stationId: string) {
    try {
      setRuns(await listCollectorRuns(stationId));
    } catch (requestError) {
      toast.error("刷新采集任务失败", readError(requestError));
    }
  }

  async function handleCollect() {
    if (!selectedStation) return;
    setTaskStatus("collecting");
    setError(null);
    try {
      const result = await collectStationTask(selectedStation.id, taskType);
      setLatestSnapshot(result.snapshot);
      await Promise.all([
        refreshStations(),
        refreshSnapshot(selectedStation.id),
        refreshRuns(selectedStation.id),
      ]);
      setTaskStatus("success");
      toast.success("采集任务已完成");
    } catch (requestError) {
      setTaskStatus("failed");
      toast.error("采集任务失败", shortError(readError(requestError)));
    }
  }

  async function handleSaveManualSession() {
    if (!selectedStation) return;
    setError(null);
    try {
      await updateStationSession({
        stationId: selectedStation.id,
        accessToken: manualSession.accessToken || null,
        refreshToken: manualSession.refreshToken || null,
        cookie: manualSession.cookie || null,
        newapiUserId: manualSession.newapiUserId || null,
        tokenExpiresAt: manualSession.tokenExpiresAt || null,
      });
      setManualSession({
        accessToken: "",
        refreshToken: "",
        cookie: "",
        newapiUserId: "",
        tokenExpiresAt: "",
      });
      toast.success("手动登录态已保存");
    } catch (requestError) {
      toast.error("保存手动登录态失败", shortError(readError(requestError)));
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
      toast.success("网页登录授权窗口已打开");
    } catch (requestError) {
      setTaskStatus("failed");
      toast.error("打开网页登录授权失败", shortError(readError(requestError)));
    }
  }

  async function handleFinishCapture() {
    if (!selectedStation) return;
    setTaskStatus("finishingCapture");
    setError(null);
    try {
      const result = await finishWebAuthorizationSession(selectedStation.id);
      setLatestSnapshot(result.snapshot);
      await Promise.all([refreshStations(), refreshSnapshot(selectedStation.id), refreshCaptureStatus(selectedStation.id)]);
      setTaskStatus("success");
      toast.success("网页登录授权已保存");
    } catch (requestError) {
      setTaskStatus("failed");
      toast.error("保存网页登录授权失败", shortError(readError(requestError)));
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
      toast.success("网页登录授权窗口已关闭");
    } catch (requestError) {
      toast.error("关闭授权窗口失败", shortError(readError(requestError)));
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
          <SelectControl
            ariaLabel="采集任务"
            className={selectClassName}
            disabled={!selectedStation || actionBusy}
            value={taskType}
            options={[
              { value: "detect", label: "探测" },
              { value: "balance", label: "余额" },
              { value: "groups", label: "分组 / 倍率" },
              { value: "models", label: "模型" },
              { value: "full", label: "完整采集" },
            ]}
            onChange={(value) => setTaskType(value as CollectorTaskType)}
          />
          <Button variant="secondary" onClick={handleCollect} disabled={actionBusy || !selectedStation}>
            <Database className="h-4 w-4" />
            {taskStatus === "collecting" ? "采集中" : "运行任务"}
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
              description={`${selectedStation.name} · ${stationTypeLabels[selectedStation.stationType]} · ${selectedStation.websiteUrl}`}
              action={<StatusBadge tone={toneForConclusion(conclusion)}>{conclusion}</StatusBadge>}
            >
              <div className="grid gap-3">
                <div className="rounded-[10px] bg-slate-50/70 p-3">
                  <div className="flex items-start gap-3">
                    <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-[10px] bg-white text-teal-700">
                      <ShieldCheck className="h-5 w-5" />
                    </div>
                    <div className="min-w-0">
                      <div className="text-sm font-semibold text-slate-800">
                        {summary.message ?? fallbackMessage(latestSnapshot)}
                      </div>
                      <div className="mt-1 text-xs leading-5 text-muted-foreground">
                        采集器：{summary.adapter ?? adapterForStation(selectedStation)} · 识别类型：
                        {summary.detectedType ?? "未知"}
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
                <CompactFact label="密钥" value={countValue(recognized?.keyCount)} />
              </div>
              <div className="mt-3 grid gap-2 md:grid-cols-2">
                <CompactFact label="模型" value={countValue(modelCount)} />
                <CompactFact label="字段" value={countValue(recognized?.matchedFieldCount)} />
              </div>
              {(groupDetails.length > 0 || rateMultiplierDetails.length > 0) && (
                <div className="mt-3 grid gap-2 md:grid-cols-2">
                  <DetailList
                    title="分组明细"
                    items={groupDetails.map(formatGroupDetail)}
                    emptyText="未识别到分组名称"
                  />
                  <DetailList
                    title="倍率明细"
                    items={rateMultiplierDetails.map(formatRateMultiplierDetail)}
                    emptyText="未识别到分组倍率"
                  />
                </div>
              )}
              <div className="mt-3 rounded-[10px] bg-slate-50/70 px-3 py-2 text-xs leading-5 text-muted-foreground">
                {summary.diagnosis ??
                  summary.nextStep ??
                  (summary.loginRequired
                    ? "未采集到有效登录态信息，建议先测试登录。"
                    : "采集已完成，若结果不完整可到高级选项里做进一步探测。")}
              </div>
            </SectionCard>

            <SectionCard title="采集摘要">
              <div className="grid gap-2 md:grid-cols-3">
                <CompactFact label="站点账号" value={selectedStation.name} />
                <CompactFact label="站点类型" value={stationTypeLabels[selectedStation.stationType]} />
                <CompactFact label="密钥" value={`${selectedStation.keyCount}`} />
              </div>
              <div className="mt-3 rounded-[10px] bg-slate-50/70 px-3 py-2 text-sm text-slate-700">
                {summary.loginRequired
                  ? "这个站点当前更像需要登录后才能拿到完整信息。先测试登录，再做采集。"
                  : "已尽量使用登录态接口读取余额、分组、倍率、密钥和模型信息。"}
              </div>
            </SectionCard>

            <SectionCard
              title="手动登录态"
              description="保存后不会回显原文。"
              action={
                <Button variant="secondary" onClick={handleSaveManualSession} disabled={!selectedStation || actionBusy}>
                  保存登录态
                </Button>
              }
            >
              <div className="grid gap-3 md:grid-cols-2">
                <Field label="访问令牌">
                  <input
                    type="password"
                    className={inputClassName}
                    value={manualSession.accessToken}
                    onChange={(event) =>
                      setManualSession({ ...manualSession, accessToken: event.target.value })
                    }
                  />
                </Field>
                <Field label="刷新令牌">
                  <input
                    type="password"
                    className={inputClassName}
                    value={manualSession.refreshToken}
                    onChange={(event) =>
                      setManualSession({ ...manualSession, refreshToken: event.target.value })
                    }
                  />
                </Field>
                <Field label="NewAPI 用户 ID">
                  <input
                    className={inputClassName}
                    value={manualSession.newapiUserId}
                    onChange={(event) =>
                      setManualSession({ ...manualSession, newapiUserId: event.target.value })
                    }
                  />
                </Field>
                <Field label="Cookie">
                  <input
                    type="password"
                    className={inputClassName}
                    value={manualSession.cookie}
                    onChange={(event) =>
                      setManualSession({ ...manualSession, cookie: event.target.value })
                    }
                  />
                </Field>
                <Field label="令牌过期时间">
                  <input
                    className={inputClassName}
                    placeholder="Unix ms 或 ISO 时间"
                    value={manualSession.tokenExpiresAt}
                    onChange={(event) =>
                      setManualSession({ ...manualSession, tokenExpiresAt: event.target.value })
                    }
                  />
                </Field>
              </div>
            </SectionCard>
          </div>

          <div className="space-y-3">
            <CollectorAdvancedSettings />

            <InspectorPanel title="高级选项">
              <details className="group rounded-[10px] border border-slate-100 bg-slate-50/70">
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
                    <div className="rounded-[10px] bg-white/75 px-3 py-2 text-xs leading-5 text-slate-700">
                      用于验证码、二次验证或魔改站网页登录授权，完成登录后会验证后台会话并保存可复用的登录态。
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
                        网页登录授权
                      </Button>
                    )}
                  </div>
                </div>
              </details>
            </InspectorPanel>

            <SectionCard title="最近采集任务">
              <div className="grid gap-2">
                {runs.length === 0 ? (
                  <div className="rounded-[10px] border border-dashed border-slate-200 bg-slate-50/70 px-3 py-4 text-sm text-muted-foreground">
                    暂无采集任务。
                  </div>
                ) : (
                  runs.slice(0, 10).map((run) => (
                    <div
                      key={run.id}
                      className="grid grid-cols-[5rem_7rem_minmax(0,1fr)_5rem] items-center gap-2 rounded-[10px] border border-slate-100 bg-slate-50/70 px-3 py-2 text-xs"
                    >
                      <span className="font-medium text-slate-700">{taskTypeLabel(run.taskType)}</span>
                      <StatusBadge tone={toneForRunStatus(run.status)}>{runStatusLabel(run.status)}</StatusBadge>
                      <span className="truncate text-muted-foreground">
                        {run.errorMessage ?? `${run.successCount}/${run.endpointCount} 接口`}
                      </span>
                      <span className="text-right text-muted-foreground">
                        {run.durationMs == null ? "-" : `${run.durationMs}ms`}
                      </span>
                    </div>
                  ))
                )}
              </div>
            </SectionCard>

            <InspectorPanel title="历史快照">
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
                  <div className="rounded-[10px] border border-dashed border-slate-200 bg-slate-50/70 px-3 py-4 text-sm text-muted-foreground">
                    暂无历史快照。
                  </div>
                )}
              </div>
            </InspectorPanel>

            <InspectorPanel title="开发者详情" description="默认收起，仅用于排查采集器。">
              <details className="group rounded-[10px] border border-slate-100 bg-slate-50/70">
                <summary className="flex cursor-pointer list-none items-center justify-between gap-2 px-3 py-2 text-sm font-medium text-slate-700">
                  脱敏快照 JSON
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
                        {buildDeveloperJson(latestSnapshot) || "暂无快照。"}
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
    <div className="rounded-[10px] border border-slate-100 bg-slate-50/70 px-3 py-2">
      <div className="text-[11px] text-muted-foreground">{label}</div>
      <div className="mt-0.5 truncate text-sm font-semibold text-slate-800">{value}</div>
    </div>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="grid gap-1.5">
      <span className="text-xs font-medium text-slate-700">{label}</span>
      {children}
    </label>
  );
}

function DetailList({
  title,
  items,
  emptyText,
}: {
  title: string;
  items: string[];
  emptyText: string;
}) {
  return (
    <div className="rounded-[10px] border border-slate-100 bg-slate-50/70 px-3 py-2">
      <div className="text-[11px] text-muted-foreground">{title}</div>
      <div className="mt-1 flex flex-wrap gap-1.5">
        {items.length > 0 ? (
          items.map((item, index) => (
            <span
              key={`${item}-${index}`}
              className="rounded-md border border-slate-200 bg-slate-50 px-2 py-1 text-xs font-medium text-slate-700"
            >
              {item}
            </span>
          ))
        ) : (
          <span className="text-xs text-muted-foreground">{emptyText}</span>
        )}
      </div>
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

function formatGroupDetail(value: unknown) {
  if (typeof value === "string" || typeof value === "number") return String(value);
  if (value && typeof value === "object") {
    const record = value as Record<string, unknown>;
    return readString(record.name) ?? readString(record.group) ?? readString(record.groupName) ?? displayValue(value);
  }
  return displayValue(value);
}

function formatRateMultiplierDetail(value: unknown) {
  if (typeof value === "string" || typeof value === "number") return String(value);
  if (value && typeof value === "object") {
    const record = value as Record<string, unknown>;
    const group = readString(record.group) ?? readString(record.groupName) ?? readString(record.name);
    const multiplier = record.multiplier ?? record.rateMultiplier ?? record.ratio;
    if (group && multiplier !== undefined && multiplier !== null) {
      return `${group} = ${displayValue(multiplier)}`;
    }
  }
  return displayValue(value);
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
  if (status === "capturing") return "网页登录授权窗口已打开";
  if (status === "finishingCapture") return "正在保存网页登录授权...";
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
  if (station.stationType === "sub2api") return "登录态采集";
  if (station.stationType === "newapi") return "NewAPI 采集（待接入）";
  if (station.stationType === "openai-compatible") return "OpenAI 兼容探测";
  return "自动探测";
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

function toneForRunStatus(status: string) {
  if (status === "success") return "healthy" as const;
  if (status === "failed") return "error" as const;
  if (status === "manual_required") return "warning" as const;
  if (status === "running" || status === "partial") return "info" as const;
  return "info" as const;
}

function taskTypeLabel(value: string) {
  if (value === "detect") return "探测";
  if (value === "balance") return "余额";
  if (value === "groups") return "分组";
  if (value === "models") return "模型";
  if (value === "full") return "完整";
  return value;
}

function runStatusLabel(status: string) {
  if (status === "success") return "成功";
  if (status === "failed") return "失败";
  if (status === "manual_required") return "需要登录";
  if (status === "running") return "运行中";
  if (status === "partial") return "部分完成";
  return status;
}

function shortError(error: string) {
  if (/timeout|timed out/i.test(error)) return "站点请求超时";
  if (/resolve|dns/i.test(error)) return "域名解析失败";
  if (/connection refused/i.test(error)) return "站点拒绝连接";
  if (/certificate|tls/i.test(error)) return "TLS 证书或连接失败";
  return error.length > 120 ? `${error.slice(0, 120)}...` : error;
}


const selectClassName =
  "h-8 rounded-xl border border-cyan-100 bg-cyan-50/45 px-3 text-sm text-slate-800 outline-none transition focus:border-teal-300 focus:bg-white focus:ring-2 focus:ring-teal-100";

const inputClassName =
  "h-9 w-full rounded-[var(--surface-radius)] border border-border bg-white px-3 text-sm text-slate-800 outline-none transition placeholder:text-slate-400 focus:border-teal-300 focus:ring-2 focus:ring-teal-100";
