// @vitest-environment jsdom
import { StrictMode, act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { NativeThemeSyncResult } from "./nativeTheme";
import type { ThemeSnapshot } from "./theme";
import { ThemeProvider, useTheme } from "./ThemeProvider";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean })
  .IS_REACT_ACT_ENVIRONMENT = true;

const mocks = vi.hoisted(() => ({
  applyResolvedTheme: vi.fn(),
  systemPrefersDark: vi.fn(() => false),
  subscribeToSystemTheme: vi.fn(),
  writeThemePreference: vi.fn(() => true),
  syncNativeTheme: vi.fn(),
}));

vi.mock("./themeDom", () => ({
  applyResolvedTheme: mocks.applyResolvedTheme,
  systemPrefersDark: mocks.systemPrefersDark,
  subscribeToSystemTheme: mocks.subscribeToSystemTheme,
}));

vi.mock("./themeStorage", () => ({
  writeThemePreference: mocks.writeThemePreference,
}));

vi.mock("./nativeTheme", () => ({
  syncNativeTheme: mocks.syncNativeTheme,
}));

const initialSnapshot: ThemeSnapshot = {
  preference: "system",
  resolvedTheme: "light",
};

let latestTheme: ReturnType<typeof useTheme> | undefined;
let container: HTMLDivElement;
let root: Root;

function Probe() {
  const theme = useTheme();
  latestTheme = theme;
  return <span>{`${theme.preference}:${theme.resolvedTheme}`}</span>;
}

function renderProvider(strict = true) {
  const provider = (
    <ThemeProvider initialSnapshot={initialSnapshot}>
      <Probe />
    </ThemeProvider>
  );

  act(() => {
    root.render(strict ? <StrictMode>{provider}</StrictMode> : provider);
  });
}

function currentTheme() {
  if (!latestTheme) throw new Error("Theme probe has not rendered");
  return latestTheme;
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((fulfill) => {
    resolve = fulfill;
  });
  return { promise, resolve };
}

beforeEach(() => {
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  latestTheme = undefined;

  mocks.applyResolvedTheme.mockReset();
  mocks.systemPrefersDark.mockReset().mockReturnValue(false);
  mocks.subscribeToSystemTheme.mockReset().mockReturnValue(vi.fn());
  mocks.writeThemePreference.mockReset().mockReturnValue(true);
  mocks.syncNativeTheme.mockReset().mockResolvedValue({
    generation: 1,
    applied: false,
    current: true,
  });
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("ThemeProvider", () => {
  it("shares the initial snapshot and still updates when persistence fails", () => {
    mocks.writeThemePreference.mockReturnValue(false);
    renderProvider();

    expect(container.textContent).toBe("system:light");

    let result;
    act(() => {
      result = currentTheme().setPreference("dark");
    });

    expect(result).toEqual({ persisted: false });
    expect(container.textContent).toBe("dark:dark");
    expect(mocks.applyResolvedTheme).toHaveBeenLastCalledWith("dark");
  });

  it("updates from the system listener and disposes it after selecting a manual theme", () => {
    const dispose = vi.fn();
    let listener: ((prefersDark: boolean) => void) | undefined;
    mocks.subscribeToSystemTheme.mockImplementation((nextListener) => {
      listener = nextListener;
      return dispose;
    });
    renderProvider();
    dispose.mockClear();

    act(() => listener?.(true));
    expect(container.textContent).toBe("system:dark");

    act(() => {
      currentTheme().setPreference("light");
    });

    expect(container.textContent).toBe("light:light");
    expect(dispose).toHaveBeenCalledOnce();
  });

  it("balances every StrictMode system listener setup with cleanup", () => {
    const dispose = vi.fn();
    mocks.subscribeToSystemTheme.mockReturnValue(dispose);
    renderProvider();

    act(() => root.unmount());

    expect(dispose).toHaveBeenCalledTimes(mocks.subscribeToSystemTheme.mock.calls.length);
  });

  it("resamples the system preference after current native synchronization completes", async () => {
    const nativeResult = deferred<NativeThemeSyncResult>();
    mocks.syncNativeTheme.mockReturnValue(nativeResult.promise);
    mocks.systemPrefersDark.mockReturnValueOnce(false).mockReturnValueOnce(true);
    renderProvider(false);

    await act(async () => {
      nativeResult.resolve({ generation: 1, applied: true, current: true });
      await nativeResult.promise;
    });

    expect(container.textContent).toBe("system:dark");
    expect(mocks.systemPrefersDark).toHaveBeenCalledTimes(2);
  });
});
