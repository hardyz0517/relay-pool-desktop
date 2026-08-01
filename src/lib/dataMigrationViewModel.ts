import type {
  PortableImportRecoveryReasonCode,
  PortableImportRecoveryState,
  PortableMigrationBlockedReason,
  PortableMigrationCapability,
  PortableMigrationOperation,
} from "@/lib/types/dataMigration";

export const EXPORT_STEP_LABELS = [
  "选择保存位置",
  "设置迁移密码",
  "确认范围",
  "生成加密数据包",
  "完成并校验",
] as const;

export const IMPORT_STEP_LABELS = [
  "选择数据包",
  "输入迁移密码",
  "读取并校验包",
  "确认数据摘要",
  "选择恢复模式",
  "冻结当前数据",
  "准备新数据库",
  "重启完成激活",
] as const;

export const MIN_PASSPHRASE_SCALARS = 12;

export type PassphraseValidation =
  | { ok: true; scalarCount: number; utf8Bytes: number }
  | { ok: false; scalarCount: number; utf8Bytes: number; reason: "too_short" | "too_large" | "mismatch" };

export function validatePassphrase(
  passphrase: string,
  confirmation: string,
  maxUtf8Bytes: number,
): PassphraseValidation {
  const scalarCount = Array.from(passphrase).length;
  const utf8Bytes = new TextEncoder().encode(passphrase).byteLength;
  if (scalarCount < MIN_PASSPHRASE_SCALARS) {
    return { ok: false, scalarCount, utf8Bytes, reason: "too_short" };
  }
  if (utf8Bytes > maxUtf8Bytes) {
    return { ok: false, scalarCount, utf8Bytes, reason: "too_large" };
  }
  if (passphrase !== confirmation) {
    return { ok: false, scalarCount, utf8Bytes, reason: "mismatch" };
  }
  return { ok: true, scalarCount, utf8Bytes };
}

export function defaultIncludeHistory(capability: PortableMigrationCapability | null): boolean {
  return capability?.historySupported ? false : false;
}

export function describeCapability(capability: PortableMigrationCapability | null) {
  if (!capability) {
    return {
      enabled: false,
      tone: "disabled" as const,
      title: "正在检查跨设备搬家能力",
      detail: "检查完成前不会启用导出或导入。",
    };
  }
  if (capability.enabled) {
    return {
      enabled: true,
      tone: "healthy" as const,
      title: "可用",
      detail: `格式 ${capability.supportedProfile}，当前 ${capability.currentSchemaProfile}。`,
    };
  }
  return {
    enabled: false,
    tone: "warning" as const,
    title: "暂未开放",
    detail: capability.blockedReasons.map(blockedReasonLabel).join("；") || "当前环境未满足迁移要求。",
  };
}

export function blockedReasonLabel(reason: PortableMigrationBlockedReason): string {
  switch (reason) {
    case "security_policy_not_approved":
      return "安全策略尚未批准跨设备迁移";
    case "unsupported_platform":
      return "当前平台暂不支持";
    case "security_baseline_incomplete":
      return "本地数据尚未完成安全基线转换";
    case "credential_store_key_missing":
      return "系统凭据中缺少当前设备密钥";
    case "credential_store_unavailable":
      return "系统凭据服务不可用";
    case "data_store_not_writable":
      return "当前数据目录不可写";
    case "maintenance_in_progress":
      return "已有数据维护操作正在进行";
    default:
      return assertNever(reason);
  }
}

export function recoveryReasonLabel(reason: PortableImportRecoveryReasonCode): string {
  switch (reason) {
    case "activation_validation_failed":
      return "激活后数据库校验失败，已回滚或等待人工处理";
    case "atomic_replace_failed":
      return "原子替换没有得到确定结果";
    case "journal_invalid":
      return "迁移激活日志损坏或无法验证";
    case "artifact_identity_mismatch":
      return "迁移工件身份与日志不一致";
    case "rollback_validation_failed":
      return "回滚校验失败，需要人工恢复";
    default:
      return assertNever(reason);
  }
}

export function describeRecoveryState(state: PortableImportRecoveryState) {
  switch (state.state) {
    case "none":
      return { blocksBusinessApp: false, title: "无待处理迁移", detail: "可以正常进入应用。" };
    case "activationPending":
      return {
        blocksBusinessApp: true,
        title: "跨设备导入已准备完成",
        detail: `导入 ${state.importId} 等待重启激活。请重启应用完成最后替换。`,
      };
    case "activated":
      return {
        blocksBusinessApp: true,
        title: "跨设备导入已激活",
        detail: `导入 ${state.importId} 已激活。请确认数据后继续。`,
      };
    case "rolledBack":
      return {
        blocksBusinessApp: true,
        title: "跨设备导入已回滚",
        detail: `导入 ${state.importId} 未激活：${recoveryReasonLabel(state.reasonCode)}。`,
      };
    case "manualRecoveryRequired":
      return {
        blocksBusinessApp: true,
        title: "需要人工恢复跨设备导入",
        detail: `${state.importId ? `导入 ${state.importId}：` : ""}${recoveryReasonLabel(state.reasonCode)}。`,
      };
    default:
      return assertNever(state);
  }
}

export function operationProgressLabel(operation: PortableMigrationOperation | null): string {
  const latest = operation?.progress[operation.progress.length - 1];
  if (!operation || !latest) return "等待开始";
  switch (latest.phase) {
    case "queued":
      return "已排队";
    case "kdf_started":
      return "正在处理迁移密码";
    case "kdf_finished":
      return "密码处理完成";
    case "reading_package":
      return `正在读取数据包 ${latest.percent}%`;
    case "writing_database":
      return `正在写入数据库 ${latest.percent}%`;
    case "publishing_package":
      return `正在发布数据包 ${latest.percent}%`;
    case "verifying_package":
      return "正在校验结果";
    default:
      return assertNever(latest as never);
  }
}

export function terminalLabel(operation: PortableMigrationOperation | null): string | null {
  const terminal = operation?.terminal;
  if (!terminal) return null;
  switch (terminal.terminal) {
    case "completed":
      return "已完成";
    case "failed":
      return `失败：${terminal.code}`;
    case "cancelled":
      return "已取消";
    case "timed_out":
      return "已超时";
    case "result_unknown":
      return "结果已过期或不可确认";
    default:
      return assertNever(terminal);
  }
}

function assertNever(value: never): never {
  throw new Error(`Unhandled portable migration variant: ${String(value)}`);
}
