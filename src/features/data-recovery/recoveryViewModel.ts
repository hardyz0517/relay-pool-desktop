import type {
  DataStoreCandidate,
  DataStoreStartupDecision,
  DataStoreStartupView,
  RecoveryReason,
} from "@/lib/types/dataRecovery";

export type RecoveryCandidateViewModel = {
  id: string;
  path: string;
  roleLabel: string;
  healthLabel: string;
  schemaLabel: string;
  summary: string;
  metadata: string;
  selectable: boolean;
  disabledReason: string | null;
};

export type RecoveryViewModel = {
  title: string;
  description: string;
  candidates: RecoveryCandidateViewModel[];
};

export function buildRecoveryViewModel(state: DataStoreStartupView): RecoveryViewModel {
  return {
    ...describeDecision(state.decision),
    candidates: state.candidates.map(toCandidateViewModel),
  };
}

function describeDecision(decision: DataStoreStartupDecision) {
  if (decision.kind === "conflict") {
    return {
      title: "发现多个可能的数据文件",
      description: "Relay Pool 检测到多个包含本地数据的数据库。请选择你确认要继续使用的那个；不会自动合并或覆盖任何文件。",
    };
  }
  if (decision.kind === "firstRun") {
    return {
      title: "准备初始化本地数据",
      description: `即将在默认目录创建新的本地数据库：${decision.defaultDataDir}`,
    };
  }
  if (decision.kind === "needsRecovery") {
    return describeRecoveryReason(decision.reason);
  }
  return {
    title: "本地数据已就绪",
    description: "Relay Pool 可以继续启动。",
  };
}

function describeRecoveryReason(reason: RecoveryReason) {
  const descriptions: Record<RecoveryReason, { title: string; description: string }> = {
    missing: {
      title: "需要确认本地数据位置",
      description: "上次记录的数据文件不存在。请从下方健康的候选数据库中选择一个，避免误打开空数据库。",
    },
    unreadable: {
      title: "数据文件暂时不可读取",
      description: "Relay Pool 无法读取上次记录的数据文件。请检查磁盘、权限或选择一个健康候选。",
    },
    invalidSqlite: {
      title: "数据文件格式异常",
      description: "上次记录的文件不是有效的 SQLite 数据库。为保护数据，业务页面不会继续启动。",
    },
    integrityFailed: {
      title: "数据完整性检查失败",
      description: "SQLite quick_check 没有通过。请先选择其它健康备份或保留现场用于诊断。",
    },
    openOrMigrationFailed: {
      title: "数据库打开或迁移失败",
      description: "应用无法安全打开当前数据库，因此已停在恢复模式，避免创建空数据或继续写入。",
    },
    pendingRelocation: {
      title: "数据目录迁移未完成",
      description: "检测到旧版迁移状态。Relay Pool 不会自动覆盖任何现有数据库，需要你确认要使用的数据文件。",
    },
  };
  return descriptions[reason];
}

function toCandidateViewModel(candidate: DataStoreCandidate): RecoveryCandidateViewModel {
  const selectable = candidate.health === "healthy" && candidate.schemaCompatible;
  return {
    id: candidate.id,
    path: candidate.path,
    roleLabel: roleLabels[candidate.role],
    healthLabel: healthLabels[candidate.health],
    schemaLabel: candidate.schemaCompatible ? "结构兼容" : "结构不兼容",
    summary: formatCounts(candidate.counts),
    metadata: formatMetadata(candidate),
    selectable,
    disabledReason: selectable ? null : disabledReason(candidate),
  };
}

function disabledReason(candidate: DataStoreCandidate) {
  if (candidate.health !== "healthy") return healthLabels[candidate.health];
  if (!candidate.schemaCompatible) return "数据库结构不兼容";
  return "不可选择";
}

const roleLabels: Record<DataStoreCandidate["role"], string> = {
  active: "当前记录",
  default: "默认目录",
  source: "迁移来源",
  pending: "迁移目标",
  backup: "备份",
  located: "手动定位",
};

const healthLabels: Record<DataStoreCandidate["health"], string> = {
  healthy: "健康",
  missing: "文件不存在",
  unreadable: "无法读取",
  invalidSqlite: "不是有效的 SQLite 数据库",
  integrityFailed: "完整性检查失败",
};

const countLabels: Record<string, string> = {
  stations: "站点",
  station_keys: "密钥",
  channel_monitors: "监控",
  settings: "设置",
};

function formatCounts(counts: Record<string, number>) {
  const parts = Object.entries(counts)
    .filter(([, value]) => Number.isFinite(value))
    .map(([key, value]) => `${countLabels[key] ?? key} ${value}`);
  return parts.length > 0 ? parts.join(" · ") : "没有可展示的计数";
}

function formatMetadata(candidate: DataStoreCandidate) {
  const parts = [formatBytes(candidate.sizeBytes)];
  if (candidate.modifiedAt) {
    parts.push(`修改于 ${formatDate(candidate.modifiedAt)}`);
  }
  return parts.join(" · ");
}

function formatBytes(value: number | null) {
  if (value === null) return "大小未知";
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${Math.round(value / 1024)} KB`;
  return `${(value / 1024 / 1024).toFixed(1)} MB`;
}

function formatDate(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return date.toLocaleString();
}
