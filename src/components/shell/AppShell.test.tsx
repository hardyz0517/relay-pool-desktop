// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useQuery } from "@tanstack/react-query";
import { AppShell } from "./AppShell";

vi.mock("@tanstack/react-query", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@tanstack/react-query")>()),
  useQuery: vi.fn(),
}));

const mockedUseQuery = vi.mocked(useQuery);

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

describe("AppShell change-center unread badge", () => {
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
    mockedUseQuery.mockReset();
  });

  function renderShell(totalCount: number) {
    mockedUseQuery.mockImplementation((options) => {
      const queryName = options.queryKey[0];
      if (queryName === "alertingActivity") {
        return { data: { totalCount } } as ReturnType<typeof useQuery>;
      }
      if (queryName === "proxyStatus") {
        return { data: { running: false } } as ReturnType<typeof useQuery>;
      }
      if (queryName === "settings") {
        return { data: { developerModeEnabled: false } } as ReturnType<typeof useQuery>;
      }
      return { data: null } as ReturnType<typeof useQuery>;
    });

    act(() => {
      root.render(
        <AppShell
          activeRouteId="dashboard"
          navigationSequence={1}
          onRouteChange={vi.fn()}
        >
          <div>content</div>
        </AppShell>,
      );
    });
  }

  it("shows unread changes in the change-center badge", () => {
    renderShell(2);

    const changesButton = host.querySelector('[data-navigation-route-id="changes"]');
    expect(changesButton?.querySelector('[aria-label="2 条未读变更"]')?.textContent).toBe("2");
    expect(mockedUseQuery).toHaveBeenCalledWith(
      expect.objectContaining({
        queryKey: ["alertingActivity", { limit: 1, recordType: "change", unreadOnly: true }],
        refetchInterval: 30_000,
      }),
    );
  });

  it("hides the badge when no unread changes exist, regardless of active issues", () => {
    renderShell(0);

    const changesButton = host.querySelector('[data-navigation-route-id="changes"]');
    expect(changesButton?.querySelector('[aria-label$="条未读变更"]')).toBeNull();
  });
});
