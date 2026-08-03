// @vitest-environment jsdom

import { act } from "react";
import type { ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { BackendBootstrap } from "./BackendBootstrap";
import { RuntimeContractMismatchError } from "@/lib/bridge/runtimeContractError";
import type { BackendClient } from "@/lib/bridge/BackendClient";
import type { RuntimeContractInfo } from "@/lib/bridge/contract";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const contract: RuntimeContractInfo = {
  appVersion: "0.3.2",
  ipcContractVersion: 1,
  bindingHash: "test-binding",
  capabilities: ["runtime_contract"],
};

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  host = document.createElement("div");
  document.body.appendChild(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

describe("BackendBootstrap", () => {
  it("handshakes desktop before mounting data-store bootstrap", async () => {
    let resolveHandshake: (value: RuntimeContractInfo) => void = () => undefined;
    const client = testBackendClient("desktop", {
      handshake: vi.fn(() => new Promise<RuntimeContractInfo>((resolve) => {
        resolveHandshake = resolve;
      })),
    });
    const renderDataStoreBootstrap = vi.fn((renderReady: () => ReactNode) => renderReady());

    await act(async () => {
      root.render(
        <BackendBootstrap
          createClient={() => client}
          renderDataStoreBootstrap={renderDataStoreBootstrap}
          renderReady={() => <div data-testid="business-app">ready</div>}
        />,
      );
    });

    expect(client.handshake).toHaveBeenCalledTimes(1);
    expect(renderDataStoreBootstrap).not.toHaveBeenCalled();

    await act(async () => {
      resolveHandshake(contract);
      await Promise.resolve();
    });

    expect(renderDataStoreBootstrap).toHaveBeenCalledTimes(1);
    expect(host.querySelector("[data-testid='business-app']")).not.toBeNull();
  });

  it("shows incompatible runtime without falling back to demo or data-store bootstrap", async () => {
    const client = testBackendClient("desktop", {
      handshake: vi.fn(() => Promise.reject(new RuntimeContractMismatchError("hash_mismatch"))),
    });
    const renderDataStoreBootstrap = vi.fn((renderReady: () => ReactNode) => renderReady());

    await act(async () => {
      root.render(
        <BackendBootstrap
          createClient={() => client}
          renderDataStoreBootstrap={renderDataStoreBootstrap}
          renderReady={() => <div data-testid="business-app">ready</div>}
        />,
      );
      await Promise.resolve();
    });

    expect(renderDataStoreBootstrap).not.toHaveBeenCalled();
    expect(host.textContent).toContain("Incompatible desktop runtime");
    expect(host.querySelector("[data-testid='business-app']")).toBeNull();
  });

  it("uses demo mode without data-store bootstrap", async () => {
    const client = testBackendClient("demo", {
      handshake: vi.fn(() => Promise.resolve(contract)),
    });
    const renderDataStoreBootstrap = vi.fn((renderReady: () => ReactNode) => renderReady());

    await act(async () => {
      root.render(
        <BackendBootstrap
          createClient={() => client}
          renderDataStoreBootstrap={renderDataStoreBootstrap}
          renderReady={() => <div data-testid="demo-app">demo</div>}
        />,
      );
      await Promise.resolve();
    });

    expect(renderDataStoreBootstrap).not.toHaveBeenCalled();
    expect(host.querySelector("[data-testid='demo-app']")).not.toBeNull();
  });
});

function testBackendClient(
  mode: BackendClient["mode"],
  overrides: Pick<BackendClient, "handshake">,
): BackendClient {
  return {
    mode,
    settings: {} as BackendClient["settings"],
    stations: {} as BackendClient["stations"],
    stationKeys: {} as BackendClient["stationKeys"],
    changeEvents: {} as BackendClient["changeEvents"],
    collectorRuns: {} as BackendClient["collectorRuns"],
    collectors: {} as BackendClient["collectors"],
    proxy: {} as BackendClient["proxy"],
    dashboard: {} as BackendClient["dashboard"],
    runtime: {} as BackendClient["runtime"],
    localRouting: {} as BackendClient["localRouting"],
    dataRecovery: {} as BackendClient["dataRecovery"],
    dataMigration: {} as BackendClient["dataMigration"],
    economics: {} as BackendClient["economics"],
    groupFacts: {} as BackendClient["groupFacts"],
    pricing: {} as BackendClient["pricing"],
    routing: {} as BackendClient["routing"],
    channels: {} as BackendClient["channels"],
    updater: {} as BackendClient["updater"],
    ...overrides,
  };
}
