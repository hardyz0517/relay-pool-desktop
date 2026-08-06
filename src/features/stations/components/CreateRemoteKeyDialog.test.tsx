// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { StationGroupOption } from "@/lib/types/groupFacts";
import { CreateRemoteKeyDialog } from "./CreateRemoteKeyDialog";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => {
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

describe("CreateRemoteKeyDialog", () => {
  it("reuses the shared rich group selector", async () => {
    Element.prototype.scrollIntoView = vi.fn();
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);
    const group: StationGroupOption = {
      value: "binding:binding-1",
      groupBindingId: "binding-1",
      groupIdHash: "group-hash-1",
      groupName: "GPT系列(PRO 号池)",
      rateMultiplier: 0.2,
      inferredGroupCategory: "unknown",
      groupCategoryOverride: null,
      effectiveGroupCategory: "unknown",
      rateSource: "test",
      selectableForRemoteKey: true,
    };

    await act(async () => {
      root.render(
        <CreateRemoteKeyDialog
          open
          groups={[group]}
          onClose={vi.fn()}
          onSubmit={vi.fn()}
        />,
      );
    });

    const trigger = document.querySelector<HTMLButtonElement>('button[aria-label="远端分组"]')!;
    expect(trigger.textContent).toContain("GPT系列(PRO 号池)");
    expect(trigger.textContent).toContain("0.2x");
    expect(trigger.textContent).not.toContain("倍率");

    await act(async () => trigger.click());

    const listbox = document.querySelector<HTMLElement>('[role="listbox"]')!;
    expect(listbox.style.width).toBe("420px");
    expect(listbox.className).toContain("[scrollbar-width:none]");
    expect(listbox.className).toContain("[&::-webkit-scrollbar]:hidden");
    expect(listbox.textContent).toContain("不指定分组");
    expect(listbox.textContent).toContain("按远端默认策略创建");
    const noGroupOption = listbox.querySelector('[role="option"]')!;
    expect(noGroupOption.querySelector(".shrink-0")?.textContent).toContain("按远端默认策略创建");
    expect(listbox.textContent).toContain("GPT系列(PRO 号池)");
    expect(listbox.textContent).toContain("0.2x 倍率");
    expect(listbox.querySelector("svg")).not.toBeNull();

    await act(async () => root.unmount());
  });
});
