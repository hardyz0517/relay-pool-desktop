// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { DataStoreStartupView } from "@/lib/types/dataRecovery";
import { DataStoreBootstrap } from "./DataStoreBootstrap";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const mocks = vi.hoisted(() => ({
  getDataStoreStartupState: vi.fn<() => Promise<DataStoreStartupView>>(),
}));

vi.mock("@/lib/api/dataRecovery", () => ({
  getDataStoreStartupState: mocks.getDataStoreStartupState,
}));

vi.mock("@/features/updater/UpdaterProvider", () => ({
  useUpdater: () => ({
    state: { phase: "idle" },
    checkNow: vi.fn(),
  }),
}));

let host: HTMLDivElement;
let root: Root;

const readyState: DataStoreStartupView = {
  mode: "writable",
  databaseGeneration: "two",
  compatibility: {
    decisionCode: "writable",
    schemaVersion: 7,
    appVersion: "0.4.0",
  },
  capabilities: {
    canBackup: true,
    canExportDiagnostic: true,
    canCheckForUpdates: true,
    canLocateCandidate: false,
    canActivateCandidate: false,
    canCreateDataStore: false,
  },
  decision: { kind: "ready", candidateId: "active" },
  candidates: [],
};

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  mocks.getDataStoreStartupState.mockReset();
});

async function renderBootstrap() {
  await act(async () => {
    root.render(<DataStoreBootstrap renderReady={() => <div data-testid="business-app">App mounted</div>} />);
  });
}

async function unmountBootstrap() {
  await act(async () => {
    root.unmount();
  });
  host.remove();
}

describe("DataStoreBootstrap", () => {
  it("does not render the business app before the startup decision is ready", async () => {
    let resolveStartup!: (state: DataStoreStartupView) => void;
    mocks.getDataStoreStartupState.mockReturnValue(new Promise((resolve) => {
      resolveStartup = resolve;
    }));

    await renderBootstrap();

    expect(host.textContent).toContain("正在检查本地数据");
    expect(host.querySelector('[data-testid="business-app"]')).toBeNull();

    await act(async () => {
      resolveStartup(readyState);
    });

    expect(host.querySelector('[data-testid="business-app"]')).not.toBeNull();
    await unmountBootstrap();
  });

  it("renders recovery UI instead of the business app when startup needs recovery", async () => {
    mocks.getDataStoreStartupState.mockResolvedValue({
      mode: "recovery",
      databaseGeneration: "two",
      compatibility: null,
      capabilities: {
        canBackup: true,
        canExportDiagnostic: true,
        canCheckForUpdates: true,
        canLocateCandidate: true,
        canActivateCandidate: false,
        canCreateDataStore: false,
      },
      decision: { kind: "needsRecovery", reason: "missing" },
      candidates: [
        {
          id: "active",
          role: "active",
          path: "D:\\missing\\relay-pool-desktop.sqlite3",
          health: "missing",
          databaseGeneration: "one",
          compatibility: null,
          sizeBytes: null,
          modifiedAt: null,
          counts: {},
        },
      ],
    });

    await renderBootstrap();
    await act(async () => undefined);

    expect(host.textContent).toContain("需要确认本地数据位置");
    expect(host.querySelector('[data-testid="business-app"]')).toBeNull();
    await unmountBootstrap();
  });

  it("does not mount the business app in inspection-only mode", async () => {
    mocks.getDataStoreStartupState.mockResolvedValue({
      mode: "inspectionOnly",
      databaseGeneration: "two",
      compatibility: {
        decisionCode: "writerTooOld",
        schemaVersion: 8,
        appVersion: "0.4.0",
      },
      capabilities: {
        canBackup: true,
        canExportDiagnostic: true,
        canCheckForUpdates: true,
        canLocateCandidate: false,
        canActivateCandidate: false,
        canCreateDataStore: false,
      },
      decision: {
        kind: "inspectionOnly",
        candidateId: "active-v2",
        reason: "writerTooOld",
      },
      candidates: [],
    });

    await renderBootstrap();
    await act(async () => undefined);

    expect(host.textContent).toContain("只读检查模式");
    expect(host.textContent).toContain("当前版本不能安全写入此数据库");
    expect(host.querySelector('[data-testid="business-app"]')).toBeNull();
    await unmountBootstrap();
  });

  it("renders ACL failures as fatal startup errors", async () => {
    mocks.getDataStoreStartupState.mockRejectedValue(new Error("Command get_data_store_startup_state not allowed by ACL"));

    await renderBootstrap();
    await act(async () => undefined);

    expect(host.textContent).toContain("启动检查失败");
    expect(host.textContent).toContain("not allowed by ACL");
    expect(host.querySelector('[data-testid="business-app"]')).toBeNull();
    await unmountBootstrap();
  });
});
