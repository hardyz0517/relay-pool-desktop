import { describe, expect, it } from "vitest";

import type { DataStoreStartupView } from "@/lib/types/dataRecovery";
import { buildRecoveryViewModel } from "./recoveryViewModel";

const recoveryCapabilities = {
  canBackup: true,
  canExportDiagnostic: true,
  canCheckForUpdates: true,
  canLocateCandidate: true,
  canActivateCandidate: true,
  canCreateDataStore: true,
} as const;

const readyUpgrade = {
  stage: "ready" as const,
  currentSchemaVersion: 17,
  targetSchemaVersion: 17,
  failureReason: null,
  failureStage: null,
};

describe("buildRecoveryViewModel", () => {
  it("requires health, writable compatibility, and activation capability", () => {
    const state: DataStoreStartupView = {
      mode: "recovery",
      databaseGeneration: "two",
      compatibility: null,
      capabilities: recoveryCapabilities,
      upgrade: readyUpgrade,
      decision: { kind: "conflict", candidateIds: ["healthy", "corrupt"] },
      candidates: [
        {
          id: "healthy",
          role: "active",
          path: "D:\\Relay Pool\\relay-pool-desktop-v2.sqlite3",
          health: "healthy",
          databaseGeneration: "two",
          compatibility: {
            decisionCode: "writable",
            schemaVersion: 7,
            appVersion: "0.4.0",
          },
          sizeBytes: 2048,
          modifiedAt: "2026-07-17T08:00:00Z",
          counts: { stations: 3, settings: 8 },
        },
        {
          id: "missing",
          role: "default",
          path: "C:\\Users\\Someone\\AppData\\relay-pool-desktop-v2.sqlite3",
          health: "missing",
          databaseGeneration: "two",
          compatibility: null,
          sizeBytes: null,
          modifiedAt: null,
          counts: {},
        },
        {
          id: "corrupt",
          role: "source",
          path: "E:\\broken\\relay-pool-desktop-v2.sqlite3",
          health: "invalidSqlite",
          databaseGeneration: "two",
          compatibility: null,
          sizeBytes: 128,
          modifiedAt: null,
          counts: {},
        },
        {
          id: "inspection",
          role: "backup",
          path: "E:\\newer\\relay-pool-desktop-v2.sqlite3",
          health: "healthy",
          databaseGeneration: "two",
          compatibility: {
            decisionCode: "writerTooOld",
            schemaVersion: 8,
            appVersion: "0.4.0",
          },
          sizeBytes: 512,
          modifiedAt: null,
          counts: { stations: 1 },
        },
      ],
    };

    const viewModel = buildRecoveryViewModel(state);

    expect(viewModel.title).toContain("发现多个");
    expect(viewModel.candidates.map((candidate) => [candidate.id, candidate.selectable])).toEqual([
      ["healthy", true],
      ["missing", false],
      ["corrupt", false],
      ["inspection", false],
    ]);
    expect(viewModel.candidates[0].summary).toContain("站点 3");
    expect(viewModel.candidates[0].metadata).toContain("2 KB");
    expect(viewModel.candidates[0].generationLabel).toBe("Gen2");
    expect(viewModel.candidates[0].schemaLabel).toBe("schema 7 · 待升级至 17");
    expect(viewModel.candidates[1].disabledReason).toBe("文件不存在");
    expect(viewModel.candidates[2].disabledReason).toBe("不是有效的 SQLite 数据库");
    expect(viewModel.candidates[3].disabledReason).toBe("当前版本不可写入");
  });

  it("never makes candidates selectable when the backend denies activation", () => {
    const state: DataStoreStartupView = {
      mode: "recovery",
      databaseGeneration: "two",
      compatibility: null,
      capabilities: { ...recoveryCapabilities, canActivateCandidate: false },
      upgrade: readyUpgrade,
      decision: { kind: "needsRecovery", reason: "interruptedUpgrade" },
      candidates: [
        {
          id: "healthy",
          role: "backup",
          path: "D:\\backup\\relay-pool-desktop-v2.sqlite3",
          health: "healthy",
          databaseGeneration: "two",
          compatibility: {
            decisionCode: "writable",
            schemaVersion: 7,
            appVersion: "0.4.0",
          },
          sizeBytes: 2048,
          modifiedAt: null,
          counts: {},
        },
      ],
    };

    const [candidate] = buildRecoveryViewModel(state).candidates;

    expect(candidate.selectable).toBe(false);
    expect(candidate.disabledReason).toBe("当前启动模式不允许切换数据库");
  });

  it("explains pending relocation as a manual recovery state", () => {
    const state: DataStoreStartupView = {
      mode: "recovery",
      databaseGeneration: "two",
      compatibility: null,
      capabilities: recoveryCapabilities,
      upgrade: readyUpgrade,
      decision: { kind: "needsRecovery", reason: "pendingRelocation" },
      candidates: [],
    };

    const viewModel = buildRecoveryViewModel(state);

    expect(viewModel.title).toContain("数据目录迁移未完成");
    expect(viewModel.description).toContain("不会自动覆盖任何现有数据库");
  });

  it("describes inspection-only mode without offering destructive confirmation", () => {
    const state: DataStoreStartupView = {
      mode: "inspectionOnly",
      databaseGeneration: "two",
      compatibility: {
        decisionCode: "writerTooOld",
        schemaVersion: 8,
        appVersion: "0.4.0",
      },
      capabilities: {
        ...recoveryCapabilities,
        canActivateCandidate: false,
        canCreateDataStore: false,
        canLocateCandidate: false,
      },
      upgrade: readyUpgrade,
      decision: {
        kind: "inspectionOnly",
        candidateId: "active",
        reason: "writerTooOld",
      },
      candidates: [],
    };

    const viewModel = buildRecoveryViewModel(state);

    expect(viewModel.eyebrow).toBe("只读检查模式");
    expect(viewModel.description).toContain("最低写入版本");
    expect(viewModel.requiresDestructiveActionConfirmation).toBe(false);
  });

  it("shows schema metadata disagreement instead of reporting an unknown schema", () => {
    const state: DataStoreStartupView = {
      mode: "recovery",
      databaseGeneration: "two",
      compatibility: null,
      capabilities: recoveryCapabilities,
      upgrade: readyUpgrade,
      decision: { kind: "needsRecovery", reason: "inconsistentSchemaMetadata" },
      candidates: [{
        id: "metadata-mismatch",
        role: "active",
        path: "D:\\Relay Pool\\relay-pool-desktop-v2.sqlite3",
        health: "healthy",
        databaseGeneration: "two",
        compatibility: {
          decisionCode: "metadataMismatch",
          schemaVersion: 55,
          appVersion: "0.4.0",
        },
        sizeBytes: 2048,
        modifiedAt: null,
        counts: {},
      }],
    };

    const [candidate] = buildRecoveryViewModel(state).candidates;

    expect(candidate.schemaLabel).toBe("schema 55 · schema metadata 不一致");
    expect(candidate.selectable).toBe(false);
    expect(candidate.disabledReason).toBe("兼容性元数据不一致");
  });

  it.each([
    ["missing", "probe"],
    ["schemaMigrationFailed", "migrate"],
    ["secretBaselineFailed", "migrate"],
    ["keyMismatch", "validate"],
    ["corruptedDatabase", "validate"],
  ] as const)("places %s failures in the typed %s step", (failureReason, expectedStage) => {
    const state: DataStoreStartupView = {
      mode: "recovery",
      databaseGeneration: "two",
      compatibility: null,
      capabilities: recoveryCapabilities,
      decision: { kind: "needsRecovery", reason: failureReason },
      candidates: [],
      upgrade: {
        stage: "blocked",
        currentSchemaVersion: 15,
        targetSchemaVersion: 17,
        failureReason,
        failureStage: expectedStage,
      },
    };

    const viewModel = buildRecoveryViewModel(state);
    expect(viewModel.upgradeSteps.find((step) => step.id === expectedStage)?.state).toBe("blocked");
    const failedIndex = viewModel.upgradeSteps.findIndex((step) => step.id === expectedStage);
    expect(viewModel.upgradeSteps.slice(0, failedIndex).every((step) => step.state === "done")).toBe(true);
    expect(viewModel.upgradeSteps.slice(failedIndex + 1).every((step) => step.state === "pending")).toBe(true);
  });
});
