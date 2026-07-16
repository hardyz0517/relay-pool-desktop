import { useMemo, useState } from "react";
import { relaunch } from "@tauri-apps/plugin-process";

import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/Card";
import { activateDataStoreCandidate } from "@/lib/api/dataRecovery";
import { tauriErrorMessage } from "@/lib/tauriErrors";
import type { DataStoreStartupView } from "@/lib/types/dataRecovery";
import { buildRecoveryViewModel } from "./recoveryViewModel";

type DataRecoveryScreenProps = {
  state: DataStoreStartupView;
  onActivated: () => void;
};

export function DataRecoveryScreen({ state, onActivated }: DataRecoveryScreenProps) {
  const viewModel = useMemo(() => buildRecoveryViewModel(state), [state]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  const selected = viewModel.candidates.find((candidate) => candidate.id === selectedId) ?? null;
  const canActivate = Boolean(selected?.selectable && confirmed && !submitting);

  async function activateSelected() {
    if (!selected || !canActivate) return;
    setSubmitting(true);
    setMessage(null);
    try {
      const result = await activateDataStoreCandidate(selected.path);
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
      setSubmitting(false);
    }
  }

  return (
    <main className="min-h-screen bg-app px-6 py-8 text-foreground">
      <div className="mx-auto flex w-full max-w-[960px] flex-col gap-4">
        <Card className="p-6">
          <p className="text-sm font-semibold text-warning-foreground">{viewModel.title}</p>
          <h1 className="mt-2 text-2xl font-semibold tracking-[-0.02em]">需要确认本地数据位置</h1>
          <p className="mt-3 max-w-[720px] text-sm leading-6 text-muted-foreground">{viewModel.description}</p>
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
          <label className="flex items-start gap-2 text-sm text-muted-foreground">
            <input
              className="mt-1"
              type="checkbox"
              checked={confirmed}
              onChange={(event) => setConfirmed(event.currentTarget.checked)}
            />
            <span>我确认要把选中的数据库作为 Relay Pool 当前本地数据。未选中的文件不会被删除、覆盖或合并。</span>
          </label>
          <div className="flex flex-wrap items-center gap-3">
            <Button disabled={!canActivate} onClick={activateSelected}>
              {submitting ? "正在保存" : "使用选中的数据库并重启"}
            </Button>
            <span className="text-xs text-muted-foreground">手动定位、新建数据库、导出诊断和打开备份目录会在下一阶段接入。</span>
          </div>
          {message ? <p className="text-sm text-warning-foreground">{message}</p> : null}
        </Card>
      </div>
    </main>
  );
}
