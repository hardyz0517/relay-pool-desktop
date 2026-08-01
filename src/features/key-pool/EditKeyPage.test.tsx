// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";
import { KeyModelConfigurationEditor } from "./EditKeyPage";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => {
  document.body.innerHTML = "";
});

describe("KeyModelConfigurationEditor", () => {
  it("supports typing a default model and selecting one from the model menu", async () => {
    const onDefaultModelChange = vi.fn();
    const onModelListChange = vi.fn();
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);

    await act(async () => {
      root.render(
        <KeyModelConfigurationEditor
          defaultModel=""
          modelList={"gpt-5\ngpt-4.1"}
          modelListAction={null}
          onDefaultModelChange={onDefaultModelChange}
          onModelListChange={onModelListChange}
        />,
      );
    });

    const input = document.querySelector<HTMLInputElement>('input[aria-label="默认模型"]')!;
    const valueSetter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    valueSetter.call(input, "custom-model");
    await act(async () => input.dispatchEvent(new Event("input", { bubbles: true })));
    expect(onDefaultModelChange).toHaveBeenCalledWith("custom-model");

    const menuButton = document.querySelector<HTMLButtonElement>(
      'button[aria-label="从模型列表选择默认模型"]',
    )!;
    await act(async () => menuButton.click());

    const modelOption = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="option"]'))
      .find((option) => option.textContent?.includes("gpt-5"));
    expect(modelOption).toBeDefined();
    await act(async () => modelOption!.click());
    expect(onDefaultModelChange).toHaveBeenCalledWith("gpt-5");

    onDefaultModelChange.mockClear();
    const addModelInput = document.querySelector<HTMLInputElement>('input[placeholder="添加模型"]')!;
    valueSetter.call(addModelInput, "new-model");
    await act(async () => addModelInput.dispatchEvent(new Event("input", { bubbles: true })));
    await act(async () => addModelInput.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "Enter" })));

    expect(onModelListChange).toHaveBeenCalledWith("gpt-5\ngpt-4.1\nnew-model");
    expect(onDefaultModelChange).not.toHaveBeenCalled();

    await act(async () => root.unmount());
  });
});
