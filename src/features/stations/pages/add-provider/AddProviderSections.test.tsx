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
    const onCommonEmailSelect = vi.fn();
    const onCommonPasswordSelect = vi.fn();
    const onCopyWebsiteUrl = vi.fn();
    const onStartManualAuthorization = vi.fn();
    const onTestConnection = vi.fn();
    const host = document.createElement("div");
    const root = createRoot(host);
    const form = createDefaultProviderForm();

    await act(async () =>
      root.render(
        <ProviderConnectionSection
          commonLoginEmails={[{
            id: "email-1",
            email: "shared@example.com",
          }]}
          commonLoginPasswords={[{
            id: "password-1",
            passwordMasked: "sha...word",
          }]}
          connectionTest={{ status: "idle", message: null }}
          editing={false}
          error={null}
          form={form}
          loading={false}
          passwordProfileLoading={false}
          saving={false}
          startingAuthorization={false}
          testingConnection={false}
          onConnectionTestReset={vi.fn()}
          onCommonEmailSelect={onCommonEmailSelect}
          onCommonPasswordSelect={onCommonPasswordSelect}
          onCopyWebsiteUrl={onCopyWebsiteUrl}
          onFormChange={vi.fn()}
          onStartManualAuthorization={onStartManualAuthorization}
          onStationTypeChange={vi.fn()}
          onTestConnection={onTestConnection}
        />,
      ),
    );

    const copyButton = [...host.querySelectorAll<HTMLButtonElement>("button")].find(
      (button) => button.textContent?.includes("复制前端网址"),
    )!;
    await act(async () => copyButton.dispatchEvent(new MouseEvent("click", { bubbles: true })));

    const authorizationButton = [...host.querySelectorAll<HTMLButtonElement>("button")].find(
      (button) => button.textContent?.includes("打开窗口授权"),
    )!;
    const testButton = [...host.querySelectorAll<HTMLButtonElement>("button")].find(
      (button) => button.textContent?.includes("测试连通性"),
    )!;
    await act(async () => authorizationButton.dispatchEvent(new MouseEvent("click", { bubbles: true })));
    await act(async () => testButton.dispatchEvent(new MouseEvent("click", { bubbles: true })));

    const emailMenuButton = host.querySelector<HTMLButtonElement>('button[aria-label="选择常用邮箱"]')!;
    const usernameInput = host.querySelector<HTMLInputElement>('input[aria-label="登录用户名或邮箱"]')!;
    expect(emailMenuButton.closest("label")).toBeNull();
    expect(usernameInput.closest("label")).toBeNull();
    expect(
      usernameInput.compareDocumentPosition(emailMenuButton) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(usernameInput.className).toContain("w-full");
    expect(emailMenuButton.className).not.toContain("rounded-r-none");
    expect(usernameInput.type).toBe("text");
    expect(usernameInput.autocomplete).toBe("username");
    await act(async () => emailMenuButton.dispatchEvent(new MouseEvent("click", { bubbles: true })));
    const emailOption = [...document.body.querySelectorAll<HTMLButtonElement>('[role="option"]')]
      .find((button) => button.textContent?.includes("shared@example.com"))!;
    await act(async () => emailOption.dispatchEvent(new MouseEvent("click", { bubbles: true })));

    const passwordMenuButton = host.querySelector<HTMLButtonElement>('button[aria-label="选择常用密码"]')!;
    await act(async () => passwordMenuButton.dispatchEvent(new MouseEvent("click", { bubbles: true })));
    const passwordOption = [...document.body.querySelectorAll<HTMLButtonElement>('[role="option"]')]
      .find((button) => button.textContent?.includes("sha...word"))!;
    await act(async () => passwordOption.dispatchEvent(new MouseEvent("click", { bubbles: true })));

    expect(onCopyWebsiteUrl).toHaveBeenCalledOnce();
    expect(onStartManualAuthorization).toHaveBeenCalledOnce();
    expect(onTestConnection).toHaveBeenCalledOnce();
    expect(onCommonEmailSelect).toHaveBeenCalledWith("email-1");
    expect(onCommonPasswordSelect).toHaveBeenCalledWith("password-1");
    expect(
      authorizationButton.compareDocumentPosition(testButton) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();

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
          onDeleteRemoteKey={vi.fn()}
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

  it("delegates a supported remote-key delete without deleting locally", async () => {
    const onDeleteRemoteKey = vi.fn();
    const remoteKey = {
      id: "remote-1",
      stationId: "station-1",
      remoteKeyIdHash: "remote-hash",
      remoteKeyName: "Remote fixture",
      apiKeyMasked: "sk-fixture********test",
      apiKeyFingerprint: null,
      groupIdHash: null,
      groupName: "default",
      tierLabel: null,
      rateMultiplier: 1,
      rateSource: "newapi_tokens",
      createdAt: null,
      lastUsedAt: null,
      rawSource: "newapi_tokens",
      matchStatus: "unbound" as const,
      matchedStationKeyId: null,
      matchConfidence: 0,
      collectedAt: "1700000000000",
    };
    const host = document.createElement("div");
    const root = createRoot(host);

    await act(async () =>
      root.render(
        <ProviderKeysSection
          activeStationId="station-1"
          createRemoteDisabled={false}
          currentCreditPerCny={1}
          disabled={false}
          groupOptions={[]}
          localKeyIdsCreatedByRemote={{}}
          localKeys={[]}
          remoteCapability={{
            stationId: "station-1",
            stationType: "newapi",
            canListRemoteKeys: true,
            canCreateRemoteKey: true,
            canDeleteRemoteKeys: true,
            canReadGroups: true,
            requiresManualSession: true,
            unsupportedReason: null,
          }}
          remoteCapabilityError={null}
          remoteCapabilityUnavailableReason={null}
          remoteDiscoveryReason={null}
          remoteKeys={[remoteKey]}
          remoteListError={null}
          remoteLoading={false}
          remoteUnsupportedReason={null}
          rows={[]}
          scanRemoteDisabled={false}
          onAddLocalKey={vi.fn()}
          onBindRemoteKey={vi.fn()}
          onDeleteRemoteKey={onDeleteRemoteKey}
          onLocalKeyToggle={vi.fn()}
          onOpenCreateRemoteKey={vi.fn()}
          onRowsChange={vi.fn()}
          onScanRemoteKeys={vi.fn()}
        />,
      ),
    );

    const deleteButton = host.querySelector<HTMLButtonElement>(
      'button[aria-label="删除远端 Key Remote fixture"]',
    )!;
    await act(async () => deleteButton.dispatchEvent(new MouseEvent("click", { bubbles: true })));

    expect(onDeleteRemoteKey).toHaveBeenCalledWith(remoteKey);

    await act(async () => root.unmount());
  });
});
