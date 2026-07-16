import { describe, expect, it } from "vitest";

import type { DataStoreStartupView } from "@/lib/types/dataRecovery";
import { buildRecoveryViewModel } from "./recoveryViewModel";

describe("buildRecoveryViewModel", () => {
  it("disables missing, unreadable, corrupt, and schema-incompatible candidates", () => {
    const state: DataStoreStartupView = {
      decision: { kind: "conflict", candidateIds: ["healthy", "corrupt"] },
      candidates: [
        {
          id: "healthy",
          role: "active",
          path: "D:\\Relay Pool\\relay-pool-desktop.sqlite3",
          health: "healthy",
          schemaCompatible: true,
          sizeBytes: 2048,
          modifiedAt: "2026-07-17T08:00:00Z",
          counts: { stations: 3, settings: 8 },
        },
        {
          id: "missing",
          role: "default",
          path: "C:\\Users\\Someone\\AppData\\relay-pool-desktop.sqlite3",
          health: "missing",
          schemaCompatible: false,
          sizeBytes: null,
          modifiedAt: null,
          counts: {},
        },
        {
          id: "corrupt",
          role: "source",
          path: "E:\\broken\\relay-pool-desktop.sqlite3",
          health: "invalidSqlite",
          schemaCompatible: true,
          sizeBytes: 128,
          modifiedAt: null,
          counts: {},
        },
        {
          id: "wrong-schema",
          role: "backup",
          path: "E:\\old\\relay-pool-desktop.sqlite3",
          health: "healthy",
          schemaCompatible: false,
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
      ["wrong-schema", false],
    ]);
    expect(viewModel.candidates[0].summary).toContain("站点 3");
    expect(viewModel.candidates[0].metadata).toContain("2 KB");
    expect(viewModel.candidates[1].disabledReason).toBe("文件不存在");
    expect(viewModel.candidates[2].disabledReason).toBe("不是有效的 SQLite 数据库");
    expect(viewModel.candidates[3].disabledReason).toBe("数据库结构不兼容");
  });

  it("explains pending relocation as a manual recovery state", () => {
    const state: DataStoreStartupView = {
      decision: { kind: "needsRecovery", reason: "pendingRelocation" },
      candidates: [],
    };

    const viewModel = buildRecoveryViewModel(state);

    expect(viewModel.title).toContain("数据目录迁移未完成");
    expect(viewModel.description).toContain("不会自动覆盖任何现有数据库");
  });
});
