// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";
import { providerPresets } from "../../providerPresets";
import { createDefaultProviderForm } from "./formModel";
import { ProviderOptionsSection, ProviderPresetSection } from "./AddProviderSections";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

describe("AddProviderSections", () => {
  it("reports selected preset ids from the preset section", async () => {
    const onApplyPreset = vi.fn();
    const host = document.createElement("div");
    const root = createRoot(host);

    await act(async () =>
      root.render(
        <ProviderPresetSection presetId={providerPresets[0].id} onApplyPreset={onApplyPreset} />,
      ),
    );

    const customPreset = providerPresets.find((preset) => preset.id === "custom")!;
    const customButton = [...host.querySelectorAll<HTMLButtonElement>("button")].find(
      (button) => button.textContent?.includes(customPreset.name),
    )!;
    await act(async () => customButton.dispatchEvent(new MouseEvent("click", { bubbles: true })));

    expect(onApplyPreset).toHaveBeenCalledWith("custom");

    await act(async () => root.unmount());
  });

  it("reports option form changes without owning page state", async () => {
    const onFormChange = vi.fn();
    const host = document.createElement("div");
    const root = createRoot(host);
    const form = createDefaultProviderForm();

    await act(async () =>
      root.render(<ProviderOptionsSection form={form} onFormChange={onFormChange} />),
    );

    const thresholdInput = host.querySelector<HTMLInputElement>('input[placeholder="使用全局设置"]')!;
    const valueSetter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    valueSetter.call(thresholdInput, "20");
    await act(async () => thresholdInput.dispatchEvent(new Event("input", { bubbles: true })));

    expect(onFormChange).toHaveBeenCalledWith({
      ...form,
      lowBalanceThresholdCny: "20",
    });

    await act(async () => root.unmount());
  });
});
