// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { ThemePreference, ThemeUpdateResult } from "@/theme/theme";
import { ThemeSettings } from "./ThemeSettings";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const mocks = vi.hoisted(() => ({
  setPreference: vi.fn<(preference: ThemePreference) => ThemeUpdateResult>(() => ({ persisted: true })),
  toastError: vi.fn(),
}));

vi.mock("@/theme/ThemeProvider", () => ({
  useTheme: () => ({
    preference: "light" as ThemePreference,
    resolvedTheme: "light",
    setPreference: mocks.setPreference,
  }),
}));

vi.mock("@/components/ui", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/components/ui")>();
  return {
    ...actual,
    useToast: () => ({
      error: mocks.toastError,
    }),
  };
});

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  mocks.setPreference.mockReset().mockReturnValue({ persisted: true });
  mocks.toastError.mockReset();
});

async function renderThemeSettings() {
  await act(async () => {
    root.render(<ThemeSettings />);
  });
}

async function unmountThemeSettings() {
  await act(async () => {
    root.unmount();
  });
  host.remove();
}

describe("ThemeSettings", () => {
  it("renders three accessible appearance choices with decorative icons", async () => {
    await renderThemeSettings();

    const radios = [...host.querySelectorAll<HTMLElement>('[role="radio"]')];
    const icons = [...host.querySelectorAll("svg")];

    expect(radios.map((radio) => radio.textContent)).toEqual(["日间", "夜间", "跟随系统"]);
    expect(icons).toHaveLength(3);
    expect(icons.every((icon) => icon.getAttribute("aria-hidden") === "true")).toBe(true);

    await unmountThemeSettings();
  });

  it("updates the theme preference without toast when the preference persists", async () => {
    await renderThemeSettings();

    await act(async () => {
      host.querySelectorAll<HTMLElement>('[role="radio"]')[1].click();
    });

    expect(mocks.setPreference).toHaveBeenCalledWith("dark");
    expect(mocks.toastError).not.toHaveBeenCalled();

    await unmountThemeSettings();
  });

  it("shows one error toast when the preference switches but cannot persist", async () => {
    mocks.setPreference.mockReturnValue({ persisted: false });
    await renderThemeSettings();

    await act(async () => {
      host.querySelectorAll<HTMLElement>('[role="radio"]')[1].click();
    });

    expect(mocks.setPreference).toHaveBeenCalledWith("dark");
    expect(mocks.toastError).toHaveBeenCalledOnce();
    expect(mocks.toastError).toHaveBeenCalledWith(
      "主题偏好无法保存",
      "主题已切换，但偏好无法保存；重启后可能恢复上次设置。",
    );

    await unmountThemeSettings();
  });
});
