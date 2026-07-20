import { useEffect, useMemo, useState } from "react";
import { relaunch } from "@tauri-apps/plugin-process";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/Card";
import { useUpdater } from "@/features/updater/UpdaterProvider";
import { isUpdaterBusyPhase } from "@/features/updater/updateState";
import {
  activateDataStoreCandidate,
  createNewDataStore,
  exportDataStoreDiagnostic,
  locateDataStoreCandidate,
  openDataStoreBackupDir,
} from "@/lib/api/dataRecovery";
import { tauriErrorMessage } from "@/lib/tauriErrors";
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
          await relaunch();
        } catch {
          setMessage("配置已保存，请手动重启 Relay Pool。");
        }
        return;
      }
      onActivated();
    } catch (error) {
      setMessage(tauriErrorMessage(error));
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
      setMessage(tauriErrorMessage(error));
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
          await relaunch();
        } catch {
          setMessage("配置已保存，请手动重启 Relay Pool。");
        }
        return;
      }
      onActivated();
    } catch (error) {
      setMessage(tauriErrorMessage(error));
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
      setMessage(tauriErrorMessage(error));
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
      setMessage(tauriErrorMessage(error));
    } finally {
      setActiveOperation(null);
    }
  }

  return (
    <main className="min-h-screen bg-app px-6 py-8 text-foreground">
      <div className="mx-auto flex w-full max-w-[960px] flex-col gap-4">
        <Card className="p-6">
          <p className="text-sm font-semibold text-warning-foreground">{viewModel.eyebrow}</p>
          <h1 className="mt-2 text-2xl font-semibold">{viewModel.title}</h1>
          <p className="mt-3 max-w-[720px] text-sm leading-6 text-muted-foreground">{viewModel.description}</p>
          <p className="mt-3 text-xs text-muted-foreground">
            Generation {state.databaseGeneration === "two" ? "2" : "1"}
            {state.compatibility?.schemaVersion === null || !state.compatibility
              ? ""
              : ` · schema ${state.compatibility.schemaVersion}`}
            {state.compatibility ? ` · 应用 ${state.compatibility.appVersion}` : ""}
          </p>
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
                {activeOperation === "activate" ? "正在保存" : "使用选中的数据库并重启"}
              </Button>
            ) : null}
            {state.capabilities.canLocateCandidate ? (
              <Button variant="secondary" disabled={busy} onClick={locateCandidate}>手动定位数据库</Button>
            ) : null}
            {state.capabilities.canCreateDataStore ? (
              <Button variant="secondary" disabled={!confirmed || busy} onClick={createFreshDataStore}>新建 generation 2 数据库</Button>
            ) : null}
            {state.capabilities.canExportDiagnostic ? (
              <Button variant="outline" disabled={busy} onClick={exportDiagnostic}>导出诊断</Button>
            ) : null}
            {state.capabilities.canBackup ? (
              <Button variant="outline" disabled={busy} onClick={openBackupDir}>打开备份目录</Button>
            ) : null}
            {state.capabilities.canCheckForUpdates ? (
              <Button
                variant="outline"
                disabled={busy || isUpdaterBusyPhase(updaterState.phase)}
                onClick={() => void checkForUpdates()}
              >
                检查更新
              </Button>
            ) : null}
          </div>
          {message ? <p className="text-sm text-warning-foreground">{message}</p> : null}
        </Card>
      </div>
    </main>
  );
}
