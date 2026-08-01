// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { StationGroupOption } from "@/lib/types/groupFacts";
import { StationKeyRowsEditor, type StationKeyDraft } from "./StationKeyRowsEditor";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => {
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

describe("StationKeyRowsEditor", () => {
  it("uses the shared group badges in the trigger and dropdown", async () => {
    Element.prototype.scrollIntoView = vi.fn();
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);
    const row: StationKeyDraft = {
      clientId: "draft-1",
      id: "key-1",
      name: "Primary",
      apiKey: "",
      groupBindingId: "binding-1",
      groupIdHash: null,
      groupName: "plus",
      rateMultiplier: "0.05",
      enabled: true,
      note: "",
      deleteRequested: false,
    };
    const group: StationGroupOption = {
      value: "binding:binding-1",
      groupBindingId: "binding-1",
      groupIdHash: null,
      groupName: "plus",
      rateMultiplier: 0.05,
      inferredGroupCategory: "unknown",
      groupCategoryOverride: null,
      effectiveGroupCategory: "unknown",
      rateSource: "test",
      selectableForRemoteKey: true,
    };

    await act(async () => {
      root.render(
        <StationKeyRowsEditor
          rows={[row]}
          groupOptions={[group]}
          onRowsChange={vi.fn()}
        />,
      );
    });

    const trigger = document.querySelector<HTMLButtonElement>(
      'button[aria-label="选择密钥 1 分组"]',
    )!;
    expect(trigger.textContent).toContain("plus");
    expect(trigger.textContent).toContain("0.05x");
    expect(trigger.textContent).not.toContain("倍率");

    await act(async () => trigger.click());

    const listbox = document.querySelector<HTMLElement>('[role="listbox"]')!;
    expect(listbox.style.width).toBe("320px");
    expect(listbox.textContent).toContain("0.05x 倍率");

    await act(async () => root.unmount());
  });
});
