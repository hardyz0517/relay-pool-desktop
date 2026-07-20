import type {
  CompatibilityDecisionCode,
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
  generationLabel: string;
  schemaLabel: string;
  summary: string;
  metadata: string;
  selectable: boolean;
  disabledReason: string | null;
};

export type RecoveryViewModel = {
  eyebrow: string;
  title: string;
  description: string;
  requiresDestructiveActionConfirmation: boolean;
  candidates: RecoveryCandidateViewModel[];
};

export function buildRecoveryViewModel(state: DataStoreStartupView): RecoveryViewModel {
  return {
    ...describeStartup(state),
    candidates: state.candidates.map((candidate) =>
      toCandidateViewModel(candidate, state.capabilities.canActivateCandidate),
    ),
  };
}

function describeStartup(state: DataStoreStartupView) {
  if (state.mode === "inspectionOnly") {
    return {
      eyebrow: "只读检查模式",
      title: "当前版本不能安全写入此数据库",
      description: describeCompatibility(state.compatibility.decisionCode),
      requiresDestructiveActionConfirmation: false,
    };
  }

  return {
    eyebrow: "本地数据恢复",
    ...describeDecision(state.decision),
    requiresDestructiveActionConfirmation: true,
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
      title: "准备初始化 generation 2 本地数据",
      description: `即将在默认目录创建新的 generation 2 数据库：${decision.defaultDataDir}`,
    };
  }
  if (decision.kind === "needsRecovery") {
    return describeRecoveryReason(decision.reason);
  }
  if (decision.kind === "inspectionOnly") {
    return {
      title: "当前版本只能读取数据库",
      description: describeCompatibility(decision.reason),
    };
  }
  return {
    title: "本地数据已就绪",
    description: "Relay Pool 可以继续启动。",
  };
}

function describeCompatibility(reason: CompatibilityDecisionCode) {
  const descriptions: Record<CompatibilityDecisionCode, string> = {
    writable: "数据库与当前版本兼容，可以安全读写。",
    inspectionOnly: "数据库可以读取，但当前版本没有写入资格。业务服务、代理、采集和监控均未启动。",
    generationMismatch: "数据库 generation 与当前版本不匹配。应用已停止业务启动，避免写入错误的数据文件。",
    readerTooOld: "当前应用版本低于数据库要求的最低读取版本。请升级应用后重试。",
    writerTooOld: "当前应用版本低于数据库要求的最低写入版本。你仍可导出诊断、查看备份或检查更新。",
    metadataMismatch: "数据库兼容性元数据与 migration 记录不一致。应用已关闭业务写入，请先保留现场并导出诊断。",
  };
  return descriptions[reason];
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
      description: "检测到未完成的数据目录迁移。Relay Pool 不会自动覆盖任何现有数据库，需要你确认后续恢复动作。",
    },
    unsupportedLegacySchema: {
      title: "旧数据库版本无法识别",
      description: "此 generation 1 数据库不在已发布版本的升级矩阵内。源文件保持只读且不会被修改，请导出诊断。",
    },
    incompatibleSchema: {
      title: "数据库结构与当前版本不兼容",
      description: "兼容性检查未通过。业务服务不会启动，也不会尝试忽略未知字段继续写入。",
    },
    upgradeRecoveryRequired: {
      title: "数据库升级需要恢复",
      description: "检测到未完成的 generation 升级。应用已依据升级日志停止启动，请保留数据文件并执行允许的恢复动作。",
    },
    relocationUpgradeConflict: {
      title: "检测到两个未完成的数据操作",
      description: "数据目录迁移与 generation 升级状态同时存在。应用不会自动选择或合并状态，需要先导出诊断并人工处理。",
    },
    generationReopenFailed: {
      title: "generation 2 数据库重新打开失败",
      description: "升级结果未通过最终 reopen/health 检查。旧 generation 仍受保护，业务服务没有启动。",
    },
  };
  return descriptions[reason];
}

function toCandidateViewModel(
  candidate: DataStoreCandidate,
  activationAllowed: boolean,
): RecoveryCandidateViewModel {
  const compatibilityWritable = candidate.compatibility?.decisionCode === "writable";
  const selectable = activationAllowed && candidate.health === "healthy" && compatibilityWritable;
  return {
    id: candidate.id,
    path: candidate.path,
    roleLabel: roleLabels[candidate.role],
    healthLabel: healthLabels[candidate.health],
    generationLabel: candidate.databaseGeneration
      ? `Generation ${candidate.databaseGeneration === "two" ? "2" : "1"}`
      : "Generation 未知",
    schemaLabel: schemaLabel(candidate),
    summary: formatCounts(candidate.counts),
    metadata: formatMetadata(candidate),
    selectable,
    disabledReason: selectable ? null : disabledReason(candidate, activationAllowed),
  };
}

function schemaLabel(candidate: DataStoreCandidate) {
  const compatibility = candidate.compatibility;
  if (!compatibility) return "兼容性未确认";
  const schema = compatibility.schemaVersion === null
    ? "schema 未知"
    : `schema ${compatibility.schemaVersion}`;
  return `${schema} · ${compatibilityLabels[compatibility.decisionCode]}`;
}

function disabledReason(candidate: DataStoreCandidate, activationAllowed: boolean) {
  if (!activationAllowed) return "当前启动模式不允许切换数据库";
  if (candidate.health !== "healthy") return healthLabels[candidate.health];
  if (!candidate.compatibility) return "数据库兼容性尚未确认";
  if (candidate.compatibility.decisionCode !== "writable") {
    return compatibilityLabels[candidate.compatibility.decisionCode];
  }
  return "不可选择";
}

const roleLabels: Record<DataStoreCandidate["role"], string> = {
  active: "当前记录",
  default: "默认目录",
  source: "升级来源",
  pending: "待切换目标",
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

const compatibilityLabels: Record<CompatibilityDecisionCode, string> = {
  writable: "可读写",
  inspectionOnly: "仅可检查",
  generationMismatch: "generation 不匹配",
  readerTooOld: "当前版本不可读取",
  writerTooOld: "当前版本不可写入",
  metadataMismatch: "兼容性元数据不一致",
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
