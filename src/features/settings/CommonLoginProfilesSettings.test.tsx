// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CommonLoginProfile } from "@/lib/types/settings";
import { CommonLoginProfilesSettings } from "./CommonLoginProfilesSettings";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const mocks = vi.hoisted(() => ({
  deleteProfile: vi.fn(),
  listProfiles: vi.fn(),
  toastError: vi.fn(),
  toastSuccess: vi.fn(),
  upsertProfile: vi.fn(),
}));

vi.mock("@/lib/api/settings", () => ({
  deleteCommonLoginProfile: mocks.deleteProfile,
  listCommonLoginProfiles: mocks.listProfiles,
  upsertCommonLoginProfile: mocks.upsertProfile,
}));

vi.mock("@/components/ui", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/components/ui")>();
  return {
    ...actual,
    useToast: () => ({
      error: mocks.toastError,
      success: mocks.toastSuccess,
    }),
  };
});

const existingProfile: CommonLoginProfile = {
  id: "profile-1",
  email: "shared@example.com",
  passwordPresent: true,
  passwordMasked: "sha...word",
};

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  mocks.deleteProfile.mockReset().mockResolvedValue(undefined);
  mocks.listProfiles.mockReset().mockResolvedValue([]);
  mocks.toastError.mockReset();
  mocks.toastSuccess.mockReset();
  mocks.upsertProfile.mockReset();
});

async function renderSettings() {
  await act(async () => {
    root.render(<CommonLoginProfilesSettings />);
  });
}

async function cleanup() {
  await act(async () => root.unmount());
  host.remove();
}

function setInputValue(input: HTMLInputElement, value: string) {
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
  setter.call(input, value);
  input.dispatchEvent(new Event("input", { bubbles: true }));
}

function buttonWithText(text: string) {
  return [...document.body.querySelectorAll<HTMLButtonElement>("button")]
    .find((button) => button.textContent?.includes(text))!;
}

describe("CommonLoginProfilesSettings", () => {
  it("adds a reusable email and password profile", async () => {
    mocks.upsertProfile.mockResolvedValue(existingProfile);
    await renderSettings();

    await act(async () => buttonWithText("添加").click());
    const dialog = document.body.querySelector<HTMLElement>('[role="dialog"]')!;
    const inputs = [...dialog.querySelectorAll<HTMLInputElement>("input")];
    await act(async () => {
      setInputValue(inputs[0], "shared@example.com");
      setInputValue(inputs[1], " shared-password ");
    });
    await act(async () => {
      dialog.querySelector<HTMLFormElement>("form")!
        .dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
      await new Promise((resolve) => setTimeout(resolve, 0));
    });

    expect(mocks.upsertProfile).toHaveBeenCalledWith({
      id: null,
      email: "shared@example.com",
      password: " shared-password ",
    });
    expect(mocks.toastSuccess).toHaveBeenCalledWith("常用登录信息已添加");

    await cleanup();
  });

  it("does not create a profile with a whitespace-only password", async () => {
    await renderSettings();

    await act(async () => buttonWithText("添加").click());
    const dialog = document.body.querySelector<HTMLElement>('[role="dialog"]')!;
    const inputs = [...dialog.querySelectorAll<HTMLInputElement>("input")];
    await act(async () => {
      setInputValue(inputs[0], "shared@example.com");
      setInputValue(inputs[1], "   ");
    });

    expect(buttonWithText("保存").disabled).toBe(true);
    await act(async () => {
      dialog.querySelector<HTMLFormElement>("form")!
        .dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    });
    expect(mocks.upsertProfile).not.toHaveBeenCalled();

    await cleanup();
  });

  it("preserves the password when editing and deletes the profile after confirmation", async () => {
    mocks.listProfiles.mockResolvedValue([existingProfile]);
    mocks.upsertProfile.mockResolvedValue({ ...existingProfile, email: "updated@example.com" });
    await renderSettings();

    await act(async () => host.querySelector<HTMLButtonElement>('button[aria-label^="编辑"]')!.click());
    const dialog = document.body.querySelector<HTMLElement>('[role="dialog"]')!;
    const emailInput = dialog.querySelector<HTMLInputElement>('input[type="email"]')!;
    await act(async () => setInputValue(emailInput, "updated@example.com"));
    await act(async () => {
      dialog.querySelector<HTMLFormElement>("form")!
        .dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
      await new Promise((resolve) => setTimeout(resolve, 0));
    });

    expect(mocks.upsertProfile).toHaveBeenCalledWith({
      id: "profile-1",
      email: "updated@example.com",
      password: null,
    });

    await act(async () => host.querySelector<HTMLButtonElement>('button[aria-label^="删除"]')!.click());
    await act(async () => {
      buttonWithText("删除").click();
      await new Promise((resolve) => setTimeout(resolve, 0));
    });

    expect(mocks.deleteProfile).toHaveBeenCalledWith("profile-1");
    expect(host.textContent).not.toContain("updated@example.com");
    expect(mocks.toastSuccess).toHaveBeenLastCalledWith("常用登录信息已删除");

    await cleanup();
  });
});
