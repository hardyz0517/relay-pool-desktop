import { describe, expect, it } from "vitest";
import {
  EXPORT_STEP_LABELS,
  IMPORT_STEP_LABELS,
  blockedReasonLabel,
  defaultIncludeHistory,
  describeCapability,
  describeRecoveryState,
  operationProgressLabel,
  validatePassphrase,
} from "./migrationViewModel";
import type { PortableMigrationCapability } from "@/lib/types/dataMigration";

describe("migrationViewModel", () => {
  it("keeps export and import workflows explicit and compact", () => {
    expect(EXPORT_STEP_LABELS).toHaveLength(5);
    expect(IMPORT_STEP_LABELS).toHaveLength(8);
  });

  it("defaults history export to off even when the backend supports it", () => {
    expect(defaultIncludeHistory({ ...fixtureCapability(), historySupported: true })).toBe(false);
    expect(defaultIncludeHistory({ ...fixtureCapability(), historySupported: false })).toBe(false);
  });

  it("validates passphrase length using Unicode scalar count and UTF-8 bytes", () => {
    const elevenScalarsWithSurrogatePairs = "🔐".repeat(11);
    expect(elevenScalarsWithSurrogatePairs.length).toBe(22);
    expect(validatePassphrase(elevenScalarsWithSurrogatePairs, elevenScalarsWithSurrogatePairs, 1024)).toMatchObject({
      ok: false,
      scalarCount: 11,
      reason: "too_short",
    });

    const twelveScalars = "🔐".repeat(12);
    expect(validatePassphrase(twelveScalars, twelveScalars, 1024)).toMatchObject({
      ok: true,
      scalarCount: 12,
      utf8Bytes: 48,
    });

    expect(validatePassphrase(twelveScalars, `${twelveScalars}!`, 1024)).toMatchObject({
      ok: false,
      reason: "mismatch",
    });
  });

  it("enforces backend UTF-8 byte limits without relying on UTF-16 length", () => {
    const password = "界".repeat(400);
    expect(password.length).toBe(400);
    expect(validatePassphrase(password, password, 1024)).toMatchObject({
      ok: false,
      utf8Bytes: 1200,
      reason: "too_large",
    });
  });

  it("maps every current blocked reason to Chinese copy", () => {
    expect(blockedReasonLabel("security_policy_not_approved")).toContain("安全策略");
    expect(blockedReasonLabel("maintenance_in_progress")).toContain("维护");
  });

  it("describes disabled capability and recovery states without including passphrase-shaped canaries", () => {
    const capability = {
      ...fixtureCapability(),
      blockedReasons: ["security_policy_not_approved"],
    } satisfies PortableMigrationCapability;
    expect(describeCapability(capability).detail).not.toContain("RPD_TEST_PASSWORD_CANARY");
    expect(describeRecoveryState({ state: "activationPending", importId: "018f" })).toMatchObject({
      blocksBusinessApp: true,
    });
  });

  it("maps indeterminate KDF and typed progress", () => {
    expect(operationProgressLabel({
      operationId: "1",
      kind: "inspect_package",
      state: "running",
      deadlineMs: 1,
      progress: [{ phase: "kdf_started" }],
      terminal: null,
    })).toBe("正在处理迁移密码");
  });
});

function fixtureCapability(): PortableMigrationCapability {
  return {
    enabled: false,
    blockedReasons: [],
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
  };
}
