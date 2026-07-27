// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";
import { providerPresets } from "../../providerPresets";
import { createDefaultProviderForm } from "./formModel";
import {
  ProviderConnectionSection,
  ProviderGroupsSection,
  ProviderKeysSection,
  ProviderOptionsSection,
  ProviderPresetSection,
} from "./AddProviderSections";

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

  it("delegates connection actions to page handlers", async () => {
    const onCopyWebsiteUrl = vi.fn();
    const host = document.createElement("div");
    const root = createRoot(host);
    const form = createDefaultProviderForm();

    await act(async () =>
      root.render(
        <ProviderConnectionSection
          connectionTest={{ status: "idle", message: null }}
          editing={false}
          error={null}
          form={form}
          loading={false}
          saving={false}
          startingAuthorization={false}
          testingConnection={false}
          onConnectionTestReset={vi.fn()}
          onCopyWebsiteUrl={onCopyWebsiteUrl}
          onFormChange={vi.fn()}
          onStartManualAuthorization={vi.fn()}
          onStationTypeChange={vi.fn()}
          onTestConnection={vi.fn()}
        />,
      ),
    );

    const copyButton = [...host.querySelectorAll<HTMLButtonElement>("button")].find(
      (button) => button.textContent?.includes("复制前端网址"),
    )!;
    await act(async () => copyButton.dispatchEvent(new MouseEvent("click", { bubbles: true })));

    expect(onCopyWebsiteUrl).toHaveBeenCalledOnce();

    await act(async () => root.unmount());
  });

  it("delegates group toolbar actions to page handlers", async () => {
    const onAddGroup = vi.fn();
    const onSyncRemoteGroups = vi.fn();
    const host = document.createElement("div");
    const root = createRoot(host);

    await act(async () =>
      root.render(
        <ProviderGroupsSection
          developerModeEnabled={false}
          disabled={false}
          remoteCapabilityUnavailableReason={null}
          remoteLoading={false}
          rows={[]}
          scanRemoteDisabled={false}
          onAddGroup={onAddGroup}
          onRowsChange={vi.fn()}
          onSyncRemoteGroups={onSyncRemoteGroups}
        />,
      ),
    );

    const buttons = [...host.querySelectorAll<HTMLButtonElement>("button")];
    await act(async () =>
      buttons.find((button) => button.textContent?.includes("同步远端分组"))!
        .dispatchEvent(new MouseEvent("click", { bubbles: true })),
    );
    await act(async () =>
      buttons.find((button) => button.textContent?.includes("添加分组"))!
        .dispatchEvent(new MouseEvent("click", { bubbles: true })),
    );

    expect(onSyncRemoteGroups).toHaveBeenCalledOnce();
    expect(onAddGroup).toHaveBeenCalledOnce();

    await act(async () => root.unmount());
  });

  it("delegates key toolbar actions to page handlers", async () => {
    const onAddLocalKey = vi.fn();
    const onOpenCreateRemoteKey = vi.fn();
    const onScanRemoteKeys = vi.fn();
    const host = document.createElement("div");
    const root = createRoot(host);

    await act(async () =>
      root.render(
        <ProviderKeysSection
          activeStationId={null}
          createRemoteDisabled={false}
          currentCreditPerCny={1}
          disabled={false}
          groupOptions={[]}
          localKeyIdsCreatedByRemote={{}}
          localKeys={[]}
          remoteCapability={null}
          remoteCapabilityError={null}
          remoteCapabilityUnavailableReason={null}
          remoteDiscoveryReason={null}
          remoteKeys={[]}
          remoteListError={null}
          remoteLoading={false}
          remoteUnsupportedReason={null}
          rows={[]}
          scanRemoteDisabled={false}
          onAddLocalKey={onAddLocalKey}
          onBindRemoteKey={vi.fn()}
          onLocalKeyToggle={vi.fn()}
          onOpenCreateRemoteKey={onOpenCreateRemoteKey}
          onRowsChange={vi.fn()}
          onScanRemoteKeys={onScanRemoteKeys}
        />,
      ),
    );

    const buttons = [...host.querySelectorAll<HTMLButtonElement>("button")];
    await act(async () =>
      buttons.find((button) => button.textContent?.includes("获取所有 Key"))!
        .dispatchEvent(new MouseEvent("click", { bubbles: true })),
    );
    await act(async () =>
      buttons.find((button) => button.textContent?.includes("新建远端 Key"))!
        .dispatchEvent(new MouseEvent("click", { bubbles: true })),
    );
    await act(async () =>
      buttons.find((button) => button.textContent?.includes("添加密钥"))!
        .dispatchEvent(new MouseEvent("click", { bubbles: true })),
    );

    expect(onScanRemoteKeys).toHaveBeenCalledOnce();
    expect(onOpenCreateRemoteKey).toHaveBeenCalledOnce();
    expect(onAddLocalKey).toHaveBeenCalledOnce();

    await act(async () => root.unmount());
  });
});
