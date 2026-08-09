// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ChannelStatusController } from "../useChannelStatusController";
import { ChannelStatusToolbar } from "./ChannelStatusToolbar";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(() => {
  act(() => root.unmount());
  host.remove();
});

describe("ChannelStatusToolbar", () => {
  it("keeps the batch test action enabled during background status polling", async () => {
    const controller = {
      window: "recent",
      setWindow: vi.fn(),
      filters: { search: "", enabled: "all", outcome: "all" },
      setSearch: vi.fn(),
      setEnabled: vi.fn(),
      setOutcome: vi.fn(),
      statusQuery: { isPending: false, isFetching: true },
      isRunningAction: false,
      testAll: vi.fn(),
      refresh: vi.fn(),
    } as unknown as ChannelStatusController;

    await act(async () => {
      root.render(
        <ChannelStatusToolbar
          controller={controller}
          viewMode="table"
          onViewModeChange={vi.fn()}
        />,
      );
    });

    const testButton = Array.from(host.querySelectorAll("button")).find((button) =>
      button.querySelector("svg.lucide-play"),
    ) as HTMLButtonElement | undefined;

    expect(testButton).toBeDefined();
    expect(testButton?.disabled).toBe(false);
  });
});
