// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CommonLoginOptions } from "@/lib/types/settings";
import { CommonLoginProfilesSettings } from "./CommonLoginProfilesSettings";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const mocks = vi.hoisted(() => ({
  deleteEmail: vi.fn(),
  deletePassword: vi.fn(),
  listOptions: vi.fn(),
  toastError: vi.fn(),
  toastSuccess: vi.fn(),
  upsertEmail: vi.fn(),
  upsertPassword: vi.fn(),
}));

vi.mock("@/lib/api/settings", () => ({
  deleteCommonLoginEmail: mocks.deleteEmail,
  deleteCommonLoginPassword: mocks.deletePassword,
  listCommonLoginOptions: mocks.listOptions,
  upsertCommonLoginEmail: mocks.upsertEmail,
  upsertCommonLoginPassword: mocks.upsertPassword,
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

const existingOptions: CommonLoginOptions = {
  emails: [{ id: "email-1", email: "shared@example.com" }],
  passwords: [{ id: "password-1", passwordMasked: "sha...word" }],
};

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  mocks.deleteEmail.mockReset().mockResolvedValue(undefined);
  mocks.deletePassword.mockReset().mockResolvedValue(undefined);
  mocks.listOptions.mockReset().mockResolvedValue({ emails: [], passwords: [] });
  mocks.toastError.mockReset();
  mocks.toastSuccess.mockReset();
  mocks.upsertEmail.mockReset();
  mocks.upsertPassword.mockReset();
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

function optionGroup(title: string) {
  const heading = [...host.querySelectorAll<HTMLHeadingElement>("h3")]
    .find((item) => item.textContent === title)!;
  return heading.closest("section")!;
}

function dialog() {
  return document.body.querySelector<HTMLElement>('[role="dialog"]')!;
}

async function submitDialog() {
  await act(async () => {
    dialog().querySelector<HTMLFormElement>("form")!
      .dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

describe("CommonLoginProfilesSettings", () => {
  it("adds emails independently from passwords", async () => {
    mocks.upsertEmail.mockResolvedValue(existingOptions.emails[0]);
    await renderSettings();

    expect([...host.querySelectorAll("h2")].map((heading) => heading.textContent)).toEqual([
      "常用登录信息",
    ]);
    await act(async () => optionGroup("邮箱").querySelector<HTMLButtonElement>("button")!.click());
    const input = dialog().querySelector<HTMLInputElement>('input[type="email"]')!;
    expect(dialog().querySelector('input[type="password"]')).toBeNull();
    await act(async () => setInputValue(input, " shared@example.com "));
    await submitDialog();

    expect(mocks.upsertEmail).toHaveBeenCalledWith({
      id: null,
      email: "shared@example.com",
    });
    expect(mocks.upsertPassword).not.toHaveBeenCalled();
    expect(mocks.toastSuccess).toHaveBeenCalledWith("常用邮箱已添加");

    await cleanup();
  });

  it("adds passwords independently and preserves password bytes", async () => {
    mocks.upsertPassword.mockResolvedValue(existingOptions.passwords[0]);
    await renderSettings();

    await act(async () => optionGroup("密码").querySelector<HTMLButtonElement>("button")!.click());
    const input = dialog().querySelector<HTMLInputElement>('input[type="password"]')!;
    expect(dialog().querySelector('input[type="email"]')).toBeNull();
    await act(async () => setInputValue(input, " shared-password "));
    await submitDialog();

    expect(mocks.upsertPassword).toHaveBeenCalledWith({
      id: null,
      password: " shared-password ",
    });
    expect(mocks.upsertEmail).not.toHaveBeenCalled();
    expect(mocks.toastSuccess).toHaveBeenCalledWith("常用密码已添加");

    await cleanup();
  });

  it("blocks blank passwords and deletes each option from its own list", async () => {
    mocks.listOptions.mockResolvedValue(existingOptions);
    await renderSettings();

    await act(async () => optionGroup("密码").querySelector<HTMLButtonElement>('button[aria-label^="编辑"]')!.click());
    const passwordInput = dialog().querySelector<HTMLInputElement>('input[type="password"]')!;
    await act(async () => setInputValue(passwordInput, "   "));
    expect(dialog().querySelector<HTMLButtonElement>('button[type="submit"]')!.disabled).toBe(true);

    await act(async () => dialog().querySelector<HTMLButtonElement>('button[type="button"]')!.click());
    await act(async () => optionGroup("邮箱").querySelector<HTMLButtonElement>('button[aria-label^="删除"]')!.click());
    const confirmButton = [...document.body.querySelectorAll<HTMLButtonElement>("button")]
      .find((button) => button.textContent === "删除")!;
    await act(async () => {
      confirmButton.click();
      await new Promise((resolve) => setTimeout(resolve, 0));
    });

    expect(mocks.upsertPassword).not.toHaveBeenCalled();
    expect(mocks.deleteEmail).toHaveBeenCalledWith("email-1");
    expect(mocks.deletePassword).not.toHaveBeenCalled();
    expect(optionGroup("密码").textContent).toContain("sha...word");

    await cleanup();
  });
});
