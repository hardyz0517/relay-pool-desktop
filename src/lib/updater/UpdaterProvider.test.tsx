// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { ToastProvider } from "@/components/ui";
import { UpdaterProvider } from "./UpdaterProvider";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const mocks = vi.hoisted(() => ({
  currentAppVersion: vi.fn(),
  checkForAppUpdate: vi.fn(),
  closePendingUpdate: vi.fn(),
  downloadPendingUpdate: vi.fn(),
  installPendingUpdateAndRelaunch: vi.fn(),
  prepareLocalProxyForUpdate: vi.fn(),
}));

vi.mock("@/lib/api/updater", () => ({
  currentAppVersion: mocks.currentAppVersion,
  checkForAppUpdate: mocks.checkForAppUpdate,
  closePendingUpdate: mocks.closePendingUpdate,
  downloadPendingUpdate: mocks.downloadPendingUpdate,
  installPendingUpdateAndRelaunch: mocks.installPendingUpdateAndRelaunch,
}));

vi.mock("@/lib/api/proxy", () => ({
  prepareLocalProxyForUpdate: mocks.prepareLocalProxyForUpdate,
}));

vi.mock("./UpdateDialog", () => ({
  UpdateDialog: () => null,
}));

let host: HTMLDivElement;
let root: Root;
let queryClient: QueryClient;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  queryClient = new QueryClient();
  mocks.currentAppVersion.mockReset();
  mocks.checkForAppUpdate.mockReset().mockResolvedValue({ kind: "unsupported" });
  mocks.closePendingUpdate.mockReset().mockResolvedValue(undefined);
  mocks.downloadPendingUpdate.mockReset().mockResolvedValue(undefined);
  mocks.installPendingUpdateAndRelaunch.mockReset().mockResolvedValue(undefined);
  mocks.prepareLocalProxyForUpdate.mockReset().mockResolvedValue(null);
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
  queryClient.clear();
});

describe("UpdaterProvider", () => {
  it("keeps children mounted when the backend client is not installed yet", async () => {
    mocks.currentAppVersion.mockImplementation(() => {
      throw new Error("Backend client is not installed.");
    });

    await act(async () => {
      root.render(
        <QueryClientProvider client={queryClient}>
          <ToastProvider>
            <UpdaterProvider>
              <div data-testid="child">ready</div>
            </UpdaterProvider>
          </ToastProvider>
        </QueryClientProvider>,
      );
      await Promise.resolve();
    });

    expect(host.querySelector('[data-testid="child"]')).not.toBeNull();
  });
});
