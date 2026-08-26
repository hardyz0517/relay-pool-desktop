import { useEffect, useMemo, useState } from "react";
import { AlertTriangle, Check, Circle, Database, Download, FolderOpen, RefreshCw, Search, Wrench } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/Card";
import { useUpdater } from "@/lib/updater/UpdaterProvider";
import { isUpdaterBusyPhase } from "@/lib/updater/updateState";
import {
  activateDataStoreCandidate,
  createNewDataStore,
  exportDataStoreDiagnostic,
  locateDataStoreCandidate,
  openDataStoreBackupDir,
  restartApp,
} from "@/lib/api/dataRecovery";
import { errorMessage } from "@/lib/errorMessage";
import type { DataStoreStartupView } from "@/lib/types/dataRecovery";
import { buildRecoveryViewModel } from "./recoveryViewModel";

type DataRecoveryScreenProps = {
  state: DataStoreStartupView;
  onActivated: () => void;
};

export function DataRecoveryScreen({ state, onActivated }: DataRecoveryScreenProps) {
  const { state: updaterState, checkNow: checkForUpdates } = useUpdater();
  const [locatedCandidates, setLocatedCandidates] = useState(state.candidates);
  const currentState = useMemo(
    () => ({ ...state, candidates: locatedCandidates }),
    [locatedCandidates, state],
  );
  const viewModel = useMemo(() => buildRecoveryViewModel(currentState), [currentState]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [activeOperation, setActiveOperation] = useState<"activate" | "locate" | "create" | "diagnostic" | "backup" | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    setLocatedCandidates(state.candidates);
    setSelectedId(null);
    setConfirmed(false);
  }, [state]);

  const selected = viewModel.candidates.find((candidate) => candidate.id === selectedId) ?? null;
  const busy = activeOperation !== null;
  const canActivate = Boolean(selected?.selectable && confirmed && !busy);

  async function activateSelected() {
    if (!selected || !canActivate) return;
    setActiveOperation("activate");
    setMessage(null);
    try {
      const result = await activateDataStoreCandidate(selected.id);
      if (result.restartRequired) {
        try {
          await restartApp();
        } catch {
          setMessage("配置已保存，请手动重启 Relay Pool。");
        }
        return;
      }
      onActivated();
    } catch (error) {
      setMessage(errorMessage(error));
    } finally {
      setActiveOperation(null);
    }
  }

  async function locateCandidate() {
    if (busy || !state.capabilities.canLocateCandidate) return;
    setActiveOperation("locate");
    setMessage(null);
    try {
      const candidate = await locateDataStoreCandidate();
      if (!candidate) return;
      setLocatedCandidates((candidates) => [
        ...candidates.filter((item) => item.path !== candidate.path),
        candidate,
      ]);
      setSelectedId(candidate.id);
    } catch (error) {
      setMessage(errorMessage(error));
    } finally {
      setActiveOperation(null);
    }
  }

  async function createFreshDataStore() {
    if (busy || !confirmed || !state.capabilities.canCreateDataStore) return;
    setActiveOperation("create");
    setMessage(null);
    try {
      const result = await createNewDataStore(confirmed);
      if (result.restartRequired) {
        try {
          await restartApp();
        } catch {
          setMessage("配置已保存，请手动重启 Relay Pool。");
        }
        return;
      }
      onActivated();
    } catch (error) {
      setMessage(errorMessage(error));
    } finally {
      setActiveOperation(null);
    }
  }

  async function exportDiagnostic() {
    if (busy || !state.capabilities.canExportDiagnostic) return;
    setActiveOperation("diagnostic");
    setMessage(null);
    try {
      const path = await exportDataStoreDiagnostic();
      if (path) setMessage(`诊断文件已导出：${path}`);
    } catch (error) {
      setMessage(errorMessage(error));
    } finally {
      setActiveOperation(null);
    }
  }

  async function openBackupDir() {
    if (busy || !state.capabilities.canBackup) return;
    setActiveOperation("backup");
    setMessage(null);
    try {
      await openDataStoreBackupDir();
    } catch (error) {
      setMessage(errorMessage(error));
    } finally {
      setActiveOperation(null);
    }
  }

  return (
    <main className="min-h-screen bg-app px-4 py-6 text-foreground sm:px-6 sm:py-8">
      <div className="mx-auto flex w-full max-w-[1040px] flex-col gap-4">
        <Card className="overflow-hidden">
          <div className="border-b border-border px-5 py-5 sm:px-6">
            <div className="flex flex-wrap items-start justify-between gap-4">
              <div>
                <p className="text-xs font-semibold uppercase tracking-wide text-muted-foreground">{viewModel.eyebrow}</p>
                <h1 className="mt-2 text-2xl font-semibold tracking-tight">{viewModel.title}</h1>
                <p className="mt-2 max-w-[760px] text-sm leading-6 text-muted-foreground">{viewModel.description}</p>
              </div>
              <div className={`inline-flex items-center gap-2 rounded-full border px-3 py-1.5 text-xs font-medium ${state.upgrade.stage === "blocked" ? "border-danger/40 bg-danger/10 text-danger-foreground" : "border-border bg-muted/40 text-muted-foreground"}`}>
                {state.upgrade.stage === "blocked" ? <AlertTriangle size={14} aria-hidden="true" /> : <Database size={14} aria-hidden="true" />}
                {state.upgrade.stage === "blocked" ? "启动已阻断" : "本地数据库"}
              </div>
            </div>
            <div className="mt-5 grid gap-3 sm:grid-cols-3">
              <div className="rounded-md border border-border bg-muted/20 px-3 py-2.5">
                <p className="text-xs text-muted-foreground">当前 schema</p>
                <p className="mt-1 font-mono text-sm">{state.upgrade.currentSchemaVersion ?? "未知"}</p>
              </div>
              <div className="rounded-md border border-border bg-muted/20 px-3 py-2.5">
                <p className="text-xs text-muted-foreground">目标 schema</p>
                <p className="mt-1 font-mono text-sm">{state.upgrade.targetSchemaVersion}</p>
              </div>
              <div className="rounded-md border border-border bg-muted/20 px-3 py-2.5">
                <p className="text-xs text-muted-foreground">升级状态</p>
                <p className="mt-1 text-sm font-medium">{viewModel.upgradeSummary}</p>
              </div>
            </div>
          </div>
          <div className="px-5 py-5 sm:px-6">
            <div className="grid gap-3 sm:grid-cols-4">
              {viewModel.upgradeSteps.map((step, index) => (
                <div key={step.id} className="relative flex items-start gap-2">
                  {index > 0 ? <span className="absolute -left-3 top-3 hidden h-px w-2 bg-border sm:block" aria-hidden="true" /> : null}
                  <span className={`mt-0.5 flex size-6 shrink-0 items-center justify-center rounded-full border ${step.state === "done" ? "border-success/50 bg-success/10 text-success-foreground" : step.state === "blocked" ? "border-danger/50 bg-danger/10 text-danger-foreground" : step.state === "active" ? "border-primary/50 bg-primary/10 text-primary" : "border-border text-muted-foreground"}`}>
                    {step.state === "done" ? <Check size={14} aria-hidden="true" /> : step.state === "blocked" ? <AlertTriangle size={14} aria-hidden="true" /> : step.state === "active" ? <RefreshCw size={14} aria-hidden="true" /> : <Circle size={10} aria-hidden="true" />}
                  </span>
                  <div className="min-w-0">
                    <p className="text-sm font-medium">{step.label}</p>
                    <p className="mt-0.5 text-xs text-muted-foreground">{step.state === "done" ? "已完成" : step.state === "blocked" ? "需要处理" : step.state === "active" ? "当前阶段" : "等待中"}</p>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </Card>

        <div className="grid gap-3">
          {viewModel.candidates.length === 0 ? (
              <Card className="p-5 text-sm text-muted-foreground">
                当前没有可直接选择的候选数据库。请保留现场，等待后续诊断/定位功能处理。
            </Card>
          ) : viewModel.candidates.map((candidate) => (
            <label
              key={candidate.id}
              className="block"
            >
              <Card className="p-4">
                <div className="flex items-start gap-3">
                  <input
                    className="mt-1"
                    type="radio"
                    name="data-store-candidate"
                    disabled={!candidate.selectable}
                    checked={selectedId === candidate.id}
                    onChange={() => setSelectedId(candidate.id)}
                  />
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
                      <span>{candidate.roleLabel}</span>
                      <span>·</span>
                      <span>{candidate.healthLabel}</span>
                      <span>·</span>
                      <span>{candidate.generationLabel}</span>
                      <span>·</span>
                      <span>{candidate.schemaLabel}</span>
                    </div>
                    <p className="mt-2 break-all font-mono text-xs text-foreground">{candidate.path}</p>
                    <p className="mt-2 text-sm text-muted-foreground">{candidate.summary}</p>
                    <p className="mt-1 text-xs text-muted-foreground">{candidate.metadata}</p>
                    {candidate.disabledReason ? (
                      <p className="mt-2 text-xs text-danger-foreground">不可选择：{candidate.disabledReason}</p>
                    ) : null}
                  </div>
                </div>
              </Card>
            </label>
          ))}
        </div>

        <Card className="flex flex-col gap-3 p-5">
          {viewModel.requiresDestructiveActionConfirmation
            && (state.capabilities.canActivateCandidate || state.capabilities.canCreateDataStore) ? (
              <label className="flex items-start gap-2 text-sm text-muted-foreground">
                <input
                  className="mt-1"
                  type="checkbox"
                  checked={confirmed}
                  onChange={(event) => setConfirmed(event.currentTarget.checked)}
                />
                <span>我确认要执行选中的恢复动作。未选中的文件不会被删除、覆盖或合并。</span>
              </label>
            ) : null}
          <div className="flex flex-wrap items-center gap-3">
            {state.capabilities.canActivateCandidate ? (
            <Button disabled={!canActivate} onClick={activateSelected}>
                {activeOperation === "activate" ? <RefreshCw className="mr-2 animate-spin" size={15} aria-hidden="true" /> : <Check className="mr-2" size={15} aria-hidden="true" />}
                {activeOperation === "activate" ? "正在保存" : "使用选中的数据库并重启"}
            </Button>
            ) : null}
            {state.capabilities.canLocateCandidate ? (
              <Button variant="secondary" disabled={busy} onClick={locateCandidate}><Search className="mr-2" size={15} aria-hidden="true" />手动定位数据库</Button>
            ) : null}
            {state.capabilities.canCreateDataStore ? (
              <Button variant="secondary" disabled={!confirmed || busy} onClick={createFreshDataStore}><Database className="mr-2" size={15} aria-hidden="true" />新建本地数据库</Button>
            ) : null}
            {state.capabilities.canExportDiagnostic ? (
              <Button variant="outline" disabled={busy} onClick={exportDiagnostic}><Download className="mr-2" size={15} aria-hidden="true" />导出诊断</Button>
            ) : null}
            {state.capabilities.canBackup ? (
              <Button variant="outline" disabled={busy} onClick={openBackupDir}><FolderOpen className="mr-2" size={15} aria-hidden="true" />打开备份目录</Button>
            ) : null}
            {state.capabilities.canCheckForUpdates ? (
              <Button
                variant="outline"
                disabled={busy || isUpdaterBusyPhase(updaterState.phase)}
                onClick={() => void checkForUpdates()}
              >
                <Wrench className="mr-2" size={15} aria-hidden="true" />检查更新
              </Button>
            ) : null}
          </div>
          {message ? <p className="text-sm text-warning-foreground">{message}</p> : null}
        </Card>
      </div>
    </main>
  );
}
