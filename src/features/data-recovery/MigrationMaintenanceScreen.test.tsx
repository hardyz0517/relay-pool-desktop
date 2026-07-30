// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";
import { MigrationMaintenanceScreen } from "./MigrationMaintenanceScreen";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

describe("MigrationMaintenanceScreen", () => {
  it("shows restart-only activation pending guidance", async () => {
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);
    await act(async () => {
      root.render(
        <MigrationMaintenanceScreen
          state={{ state: "activationPending", importId: "018f" }}
          onRestart={vi.fn()}
          onRetry={vi.fn()}
        />,
      );
    });

    expect(host.textContent).toContain("跨设备导入已准备完成");
    expect(host.textContent).toContain("重启完成激活");
    expect(host.textContent).not.toContain("RPD_TEST_PASSWORD_CANARY");

    await act(async () => root.unmount());
    host.remove();
  });
});
