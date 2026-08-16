// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { StationGroupRowsEditor, type StationGroupDraft } from "./StationGroupRowsEditor";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => {
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

describe("StationGroupRowsEditor", () => {
  it("selects the detected category directly and preserves automatic behavior when it is chosen", async () => {
    Element.prototype.scrollIntoView = vi.fn();
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);
    const onRowsChange = vi.fn();
    const row: StationGroupDraft = {
      clientId: "group-draft-1",
      groupBindingId: null,
      groupKeyHash: "",
      groupIdHash: null,
      groupName: "Claude group",
      rateMultiplier: "1",
      inferredGroupCategory: "claude",
      groupCategoryOverride: null,
      source: "manual",
      deleteRequested: false,
    };

    await act(async () => {
      root.render(<StationGroupRowsEditor rows={[row]} onRowsChange={onRowsChange} />);
    });

    const trigger = document.querySelector<HTMLButtonElement>('button[aria-label="选择分组类型"]')!;
    expect(trigger.textContent).toContain("Claude");

    await act(async () => trigger.click());

    const listbox = document.querySelector<HTMLElement>('[role="listbox"]')!;
    expect(listbox.textContent).not.toContain("跟随识别结果");
    expect(listbox.textContent).not.toContain("当前识别");
    expect(listbox.textContent).not.toContain("手动指定");
    expect(listbox.querySelectorAll('[role="option"]')).toHaveLength(6);
    expect(listbox.querySelector<HTMLElement>('[role="option"][aria-selected="true"]')?.textContent).toContain(
      "Claude",
    );

    const gptOption = [...listbox.querySelectorAll<HTMLButtonElement>('[role="option"]')].find(
      (option) => option.textContent?.includes("GPT"),
    )!;
    await act(async () => gptOption.click());

    expect(onRowsChange).toHaveBeenLastCalledWith([
      { ...row, groupCategoryOverride: "gpt" },
    ]);

    await act(async () => {
      root.render(
        <StationGroupRowsEditor
          rows={[{ ...row, groupCategoryOverride: "gpt" }]}
          onRowsChange={onRowsChange}
        />,
      );
    });
    await act(async () => trigger.click());

    const detectedOption = [...document.querySelectorAll<HTMLButtonElement>('[role="option"]')].find(
      (option) => option.textContent?.includes("Claude"),
    )!;
    await act(async () => detectedOption.click());

    expect(onRowsChange).toHaveBeenLastCalledWith([
      { ...row, groupCategoryOverride: null },
    ]);

    await act(async () => root.unmount());
  });

  it("keeps a detected developer-only category selected without exposing it as a manual choice", async () => {
    Element.prototype.scrollIntoView = vi.fn();
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);
    const row: StationGroupDraft = {
      clientId: "group-draft-2",
      groupBindingId: null,
      groupKeyHash: "",
      groupIdHash: null,
      groupName: "Embeddings",
      rateMultiplier: "1",
      inferredGroupCategory: "embedding",
      groupCategoryOverride: null,
      source: "manual",
      deleteRequested: false,
    };

    await act(async () => {
      root.render(<StationGroupRowsEditor rows={[row]} onRowsChange={vi.fn()} />);
    });

    const trigger = document.querySelector<HTMLButtonElement>('button[aria-label="选择分组类型"]')!;
    expect(trigger.textContent).toContain("Embedding");

    await act(async () => trigger.click());

    const listbox = document.querySelector<HTMLElement>('[role="listbox"]')!;
    expect(listbox.querySelector<HTMLElement>('[role="option"][aria-selected="true"]')?.textContent).toContain(
      "Embedding",
    );
    expect(listbox.textContent).not.toContain("Rerank");

    await act(async () => root.unmount());
  });
});
