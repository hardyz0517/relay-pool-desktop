import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const generated = vi.hoisted(() => ({
  choosePortableExportPath: vi.fn(),
  choosePortableImportFile: vi.fn(),
  getPortableExportResult: vi.fn(),
  getPortableImportInspection: vi.fn(),
  getPortableImportPrepareResult: vi.fn(),
  getPortableImportRecoveryState: vi.fn(),
  getPortableMigrationCapability: vi.fn(),
  getPortableMigrationOperation: vi.fn(),
  startPortableExport: vi.fn(),
  startPortableImportInspection: vi.fn(),
  startPortableImportPrepare: vi.fn(),
}));
const transport = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@/lib/bridge/generated", () => generated);
vi.mock("@/lib/bridge/transport", () => transport);

import { setActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import { DesktopBackend } from "@/lib/bridge/DesktopBackend";
import {
  choosePortableExportPath,
  getPortableExportResult,
  getPortableImportRecoveryState,
  getPortableMigrationCapability,
  getPortableMigrationOperation,
  startPortableExport,
} from "./dataMigration";

describe("data migration API generated transport cutover", () => {
  beforeEach(() => {
    setActiveBackendClient(new DesktopBackend());
    generated.getPortableMigrationCapability.mockReset().mockResolvedValue({
      enabled: false,
      blockedReasons: ["security_policy_not_approved"],
      supportedFormat: "relay-pool-portable-migration",
      supportedProfile: "portable-migration-v1",
      currentSchemaProfile: "relay-pool-desktop-v10",
      historySupported: true,
      limits: fixtureLimits(),
    });
    generated.choosePortableExportPath.mockReset().mockResolvedValue({ pathToken: "token", expiresInMs: 600_000 });
    generated.startPortableExport.mockReset().mockResolvedValue({
      operationId: "1",
      resourceId: "018f0000-0000-7000-8000-000000000000",
      resourceKind: "export",
    });
    generated.getPortableExportResult.mockReset().mockResolvedValue({
      exportId: "018f0000-0000-7000-8000-000000000000",
      packageSizeBytes: 42,
    });
    generated.getPortableMigrationOperation.mockReset().mockResolvedValue({
      operationId: "1",
      kind: "export_package",
      state: "terminal",
      deadlineMs: 1_000,
      progress: [{ phase: "queued" }],
      terminal: { terminal: "result_unknown" },
    });
    generated.getPortableImportRecoveryState.mockReset().mockResolvedValue({ state: "none" });
    transport.invoke.mockReset().mockRejectedValue(new Error("legacy transport invoked"));
  });

  afterEach(() => {
    setActiveBackendClient(null);
  });

  it("routes capability, chooser, operation and result reads through dedicated generated wrappers", async () => {
    await expect(getPortableMigrationCapability()).resolves.toMatchObject({ enabled: false });
    await expect(choosePortableExportPath()).resolves.toMatchObject({ pathToken: "token" });
    await expect(getPortableExportResult("resource-1")).resolves.toMatchObject({ packageSizeBytes: 42 });
    await expect(getPortableMigrationOperation("1")).resolves.toMatchObject({ state: "terminal" });
    await expect(getPortableImportRecoveryState()).resolves.toEqual({ state: "none" });

    expect(generated.getPortableMigrationCapability).toHaveBeenCalledWith();
    expect(generated.choosePortableExportPath).toHaveBeenCalledWith();
    expect(generated.getPortableExportResult).toHaveBeenCalledWith({ resourceId: "resource-1" });
    expect(generated.getPortableMigrationOperation).toHaveBeenCalledWith({ operationId: "1" });
    expect(generated.getPortableImportRecoveryState).toHaveBeenCalledWith();
    expect(transport.invoke).not.toHaveBeenCalled();
  });

  it("passes passphrases only as command payload and never to the legacy transport", async () => {
    await startPortableExport({
      outputPathToken: "token",
      passphrase: "RPD_TEST_PASSWORD_CANARY",
      passphraseConfirmation: "RPD_TEST_PASSWORD_CANARY",
      options: { includeHistory: false },
      idempotencyKey: "018f0000-0000-7000-8000-000000000001",
    });

    expect(generated.startPortableExport).toHaveBeenCalledWith(expect.objectContaining({
      passphrase: "RPD_TEST_PASSWORD_CANARY",
      passphraseConfirmation: "RPD_TEST_PASSWORD_CANARY",
    }));
    expect(transport.invoke).not.toHaveBeenCalled();
  });
});

function fixtureLimits() {
  return {
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
  };
}
