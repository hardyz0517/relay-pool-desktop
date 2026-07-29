// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DataMigrationSection } from "./DataMigrationSection";
import type { MigrationControllerState } from "./useDataMigrationController";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const controller = vi.hoisted(() => ({
  value: null as MigrationControllerState | null,
}));

vi.mock("./useDataMigrationController", () => ({
  useDataMigrationController: () => controller.value,
}));

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  controller.value = fixtureController();
});

async function render() {
  await act(async () => {
    root.render(<DataMigrationSection />);
  });
}

describe("DataMigrationSection", () => {
  it("shows a disabled cross-device migration block without enabling actions", async () => {
    await render();

    expect(host.textContent).toContain("跨设备搬家");
    expect(host.textContent).toContain("安全策略尚未批准");
    expect([...host.querySelectorAll("button")].some((button) =>
      button.textContent?.includes("导出搬家包") && button.disabled
    )).toBe(true);
    expect(host.textContent).not.toContain("RPD_TEST_PASSWORD_CANARY");
  });
});

function fixtureController(): MigrationControllerState {
  return {
    capability: {
      enabled: false,
      blockedReasons: ["security_policy_not_approved"],
      supportedFormat: "relay-pool-portable-migration",
      supportedProfile: "portable-migration-v1",
      currentSchemaProfile: "relay-pool-desktop-v10",
      historySupported: true,
      limits: {
        maxAgeFileBytes: 1,
        maxSqliteBytes: 1,
        maxRowsPerTable: 1,
        maxTotalUserTableRows: 1,
        maxJsonDepth: 1,
        maxRegularFieldBytes: 1,
        maxLargeRedactedJsonFieldBytes: 1,
        maxPassphraseUtf8Bytes: 1024,
        exportDeadlineMs: 1,
        inspectionDeadlineMs: 1,
        prepareDeadlineMs: 1,
      },
    },
    recoveryState: { state: "none" },
    operation: null,
    loading: false,
    busy: false,
    message: null,
    exportOpen: false,
    importOpen: false,
    openExportDialog: vi.fn(),
    closeExportDialog: vi.fn(),
    openImportDialog: vi.fn(),
    closeImportDialog: vi.fn(),
    refresh: vi.fn(),
    startExport: vi.fn(),
    startImportInspection: vi.fn(),
    prepareImport: vi.fn(),
    restart: vi.fn(),
  };
}
