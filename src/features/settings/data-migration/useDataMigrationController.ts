import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  choosePortableExportPath,
  choosePortableImportFile,
  getPortableImportRecoveryState,
  getPortableMigrationCapability,
  getPortableMigrationOperation,
  startPortableExport,
  startPortableImportInspection,
  startPortableImportPrepare,
} from "@/lib/api/dataMigration";
import { restartApp } from "@/lib/api/dataRecovery";
import type {
  PortableImportMode,
  PortableImportRecoveryState,
  PortableMigrationCapability,
  PortableMigrationOperation,
} from "@/lib/types/dataMigration";
import { REPLACE_CURRENT_CONFIRMATION } from "@/lib/types/dataMigration";
import { defaultIncludeHistory, validatePassphrase } from "./migrationViewModel";

const LAST_OPERATION_KEY = "relay-pool-portable-migration:last-operation-id";

export type MigrationControllerState = {
  capability: PortableMigrationCapability | null;
  recoveryState: PortableImportRecoveryState | null;
  operation: PortableMigrationOperation | null;
  loading: boolean;
  busy: boolean;
  message: string | null;
  exportOpen: boolean;
  importOpen: boolean;
  openExportDialog: () => void;
  closeExportDialog: () => void;
  openImportDialog: () => void;
  closeImportDialog: () => void;
  refresh: () => Promise<void>;
  startExport: (draft: ExportMigrationDraft) => Promise<void>;
  startImportInspection: (draft: ImportInspectionDraft) => Promise<void>;
  prepareImport: (draft: ImportPrepareDraft) => Promise<void>;
  restart: () => Promise<void>;
};

export type ExportMigrationDraft = {
  passphrase: string;
  passphraseConfirmation: string;
  includeHistory?: boolean;
};

export type ImportInspectionDraft = {
  passphrase: string;
};

export type ImportPrepareDraft = {
  inspectedImportId: string;
  mode: PortableImportMode;
  confirmationText: string;
};

