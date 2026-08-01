import type { RoutingGroupFilter } from "@/lib/types/routing";
import type {
  AppSettings,
  ConfirmHierarchicalRoutingMigrationInput,
  HierarchicalRoutingAffinityMode,
  HierarchicalRoutingOrderingProfile,
  RoutingStrategy,
} from "@/lib/types/settings";

export type RoutingMigrationReadinessDraft = {
  orderingProfile: HierarchicalRoutingOrderingProfile | null;
  multiplierCeiling: number | null;
  groupScope: RoutingGroupFilter | null;
  groupScopeConfirmed: boolean;
  allowDepletedFallback: boolean | null;
  backupDepletedConfirmed: boolean;
  affinityMode: HierarchicalRoutingAffinityMode | null;
};

export type RoutingMigrationReadinessIssue =
  | "ordering_profile_unconfirmed"
  | "multiplier_ceiling_unconfirmed"
  | "group_scope_unconfirmed"
  | "backup_depleted_unconfirmed"
  | "affinity_unconfirmed";

export type RoutingMigrationReadiness = {
  ready: boolean;
  issues: RoutingMigrationReadinessIssue[];
  proposedProfile: HierarchicalRoutingOrderingProfile | null;
  manualPolicyChoiceRequired: boolean;
  input: ConfirmHierarchicalRoutingMigrationInput | null;
};

export const routingMigrationIssueLabels: Record<RoutingMigrationReadinessIssue, string> = {
  ordering_profile_unconfirmed: "尚未确认 ordering profile",
  multiplier_ceiling_unconfirmed: "尚未确认倍率硬上限",
  group_scope_unconfirmed: "尚未确认 group scope",
  backup_depleted_unconfirmed: "尚未确认 backup/depleted 行为",
  affinity_unconfirmed: "尚未确认 affinity 行为",
};

export const routingOrderingProfileLabels: Record<HierarchicalRoutingOrderingProfile, string> = {
  priority_first: "PriorityFirst",
  cost_first: "CostFirst",
};

export const routingAffinityModeLabels: Record<HierarchicalRoutingAffinityMode, string> = {
  disabled: "关闭 affinity",
  session: "Session affinity",
  previous_response: "Previous response affinity",
};

export function proposedRoutingOrderingProfile(
  legacyPolicy: RoutingStrategy,
): HierarchicalRoutingOrderingProfile | null {
  switch (legacyPolicy) {
    case "priority_fallback":
    case "stable_first":
      return "priority_first";
    case "cheap_first":
    case "cost_stable_first":
      return "cost_first";
    case "backup_only":
    case "automatic_balanced":
      return null;
  }
}

export function createRoutingMigrationDraft(settings: AppSettings): RoutingMigrationReadinessDraft {
  return {
    orderingProfile:
      settings.hierarchicalRoutingMigration?.orderingProfile ??
      proposedRoutingOrderingProfile(settings.defaultRoutingStrategy),
    multiplierCeiling:
      settings.hierarchicalRoutingMigration?.multiplierCeiling ??
      settings.maxRateMultiplier ??
      null,
    groupScope:
      settings.hierarchicalRoutingMigration?.groupScope ??
      settings.defaultRoutingGroupFilter ??
      null,
    groupScopeConfirmed: Boolean(settings.hierarchicalRoutingMigration),
    allowDepletedFallback:
      settings.hierarchicalRoutingMigration?.allowDepletedFallback ??
      settings.allowDepletedFallback,
    backupDepletedConfirmed: Boolean(settings.hierarchicalRoutingMigration),
    affinityMode: settings.hierarchicalRoutingMigration?.affinityMode ?? null,
  };
}

export function evaluateRoutingMigrationReadiness(
  settings: AppSettings,
  draft: RoutingMigrationReadinessDraft,
): RoutingMigrationReadiness {
  const issues: RoutingMigrationReadinessIssue[] = [];
  if (draft.orderingProfile == null) issues.push("ordering_profile_unconfirmed");
  if (
    draft.multiplierCeiling == null ||
    !Number.isFinite(draft.multiplierCeiling) ||
    draft.multiplierCeiling < 0
  ) {
    issues.push("multiplier_ceiling_unconfirmed");
  }
  if (draft.groupScope == null || !draft.groupScopeConfirmed) issues.push("group_scope_unconfirmed");
  if (draft.allowDepletedFallback == null || !draft.backupDepletedConfirmed) {
    issues.push("backup_depleted_unconfirmed");
  }
  if (draft.affinityMode == null) issues.push("affinity_unconfirmed");

  const ready = issues.length === 0;
  const input =
    ready &&
    draft.orderingProfile != null &&
    draft.multiplierCeiling != null &&
    draft.groupScope != null &&
    draft.allowDepletedFallback != null &&
    draft.affinityMode != null
      ? {
          orderingProfile: draft.orderingProfile,
          multiplierCeiling: draft.multiplierCeiling,
          groupScope: draft.groupScope,
          allowDepletedFallback: draft.allowDepletedFallback,
          affinityMode: draft.affinityMode,
          legacyPolicy: settings.defaultRoutingStrategy,
        }
      : null;
  return {
    ready,
    issues,
    proposedProfile: proposedRoutingOrderingProfile(settings.defaultRoutingStrategy),
    manualPolicyChoiceRequired:
      proposedRoutingOrderingProfile(settings.defaultRoutingStrategy) == null,
    input,
  };
}

export function formatRoutingGroupScope(scope: RoutingGroupFilter): string {
  if (scope === "all_groups") return "全部分组";
  if (scope === "ungrouped_only") return "仅未分组";
  if ("group_type" in scope) return `${scope.group_type} 分组`;
  if ("group_binding_id" in scope) return "指定绑定";
  if ("group_id_hash" in scope) return "指定分组";
  return "全部分组";
}
