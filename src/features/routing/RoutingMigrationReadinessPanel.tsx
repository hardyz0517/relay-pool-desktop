import { useEffect, useMemo, useState } from "react";
import { Button, SectionCard, StatusBadge } from "@/components/ui";
import type {
  AppSettings,
  HierarchicalRoutingAffinityMode,
  HierarchicalRoutingOrderingProfile,
} from "@/lib/types/settings";
import {
  createRoutingMigrationDraft,
  evaluateRoutingMigrationReadiness,
  formatRoutingGroupScope,
  routingAffinityModeLabels,
  routingMigrationIssueLabels,
  routingOrderingProfileLabels,
  type RoutingMigrationReadinessDraft,
} from "./routingMigrationReadiness";

type RoutingMigrationReadinessPanelProps = {
  settings: AppSettings | null;
  loading: boolean;
  saving: boolean;
  onConfirm: (draft: RoutingMigrationReadinessDraft) => void;
};

const orderingProfiles: HierarchicalRoutingOrderingProfile[] = ["priority_first", "cost_first"];
const affinityModes: HierarchicalRoutingAffinityMode[] = ["disabled", "session", "previous_response"];

export function RoutingMigrationReadinessPanel({
  settings,
  loading,
  saving,
  onConfirm,
}: RoutingMigrationReadinessPanelProps) {
  const [draft, setDraft] = useState<RoutingMigrationReadinessDraft | null>(null);

  useEffect(() => {
    setDraft(settings ? createRoutingMigrationDraft(settings) : null);
  }, [settings]);

  const readiness = useMemo(
    () => (settings && draft ? evaluateRoutingMigrationReadiness(settings, draft) : null),
    [draft, settings],
  );

  if (loading && !settings) {
    return (
      <SectionCard title="Hierarchical v1 迁移 readiness">
        <div className="text-sm text-muted-foreground">正在读取迁移所需配置...</div>
      </SectionCard>
    );
  }

  if (!settings || !draft || !readiness) {
    return (
      <SectionCard title="Hierarchical v1 迁移 readiness">
        <div className="text-sm text-muted-foreground">暂无可检查的路由设置。</div>
      </SectionCard>
    );
  }

  return (
    <SectionCard
      title="Hierarchical v1 迁移 readiness"
      description="这里仅保存预迁移确认，不切换当前 production router。后续 cutover 会读取这个完整配置。"
      action={
        <StatusBadge tone={readiness.ready ? "healthy" : "warning"}>
          {readiness.ready ? "ready" : "needs confirmation"}
        </StatusBadge>
      }
      contentClassName="grid gap-3"
    >
      {settings.hierarchicalRoutingMigration && (
        <div className="rounded-[var(--surface-radius)] border border-success-border bg-success-surface px-3 py-2 text-xs text-success-foreground">
          已保存 hierarchical_v1 预迁移配置；旧 strategy 字段会在 cutover 后标记为 legacy ignored。
        </div>
      )}

      <div className="grid gap-2 rounded-[var(--surface-radius)] border border-border bg-muted/30 p-3 text-sm">
        <div className="flex items-center justify-between gap-3">
          <span className="text-muted-foreground">当前 legacy policy</span>
          <span className="font-medium text-foreground">{settings.defaultRoutingStrategy}</span>
        </div>
        <div className="flex items-center justify-between gap-3">
          <span className="text-muted-foreground">建议迁移</span>
          <span className="font-medium text-foreground">
            {readiness.proposedProfile
              ? routingOrderingProfileLabels[readiness.proposedProfile]
              : "需要人工选择"}
          </span>
        </div>
      </div>

      <label className="grid gap-1.5 text-sm">
        <span className="font-medium text-foreground">Ordering profile</span>
        <select
          className="h-8 rounded-[var(--surface-radius)] border border-border bg-surface px-2 text-sm"
          value={draft.orderingProfile ?? ""}
          onChange={(event) =>
            setDraft({
              ...draft,
              orderingProfile: event.target.value
                ? (event.target.value as HierarchicalRoutingOrderingProfile)
                : null,
            })
          }
        >
          <option value="">请选择</option>
          {orderingProfiles.map((profile) => (
            <option key={profile} value={profile}>
              {routingOrderingProfileLabels[profile]}
            </option>
          ))}
        </select>
        {readiness.manualPolicyChoiceRequired && (
          <span className="text-xs text-warning-foreground">
            BackupOnly / Automatic 不做静默映射，必须明确选择。
          </span>
        )}
      </label>

      <label className="grid gap-1.5 text-sm">
        <span className="font-medium text-foreground">Multiplier ceiling</span>
        <input
          className="h-8 rounded-[var(--surface-radius)] border border-border bg-surface px-2 text-sm"
          inputMode="decimal"
          value={draft.multiplierCeiling ?? ""}
          onChange={(event) => {
            const value = Number(event.target.value);
            setDraft({
              ...draft,
              multiplierCeiling:
                event.target.value.trim() === "" || !Number.isFinite(value) ? null : value,
            });
          }}
          placeholder="例如 2"
        />
      </label>

      <label className="flex items-start gap-2 text-sm">
        <input
          type="checkbox"
          className="mt-1"
          checked={draft.groupScopeConfirmed}
          onChange={(event) => setDraft({ ...draft, groupScopeConfirmed: event.target.checked })}
        />
        <span>
          确认 group scope 使用当前设置：
          <span className="ml-1 font-medium text-foreground">
            {draft.groupScope ? formatRoutingGroupScope(draft.groupScope) : "未设置"}
          </span>
        </span>
      </label>

      <label className="flex items-start gap-2 text-sm">
        <input
          type="checkbox"
          className="mt-1"
          checked={draft.backupDepletedConfirmed}
          onChange={(event) =>
            setDraft({ ...draft, backupDepletedConfirmed: event.target.checked })
          }
        />
        <span>
          确认 backup/depleted fallback：
          <span className="ml-1 font-medium text-foreground">
            {draft.allowDepletedFallback ? "允许 depleted emergency" : "不允许 depleted emergency"}
          </span>
        </span>
      </label>

      <label className="grid gap-1.5 text-sm">
        <span className="font-medium text-foreground">Affinity</span>
        <select
          className="h-8 rounded-[var(--surface-radius)] border border-border bg-surface px-2 text-sm"
          value={draft.affinityMode ?? ""}
          onChange={(event) =>
            setDraft({
              ...draft,
              affinityMode: event.target.value
                ? (event.target.value as HierarchicalRoutingAffinityMode)
                : null,
            })
          }
        >
          <option value="">请选择</option>
          {affinityModes.map((mode) => (
            <option key={mode} value={mode}>
              {routingAffinityModeLabels[mode]}
            </option>
          ))}
        </select>
      </label>

      {!readiness.ready && (
        <div className="rounded-[var(--surface-radius)] border border-warning-border bg-warning-surface px-3 py-2 text-xs text-warning-foreground">
          缺少：
          {readiness.issues.map((issue) => routingMigrationIssueLabels[issue]).join("、")}
        </div>
      )}

      <div className="flex justify-end">
        <Button disabled={!readiness.ready || saving} onClick={() => onConfirm(draft)}>
          {saving ? "保存中..." : "保存完整 hierarchical_v1 配置"}
        </Button>
      </div>
    </SectionCard>
  );
}