export function useDataMigrationController(): MigrationControllerState {
  const [capability, setCapability] = useState<PortableMigrationCapability | null>(null);
  const [recoveryState, setRecoveryState] = useState<PortableImportRecoveryState | null>(null);
  const [operation, setOperation] = useState<PortableMigrationOperation | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [exportOpen, setExportOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const mounted = useRef(true);

  const refresh = useCallback(async () => {
    setLoading(true);
    setMessage(null);
    try {
      const [nextCapability, nextRecovery] = await Promise.all([
        getPortableMigrationCapability(),
        getPortableImportRecoveryState(),
      ]);
      if (!mounted.current) return;
      setCapability(nextCapability);
      setRecoveryState(nextRecovery);
      const operationId = safeSessionStorage().getItem(LAST_OPERATION_KEY);
      if (operationId) {
        try {
          setOperation(await getPortableMigrationOperation(operationId));
        } catch {
          safeSessionStorage().removeItem(LAST_OPERATION_KEY);
          setOperation(null);
        }
      }
    } catch {
      if (mounted.current) {
        setMessage("无法读取跨设备搬家状态。请稍后重试。");
      }
    } finally {
      if (mounted.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    mounted.current = true;
    void refresh();
    return () => {
      mounted.current = false;
    };
  }, [refresh]);

  const openExportDialog = useCallback(() => setExportOpen(true), []);
  const closeExportDialog = useCallback(() => setExportOpen(false), []);
  const openImportDialog = useCallback(() => setImportOpen(true), []);
  const closeImportDialog = useCallback(() => setImportOpen(false), []);

  const startExportAction = useCallback(async (draft: ExportMigrationDraft) => {
    if (!capability?.enabled) {
      setMessage("跨设备搬家当前未开放。");
      return;
    }
    const validation = validatePassphrase(
      draft.passphrase,
      draft.passphraseConfirmation,
      capability.limits.maxPassphraseUtf8Bytes,
    );
    if (!validation.ok) {
      setMessage(passphraseErrorLabel(validation.reason));
      return;
    }
    setBusy(true);
    setMessage(null);
    try {
      const token = await choosePortableExportPath();
      if (!token) {
        setMessage("已取消选择保存位置。");
        return;
      }
      const started = await startPortableExport({
        outputPathToken: token.pathToken,
        passphrase: draft.passphrase,
        passphraseConfirmation: draft.passphraseConfirmation,
        options: { includeHistory: draft.includeHistory ?? defaultIncludeHistory(capability) },
        idempotencyKey: uuidv7(),
      });
      safeSessionStorage().setItem(LAST_OPERATION_KEY, started.operationId);
      setOperation(await getPortableMigrationOperation(started.operationId));
      setExportOpen(false);
    } catch {
      setMessage("导出没有完成。请确认能力状态、保存位置和迁移密码后重试。");
    } finally {
      setBusy(false);
    }
  }, [capability]);

  const startImportInspectionAction = useCallback(async (draft: ImportInspectionDraft) => {
    if (!capability?.enabled) {
      setMessage("跨设备搬家当前未开放。");
      return;
    }
    if (!draft.passphrase) {
      setMessage("请输入迁移密码。");
      return;
    }
    setBusy(true);
    setMessage(null);
    try {
      const token = await choosePortableImportFile();
      if (!token) {
        setMessage("已取消选择搬家包。");
        return;
      }
      const started = await startPortableImportInspection({
        inputPathToken: token.pathToken,
        passphrase: draft.passphrase,
        idempotencyKey: uuidv7(),
      });
      safeSessionStorage().setItem(LAST_OPERATION_KEY, started.operationId);
      setOperation(await getPortableMigrationOperation(started.operationId));
    } catch {
      setMessage("导入检查没有完成。请确认搬家包和迁移密码后重试。");
    } finally {
      setBusy(false);
    }
  }, [capability]);

  const prepareImportAction = useCallback(async (draft: ImportPrepareDraft) => {
    if (!capability?.enabled) {
      setMessage("跨设备搬家当前未开放。");
      return;
    }
    if (draft.mode === "replaceCurrent" && draft.confirmationText !== REPLACE_CURRENT_CONFIRMATION) {
      setMessage(`替换当前数据必须精确输入“${REPLACE_CURRENT_CONFIRMATION}”。`);
      return;
    }
    if (draft.mode === "restoreIntoEmpty" && draft.confirmationText !== "") {
      setMessage("恢复到空数据库不需要替换确认文本。");
      return;
    }
    setBusy(true);
    setMessage(null);
    try {
      const started = await startPortableImportPrepare({
        inspectedImportId: draft.inspectedImportId,
        mode: draft.mode,
        confirmationText: draft.confirmationText,
        idempotencyKey: uuidv7(),
      });
      safeSessionStorage().setItem(LAST_OPERATION_KEY, started.operationId);
      setOperation(await getPortableMigrationOperation(started.operationId));
      setMessage("导入已准备，请重启应用完成激活。");
      try {
        await restartApp();
      } catch {
        setMessage("导入已准备，请手动重启应用完成激活。");
      }
    } catch {
      setMessage("导入准备没有完成。当前数据未被替换。");
    } finally {
      setBusy(false);
    }
  }, [capability]);

  const restart = useCallback(async () => {
    try {
      await restartApp();
    } catch {
      setMessage("自动重启失败，请手动重启应用。");
    }
  }, []);

  return useMemo(() => ({
    capability,
    recoveryState,
    operation,
    loading,
    busy,
    message,
    exportOpen,
    importOpen,
    openExportDialog,
    closeExportDialog,
    openImportDialog,
    closeImportDialog,
    refresh,
    startExport: startExportAction,
    startImportInspection: startImportInspectionAction,
    prepareImport: prepareImportAction,
    restart,
  }), [
    busy,
    capability,
    closeExportDialog,
    closeImportDialog,
    exportOpen,
    importOpen,
    loading,
    message,
    openExportDialog,
    openImportDialog,
    operation,
    prepareImportAction,
    recoveryState,
    refresh,
    restart,
    startExportAction,
    startImportInspectionAction,
  ]);
}

function passphraseErrorLabel(reason: "too_short" | "too_large" | "mismatch") {
  switch (reason) {
    case "too_short":
      return "迁移密码至少需要 12 个 Unicode 字符。";
    case "too_large":
      return "迁移密码超过后端允许的 UTF-8 字节上限。";
    case "mismatch":
      return "两次输入的迁移密码不一致。";
    default:
      return reason satisfies never;
  }
}

function uuidv7(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  const timestamp = BigInt(Date.now());
  bytes[0] = Number((timestamp >> 40n) & 0xffn);
  bytes[1] = Number((timestamp >> 32n) & 0xffn);
  bytes[2] = Number((timestamp >> 24n) & 0xffn);
  bytes[3] = Number((timestamp >> 16n) & 0xffn);
  bytes[4] = Number((timestamp >> 8n) & 0xffn);
  bytes[5] = Number(timestamp & 0xffn);
  bytes[6] = (bytes[6] & 0x0f) | 0x70;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = [...bytes].map((byte) => byte.toString(16).padStart(2, "0")).join("");
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

function safeSessionStorage(): Storage {
  try {
    return window.sessionStorage;
  } catch {
    return memoryStorage;
  }
}

const memory = new Map<string, string>();
const memoryStorage: Storage = {
  get length() {
    return memory.size;
  },
  clear: () => memory.clear(),
  getItem: (key) => memory.get(key) ?? null,
  key: (index) => [...memory.keys()][index] ?? null,
  removeItem: (key) => {
    memory.delete(key);
  },
  setItem: (key, value) => {
    memory.set(key, value);
  },
};
