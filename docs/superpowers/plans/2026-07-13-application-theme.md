# Application Theme Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 Relay Pool Desktop 增加可靠的日间、夜间、跟随系统三态主题，保持设备级偏好，完整迁移所有页面和浮层，并同步 Windows/Tauri 主窗口主题。

**Architecture:** 使用 React 挂载前 bootstrap、单一 `ThemeProvider`、设备级 `localStorage`、条件式 `matchMedia` 订阅和模块级原生主题队列形成单向主题状态流。视觉层通过 CSS 变量和 Tailwind 语义令牌消费主题，业务页面不读取主题存储、不监听系统主题、不调用 Tauri 主题 API。

**Tech Stack:** React 18、TypeScript 5、Vite 6、Tailwind CSS 3、Tauri 2、Vitest、jsdom、pnpm。

---

## Execution Preconditions

- 设计合同：`docs/superpowers/specs/2026-07-13-application-theme-design.md`
- 参考 CCSwitch 提交：`c6197ae32450cd70e2bf03b35e3f5f53ac12044c`，MIT License。
- 当前工作区可能含有用户的 Rust 和其他文档改动。执行前使用 `superpowers:using-git-worktrees` 创建隔离 worktree；不得覆盖、暂存或提交任务范围外的文件。
- 本计划只修改主窗口主题。`capture-*` 远程授权窗口保持第三方页面原样。
- 每个任务只 stage “Files” 中列出的明确路径，禁止 `git add .` 和 `git add -A`。

## File Structure

### New theme runtime

- `src/theme/theme.ts`: 主题类型、存储键和纯转换函数。
- `src/theme/themeStorage.ts`: 设备级偏好的容错读写。
- `src/theme/themeDom.ts`: 根元素主题应用和系统媒体查询订阅。
- `src/theme/themeBootstrap.ts`: React 挂载前初始化并返回首个快照。
- `src/theme/nativeTheme.ts`: 模块级、末值优先的 Tauri 窗口主题队列。
- `src/theme/ThemeProvider.tsx`: React 唯一主题状态源和系统监听生命周期。
- `src/theme/*.test.ts(x)`: 纯函数、DOM、原生队列和 Provider 回归测试。

### New UI and guard files

- `src/features/settings/ThemeSettings.tsx`: 设置页三态主题控件和持久化失败提示。
- `src/features/settings/ThemeSettings.test.tsx`: 控件语义、图标和失败反馈测试。
- `src/features/stations/groupVisualStyles.ts`: 平台身份到语义 class 的唯一视觉适配表。
- `scripts/theme-audit.mjs`: 禁止原始 Tailwind 调色板和任意颜色表达式的静态审计。

### Existing integration points

- `src/main.tsx`: bootstrap 和 ThemeProvider 根接入。
- `src/styles.css`: 浅色/深色变量及主题相关阴影。
- `tailwind.config.ts`: CSS 变量到语义 Tailwind class 的映射。
- `src-tauri/capabilities/default.json`: 主窗口 `core:window:allow-set-theme` 权限。
- `src/components/ui/**`: 共享表面、控件、状态和 Portal 迁移。
- `src/components/shell/**`, `src/app/ShellPageErrorBoundary.tsx`: Shell 视觉迁移。
- `src/features/**`: 全部业务页面、瞬态页面和动态状态色迁移。
- `package.json`, `pnpm-lock.yaml`: 测试、审计和构建命令。

## Semantic Class Contract

所有迁移任务使用同一合同，不在页面内创造另一套命名：

```text
page canvas                   bg-background text-foreground
card/sidebar/dialog/menu      bg-surface text-foreground border-border shadow-surface
weak header/hover             bg-surface-subtle / hover:bg-hover
input/read-only/code field    bg-surface-inset text-foreground border-input
secondary text                text-muted-foreground
selected neutral surface      bg-selected text-selected-foreground
focus                         ring-ring/30 border-primary/45
primary action                bg-primary-solid text-primary-foreground
primary link/border           text-primary / border-primary
strong destructive action     bg-danger-solid text-on-solid
weak status                   bg-*-surface text-*-foreground border-*-border
modal overlay                 bg-scrim/45
switch thumb                  bg-control-thumb
solid foreground              text-on-solid
```

Raw `white/black/slate/gray/zinc/neutral/stone` classes, raw status/brand color families, arbitrary hex/rgb/hsl colors, and component-local color gradients are forbidden after migration.

Default migration table:

```text
bg-white                                  -> bg-surface
bg-slate-50                               -> bg-surface-subtle
bg-slate-50 used by input/code/read-only  -> bg-surface-inset
bg-slate-100 used by hover                -> bg-hover
bg-slate-100/200 used by selection        -> bg-selected
text-slate-950/900/800/700                -> text-foreground
text-slate-600/500/400                    -> text-muted-foreground
text-slate-300 disabled decoration        -> text-muted-foreground/60
border/divide-slate-100/200/300           -> border/divide-border or border-input
text/border/bg emerald/green              -> success semantic triple
text/border/bg amber/orange                -> warning semantic triple unless it is Anthropic identity
text/border/bg rose/red                    -> danger semantic triple
text/border/bg blue/cyan                   -> info or primary according to interaction role
text/border/bg violet/purple/indigo        -> platform-image only for image identity; otherwise info
```

The component role wins over the mechanical default: page canvas stays `background`, editable fields use `surface-inset/input`, selected rows use `selected`, and Portal content uses `popover` or `surface`.

---

### Task 1: Add the test harness and pure theme model

**Files:**
- Create: `src/theme/theme.ts`
- Create: `src/theme/theme.test.ts`
- Modify: `package.json`
- Modify: `pnpm-lock.yaml`

- [ ] **Step 1: Install compatible test dependencies**

Run:

```powershell
pnpm add -D vitest@^3.2.4 jsdom@^26.1.0 @types/node@^24.0.0
```

Expected: exit 0; `package.json` and `pnpm-lock.yaml` include Vitest, jsdom, and Node 24 types.

- [ ] **Step 2: Add the deterministic test command**

Add this script without changing the existing dev/Tauri scripts:

```json
"test": "vitest run"
```

- [ ] **Step 3: Write the failing pure model test**

Create `src/theme/theme.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  THEME_STORAGE_KEY,
  nativeThemeFor,
  parseThemePreference,
  resolveTheme,
} from "./theme";

describe("theme model", () => {
  it.each(["light", "dark", "system"] as const)("accepts %s", (value) => {
    expect(parseThemePreference(value)).toBe(value);
  });

  it.each([null, undefined, "", "auto", "LIGHT", 1])(
    "falls back invalid value %s to system",
    (value) => expect(parseThemePreference(value)).toBe("system"),
  );

  it("resolves system and ignores the system value for manual preferences", () => {
    expect(resolveTheme("system", false)).toBe("light");
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("light", true)).toBe("light");
    expect(resolveTheme("dark", false)).toBe("dark");
  });

  it("maps system to the Tauri null theme", () => {
    expect(nativeThemeFor("light")).toBe("light");
    expect(nativeThemeFor("dark")).toBe("dark");
    expect(nativeThemeFor("system")).toBeNull();
    expect(THEME_STORAGE_KEY).toBe("relay-pool.theme-preference.v1");
  });
});
```

- [ ] **Step 4: Run the test to verify RED**

Run:

```powershell
pnpm test -- src/theme/theme.test.ts
```

Expected: FAIL because `src/theme/theme.ts` does not exist.

- [ ] **Step 5: Implement the pure theme model**

Create `src/theme/theme.ts`:

```ts
export type ThemePreference = "light" | "dark" | "system";
export type ResolvedTheme = "light" | "dark";

export type ThemeSnapshot = {
  preference: ThemePreference;
  resolvedTheme: ResolvedTheme;
};

export type ThemeUpdateResult = {
  persisted: boolean;
};

export const THEME_STORAGE_KEY = "relay-pool.theme-preference.v1";

export function parseThemePreference(value: unknown): ThemePreference {
  return value === "light" || value === "dark" || value === "system" ? value : "system";
}

export function resolveTheme(
  preference: ThemePreference,
  systemPrefersDark: boolean,
): ResolvedTheme {
  if (preference === "system") {
    return systemPrefersDark ? "dark" : "light";
  }
  return preference;
}

export function nativeThemeFor(preference: ThemePreference): ResolvedTheme | null {
  return preference === "system" ? null : preference;
}
```

- [ ] **Step 6: Run focused and compiler verification**

Run:

```powershell
pnpm test -- src/theme/theme.test.ts
pnpm exec tsc --noEmit
```

Expected: both commands exit 0; every assertion in `theme.test.ts` passes.

- [ ] **Step 7: Commit the pure model**

```powershell
git add -- package.json pnpm-lock.yaml src/theme/theme.ts src/theme/theme.test.ts
git commit -m "test: define application theme model"
```

---

### Task 2: Implement storage, DOM application, and bootstrap

**Files:**
- Create: `src/theme/themeStorage.ts`
- Create: `src/theme/themeStorage.test.ts`
- Create: `src/theme/themeDom.ts`
- Create: `src/theme/themeDom.test.ts`
- Create: `src/theme/themeBootstrap.ts`
- Create: `src/theme/themeBootstrap.test.ts`

- [ ] **Step 1: Write failing storage tests**

Create `src/theme/themeStorage.test.ts` with an injected storage double:

```ts
import { describe, expect, it, vi } from "vitest";
import { readThemePreference, writeThemePreference, type ThemeStorage } from "./themeStorage";

function storage(value: string | null): ThemeStorage {
  return { getItem: vi.fn(() => value), setItem: vi.fn() };
}

describe("theme storage", () => {
  it("reads and validates a stored preference", () => {
    expect(readThemePreference(storage("dark"))).toBe("dark");
    expect(readThemePreference(storage("legacy"))).toBe("system");
  });

  it("falls back when storage access throws", () => {
    expect(readThemePreference({ getItem: () => { throw new Error("blocked"); }, setItem: vi.fn() })).toBe("system");
  });

  it("reports persistence success and failure", () => {
    expect(writeThemePreference("light", storage(null))).toBe(true);
    expect(writeThemePreference("dark", { getItem: vi.fn(), setItem: () => { throw new Error("full"); } })).toBe(false);
    expect(writeThemePreference("system", null)).toBe(false);
  });
});
```

- [ ] **Step 2: Write failing DOM and bootstrap tests**

Create `src/theme/themeDom.test.ts` and `src/theme/themeBootstrap.test.ts`:

```ts
// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { applyResolvedTheme, subscribeToSystemTheme, systemPrefersDark } from "./themeDom";

describe("theme DOM", () => {
  it("keeps exactly one effective class and color scheme", () => {
    document.documentElement.className = "light unrelated";
    applyResolvedTheme("dark");
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    expect(document.documentElement.classList.contains("light")).toBe(false);
    expect(document.documentElement.classList.contains("unrelated")).toBe(true);
    expect(document.documentElement.style.colorScheme).toBe("dark");
  });

  it("reads and subscribes to the media query", () => {
    const addEventListener = vi.fn();
    const removeEventListener = vi.fn();
    const media = { matches: true, addEventListener, removeEventListener } as unknown as MediaQueryList;
    expect(systemPrefersDark(() => media)).toBe(true);
    const dispose = subscribeToSystemTheme(vi.fn(), () => media);
    expect(addEventListener).toHaveBeenCalledWith("change", expect.any(Function));
    dispose();
    expect(removeEventListener).toHaveBeenCalledWith("change", expect.any(Function));
  });
});
```

```ts
// @vitest-environment jsdom
import { describe, expect, it } from "vitest";
import { initializeTheme } from "./themeBootstrap";

describe("theme bootstrap", () => {
  it("returns and applies one shared initial snapshot", () => {
    const snapshot = initializeTheme(
      { getItem: () => "system", setItem: () => undefined },
      () => ({ matches: true } as MediaQueryList),
    );
    expect(snapshot).toEqual({ preference: "system", resolvedTheme: "dark" });
    expect(document.documentElement.classList.contains("dark")).toBe(true);
  });
});
```

- [ ] **Step 3: Run the tests to verify RED**

```powershell
pnpm test -- src/theme/themeStorage.test.ts src/theme/themeDom.test.ts src/theme/themeBootstrap.test.ts
```

Expected: FAIL because the three implementation modules do not exist.

- [ ] **Step 4: Implement safe storage**

Create `src/theme/themeStorage.ts`:

```ts
import { parseThemePreference, THEME_STORAGE_KEY, type ThemePreference } from "./theme";

export type ThemeStorage = Pick<Storage, "getItem" | "setItem">;

function browserStorage(): ThemeStorage | null {
  try {
    return typeof window === "undefined" ? null : window.localStorage;
  } catch {
    return null;
  }
}

export function readThemePreference(storage: ThemeStorage | null = browserStorage()): ThemePreference {
  try {
    return parseThemePreference(storage?.getItem(THEME_STORAGE_KEY));
  } catch {
    return "system";
  }
}

export function writeThemePreference(
  preference: ThemePreference,
  storage: ThemeStorage | null = browserStorage(),
): boolean {
  try {
    if (!storage) return false;
    storage.setItem(THEME_STORAGE_KEY, preference);
    return true;
  } catch {
    return false;
  }
}
```

- [ ] **Step 5: Implement DOM helpers and bootstrap**

Create `src/theme/themeDom.ts`:

```ts
import type { ResolvedTheme } from "./theme";

type MatchMedia = (query: string) => MediaQueryList;

function browserMatchMedia(): MatchMedia | null {
  return typeof window !== "undefined" && typeof window.matchMedia === "function"
    ? window.matchMedia.bind(window)
    : null;
}

export function systemPrefersDark(matchMedia: MatchMedia | null = browserMatchMedia()): boolean {
  return matchMedia?.("(prefers-color-scheme: dark)").matches ?? false;
}

export function applyResolvedTheme(
  theme: ResolvedTheme,
  root: HTMLElement = document.documentElement,
): void {
  root.classList.remove("light", "dark");
  root.classList.add(theme);
  root.style.colorScheme = theme;
}

export function subscribeToSystemTheme(
  listener: (prefersDark: boolean) => void,
  matchMedia: MatchMedia | null = browserMatchMedia(),
): () => void {
  if (!matchMedia) return () => undefined;
  const media = matchMedia("(prefers-color-scheme: dark)");
  const handleChange = (event: MediaQueryListEvent) => listener(event.matches);
  media.addEventListener("change", handleChange);
  return () => media.removeEventListener("change", handleChange);
}
```

Create `src/theme/themeBootstrap.ts`:

```ts
import { resolveTheme, type ThemeSnapshot } from "./theme";
import { applyResolvedTheme, systemPrefersDark } from "./themeDom";
import { readThemePreference, type ThemeStorage } from "./themeStorage";

export function initializeTheme(
  storage?: ThemeStorage | null,
  matchMedia?: (query: string) => MediaQueryList,
): ThemeSnapshot {
  const preference = readThemePreference(storage);
  const resolvedTheme = resolveTheme(preference, systemPrefersDark(matchMedia));
  applyResolvedTheme(resolvedTheme);
  return { preference, resolvedTheme };
}
```

- [ ] **Step 6: Run focused verification and commit**

```powershell
pnpm test -- src/theme/themeStorage.test.ts src/theme/themeDom.test.ts src/theme/themeBootstrap.test.ts
pnpm exec tsc --noEmit
git add -- src/theme/themeStorage.ts src/theme/themeStorage.test.ts src/theme/themeDom.ts src/theme/themeDom.test.ts src/theme/themeBootstrap.ts src/theme/themeBootstrap.test.ts
git commit -m "feat: bootstrap device theme preference"
```

Expected: tests and compiler exit 0 before the commit succeeds.

---

### Task 3: Implement the module-level native theme queue

**Files:**
- Create: `src/theme/nativeTheme.ts`
- Create: `src/theme/nativeTheme.test.ts`

- [ ] **Step 1: Write failing queue tests**

Create `src/theme/nativeTheme.test.ts`:

```ts
import { describe, expect, it, vi } from "vitest";
import { createNativeThemeSync, NativeThemeQueue } from "./nativeTheme";

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => { resolve = done; });
  return { promise, resolve };
}

describe("NativeThemeQueue", () => {
  it("skips queued stale values and applies the last value last", async () => {
    const first = deferred();
    const setter = vi.fn().mockReturnValueOnce(first.promise).mockResolvedValue(undefined);
    const queue = new NativeThemeQueue(setter);
    const light = queue.request("light");
    await Promise.resolve();
    await Promise.resolve();
    expect(setter).toHaveBeenCalledWith("light");
    const dark = queue.request("dark");
    const system = queue.request("system");
    first.resolve();
    expect((await light).current).toBe(false);
    expect((await dark).applied).toBe(false);
    expect(await system).toMatchObject({ applied: true, current: true });
    expect(setter).toHaveBeenLastCalledWith(null);
  });

  it("continues after a native failure", async () => {
    const setter = vi.fn().mockRejectedValueOnce(new Error("denied")).mockResolvedValue(undefined);
    const queue = new NativeThemeQueue(setter);
    expect((await queue.request("dark")).applied).toBe(false);
    expect((await queue.request("light")).applied).toBe(true);
  });

  it("reports repeated current failures once without sensitive context", async () => {
    const log = vi.fn();
    const sync = createNativeThemeSync(vi.fn().mockRejectedValue(new Error("denied")), log);
    await sync("dark");
    await sync("light");
    expect(log).toHaveBeenCalledTimes(1);
    expect(log).toHaveBeenCalledWith("Native window theme synchronization is unavailable.");
  });
});
```

- [ ] **Step 2: Run the test to verify RED**

```powershell
pnpm test -- src/theme/nativeTheme.test.ts
```

Expected: FAIL because `NativeThemeQueue` does not exist.

- [ ] **Step 3: Implement the queue and main-window adapter**

Create `src/theme/nativeTheme.ts`:

```ts
import { getCurrentWindow } from "@tauri-apps/api/window";
import { nativeThemeFor, type ThemePreference } from "./theme";

type NativeTheme = "light" | "dark" | null;
type SetNativeTheme = (theme: NativeTheme) => Promise<void>;

export type NativeThemeSyncResult = {
  generation: number;
  applied: boolean;
  current: boolean;
};

export class NativeThemeQueue {
  private generation = 0;
  private tail: Promise<void> = Promise.resolve();

  constructor(private readonly setTheme: SetNativeTheme) {}

  request(preference: ThemePreference): Promise<NativeThemeSyncResult> {
    const generation = ++this.generation;
    const run = this.tail.catch(() => undefined).then(async () => {
      if (generation !== this.generation) {
        return { generation, applied: false, current: false };
      }
      try {
        await this.setTheme(nativeThemeFor(preference));
        return { generation, applied: true, current: generation === this.generation };
      } catch {
        return { generation, applied: false, current: generation === this.generation };
      }
    });
    this.tail = run.then(() => undefined);
    return run;
  }
}

export function createNativeThemeSync(
  setTheme: SetNativeTheme,
  log: (message: string) => void = () => undefined,
) {
  const queue = new NativeThemeQueue(setTheme);
  let failureReported = false;
  return async (preference: ThemePreference): Promise<NativeThemeSyncResult> => {
    const result = await queue.request(preference);
    if (!result.applied && result.current && !failureReported) {
      failureReported = true;
      log("Native window theme synchronization is unavailable.");
    }
    return result;
  };
}

export const syncNativeTheme = createNativeThemeSync(
  (theme) => getCurrentWindow().setTheme(theme),
  import.meta.env.DEV ? console.debug : () => undefined,
);
```

- [ ] **Step 4: Run queue verification and commit**

```powershell
pnpm test -- src/theme/nativeTheme.test.ts
pnpm exec tsc --noEmit
git add -- src/theme/nativeTheme.ts src/theme/nativeTheme.test.ts
git commit -m "feat: serialize native window theme updates"
```

Expected: tests and compiler exit 0; no unhandled rejection output.

---

### Task 4: Add ThemeProvider, root bootstrap, and Tauri permission

**Files:**
- Create: `src/theme/ThemeProvider.tsx`
- Create: `src/theme/ThemeProvider.test.tsx`
- Modify: `src/main.tsx`
- Modify: `src-tauri/capabilities/default.json`

- [ ] **Step 1: Write failing Provider lifecycle tests**

Create `src/theme/ThemeProvider.test.tsx` with jsdom, a controlled `matchMedia`, mocked storage functions, and mocked `syncNativeTheme`. Cover these exact assertions:

```ts
// @vitest-environment jsdom
import { StrictMode, act } from "react";
import { createRoot } from "react-dom/client";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  apply: vi.fn(),
  prefersDark: vi.fn(() => false),
  subscribe: vi.fn<(listener: (dark: boolean) => void) => () => void>(),
  persist: vi.fn(() => true),
  syncNative: vi.fn(async () => ({ generation: 1, applied: true, current: true })),
}));

vi.mock("./themeDom", () => ({
  applyResolvedTheme: mocks.apply,
  systemPrefersDark: mocks.prefersDark,
  subscribeToSystemTheme: mocks.subscribe,
}));
vi.mock("./themeStorage", () => ({ writeThemePreference: mocks.persist }));
vi.mock("./nativeTheme", () => ({ syncNativeTheme: mocks.syncNative }));

import { ThemeProvider, useTheme } from "./ThemeProvider";

let currentTheme: ReturnType<typeof useTheme> | null = null;

function Probe() {
  currentTheme = useTheme();
  return <span>{currentTheme.preference}:{currentTheme.resolvedTheme}</span>;
}

async function renderProvider(strict = false) {
  const host = document.createElement("div");
  const root = createRoot(host);
  const provider = (
    <ThemeProvider initialSnapshot={{ preference: "system", resolvedTheme: "light" }}>
      <Probe />
    </ThemeProvider>
  );
  await act(async () => root.render(strict ? <StrictMode>{provider}</StrictMode> : provider));
  return { host, root };
}

describe("ThemeProvider", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    currentTheme = null;
    mocks.prefersDark.mockReturnValue(false);
    mocks.persist.mockReturnValue(true);
    mocks.subscribe.mockImplementation(() => () => undefined);
    mocks.syncNative.mockResolvedValue({ generation: 1, applied: true, current: true });
  });

  it("shares the bootstrap snapshot and returns the persistence result", async () => {
    mocks.persist.mockReturnValue(false);
    const { host, root } = await renderProvider();
    expect(host.textContent).toBe("system:light");
    let result;
    await act(async () => { result = currentTheme!.setPreference("dark"); });
    expect(result).toEqual({ persisted: false });
    expect(host.textContent).toBe("dark:dark");
    expect(mocks.apply).toHaveBeenLastCalledWith("dark");
    await act(async () => root.unmount());
  });

  it("updates from system media and cleans the listener after manual selection", async () => {
    const dispose = vi.fn();
    let listener: ((dark: boolean) => void) | null = null;
    mocks.subscribe.mockImplementation((next) => { listener = next; return dispose; });
    const { host, root } = await renderProvider();
    await act(async () => listener!(true));
    expect(host.textContent).toBe("system:dark");
    await act(async () => { currentTheme!.setPreference("light"); });
    expect(dispose).toHaveBeenCalledTimes(1);
    await act(async () => root.unmount());
  });

  it("balances listener setup and cleanup under StrictMode", async () => {
    const dispose = vi.fn();
    mocks.subscribe.mockImplementation(() => dispose);
    const { root } = await renderProvider(true);
    await act(async () => root.unmount());
    expect(dispose).toHaveBeenCalledTimes(mocks.subscribe.mock.calls.length);
  });

  it("resamples system media after the latest native system sync", async () => {
    mocks.prefersDark.mockReturnValueOnce(false).mockReturnValue(true);
    const { host, root } = await renderProvider();
    expect(host.textContent).toBe("system:dark");
    expect(mocks.prefersDark).toHaveBeenCalledTimes(2);
    await act(async () => root.unmount());
  });
});
```

- [ ] **Step 2: Run the test to verify RED**

```powershell
pnpm test -- src/theme/ThemeProvider.test.tsx
```

Expected: FAIL because `ThemeProvider.tsx` does not exist.

- [ ] **Step 3: Implement the Provider contract**

Create `src/theme/ThemeProvider.tsx` with this public interface and effect order:

```tsx
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { applyResolvedTheme, subscribeToSystemTheme, systemPrefersDark } from "./themeDom";
import { syncNativeTheme } from "./nativeTheme";
import { resolveTheme, type ThemePreference, type ThemeSnapshot, type ThemeUpdateResult } from "./theme";
import { writeThemePreference } from "./themeStorage";

type ThemeContextValue = ThemeSnapshot & {
  setPreference: (preference: ThemePreference) => ThemeUpdateResult;
};

const ThemeContext = createContext<ThemeContextValue | null>(null);

export function ThemeProvider({
  children,
  initialSnapshot,
}: {
  children: ReactNode;
  initialSnapshot: ThemeSnapshot;
}) {
  const [snapshot, setSnapshot] = useState(initialSnapshot);
  const preferenceRef = useRef(snapshot.preference);

  const refreshSystemTheme = useCallback(() => {
    if (preferenceRef.current !== "system") return;
    setSnapshot({ preference: "system", resolvedTheme: resolveTheme("system", systemPrefersDark()) });
  }, []);

  const setPreference = useCallback((preference: ThemePreference): ThemeUpdateResult => {
    const persisted = writeThemePreference(preference);
    preferenceRef.current = preference;
    setSnapshot({
      preference,
      resolvedTheme: resolveTheme(preference, systemPrefersDark()),
    });
    return { persisted };
  }, []);

  useLayoutEffect(() => applyResolvedTheme(snapshot.resolvedTheme), [snapshot.resolvedTheme]);

  useEffect(() => {
    preferenceRef.current = snapshot.preference;
    if (snapshot.preference !== "system") return;
    refreshSystemTheme();
    return subscribeToSystemTheme((prefersDark) => {
      if (preferenceRef.current !== "system") return;
      setSnapshot({ preference: "system", resolvedTheme: resolveTheme("system", prefersDark) });
    });
  }, [refreshSystemTheme, snapshot.preference]);

  useEffect(() => {
    let cancelled = false;
    const preference = snapshot.preference;
    void syncNativeTheme(preference).then((result) => {
      if (!cancelled && result.applied && result.current && preference === "system") {
        refreshSystemTheme();
      }
    });
    return () => { cancelled = true; };
  }, [refreshSystemTheme, snapshot.preference]);

  const value = useMemo(() => ({ ...snapshot, setPreference }), [setPreference, snapshot]);
  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme(): ThemeContextValue {
  const value = useContext(ThemeContext);
  if (!value) throw new Error("useTheme must be used within ThemeProvider");
  return value;
}
```

- [ ] **Step 4: Integrate bootstrap before React content**

In `src/main.tsx`, initialize once after CSS import and wrap all application providers:

```tsx
import { ThemeProvider } from "@/theme/ThemeProvider";
import { initializeTheme } from "@/theme/themeBootstrap";

const initialTheme = initializeTheme();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ThemeProvider initialSnapshot={initialTheme}>
      <QueryClientProvider client={queryClient}>
        <ToastProvider>
          <QueryErrorNotifier />
          <UpdaterProvider>
            <App />
          </UpdaterProvider>
        </ToastProvider>
      </QueryClientProvider>
    </ThemeProvider>
  </React.StrictMode>,
);
```

- [ ] **Step 5: Grant only the main-window permission**

Add one entry to `src-tauri/capabilities/default.json`:

```json
"core:window:allow-set-theme"
```

Keep `capture.json` and `permissions/main-window.toml` unchanged.

- [ ] **Step 6: Verify Provider, browser fallback, and capability**

```powershell
pnpm test -- src/theme
pnpm build
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: all commands exit 0; browser-mode tests catch and contain the unavailable Tauri invocation.

- [ ] **Step 7: Commit the runtime integration**

```powershell
git add -- src/theme/ThemeProvider.tsx src/theme/ThemeProvider.test.tsx src/main.tsx src-tauri/capabilities/default.json
git commit -m "feat: connect application theme runtime"
```

---

### Task 5: Add semantic tokens and the color audit

**Files:**
- Create: `scripts/theme-audit.mjs`
- Create: `src/theme/themeContrast.test.ts`
- Modify: `src/styles.css`
- Modify: `tailwind.config.ts`
- Modify: `package.json`

- [ ] **Step 1: Add the audit script before migration**

Create `scripts/theme-audit.mjs`:

```js
import { readdirSync, readFileSync, statSync } from "node:fs";
import { extname, relative, resolve } from "node:path";

const rawPalette = /\b(?:bg|text|border|ring|divide|fill|stroke|outline|decoration|from|via|to|placeholder:text)-(?:white|black|slate|gray|zinc|neutral|stone|red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose)(?:-[0-9]+)?(?:\/[0-9]+)?\b/g;
const arbitraryUtility = /\b(?:bg|text|border|ring|shadow|fill|stroke)-\[[^\]]*(?:#[0-9a-fA-F]{3,8}(?=[^0-9a-fA-F]|$)|rgba?\(|hsla?\(|hsl\(var\(--)[^\]]*\]/g;
const directColorLiteral = /(?:rgba?|hsla?)\(|#[0-9a-fA-F]{6,8}(?=[^0-9a-fA-F]|$)|hsl\(var\(--/g;
const inlineHexColor = /\b(?:color|backgroundColor|borderColor|fill|stroke)\s*(?:=|:)\s*["'`]#[0-9a-fA-F]{3,8}\b/g;

const patterns = [rawPalette, arbitraryUtility, directColorLiteral, inlineHexColor];
const requestedRoots = process.argv.slice(2).filter((value) => value !== "--");
const roots = requestedRoots.length > 0 ? requestedRoots : ["src"];
const files = roots.flatMap((root) => collect(resolve(root)));
const violations = [];

for (const file of files) {
  const displayPath = relative(process.cwd(), file).replaceAll("\\", "/");
  const lines = readFileSync(file, "utf8").split(/\r?\n/);
  lines.forEach((line, index) => {
    for (const pattern of patterns) {
      const expression = new RegExp(pattern.source, pattern.flags);
      for (const match of line.matchAll(expression)) {
        violations.push(`${displayPath}:${index + 1}: ${match[0]}`);
      }
    }
  });
}

if (violations.length > 0) {
  console.error(violations.join("\n"));
  console.error(`theme audit found ${violations.length} violation(s)`);
  process.exitCode = 1;
} else {
  console.log(`theme audit passed (${files.length} files)`);
}

function collect(path) {
  const stat = statSync(path);
  if (stat.isFile()) {
    return [".ts", ".tsx"].includes(extname(path)) ? [path] : [];
  }
  return readdirSync(path, { withFileTypes: true }).flatMap((entry) => {
    const child = resolve(path, entry.name);
    return entry.isDirectory() ? collect(child) : collect(child);
  });
}
```

Add the package script:

```json
"theme:audit": "node scripts/theme-audit.mjs"
```

Do not add it to `build` until Task 13, because the existing application is intentionally RED.

- [ ] **Step 2: Run the audit to verify RED**

```powershell
pnpm theme:audit
```

Expected: exit 1 with violations including `src/components/ui/button.tsx` and `src/features/dashboard/DashboardPage.tsx`.

- [ ] **Step 3: Write the failing token contrast test**

Create `src/theme/themeContrast.test.ts`. Read `src/styles.css`, extract the `:root, .light` and `.dark` variable blocks, convert HSL triples to relative luminance, and assert every pair below is at least 4.5:1 in both blocks:

```ts
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const css = readFileSync(new URL("../styles.css", import.meta.url), "utf8");
const pairs = [
  ["foreground", "background"], ["foreground", "surface"],
  ["muted-foreground", "surface"], ["selected-foreground", "selected"],
  ["primary", "surface"], ["primary-foreground", "primary-solid"],
  ["on-solid", "danger-solid"],
  ...["success", "warning", "danger", "info"].map((tone) => [`${tone}-foreground`, `${tone}-surface`]),
  ...["anthropic", "openai", "gemini", "grok", "image", "generic"].map(
    (platform) => [`platform-${platform}-foreground`, `platform-${platform}-surface`],
  ),
] as Array<[string, string]>;

type Hsl = [number, number, number];

function themeBlock(theme: "light" | "dark") {
  const pattern = theme === "light"
    ? /:root,\s*\.light\s*\{([^}]*)\}/s
    : /\.dark\s*\{([^}]*)\}/s;
  const block = css.match(pattern)?.[1];
  if (!block) throw new Error(`Missing ${theme} theme block`);
  return block;
}

function parseVariables(block: string) {
  const values = new Map<string, Hsl>();
  for (const match of block.matchAll(/--([\w-]+):\s*([\d.]+)\s+([\d.]+)%\s+([\d.]+)%/g)) {
    values.set(match[1], [Number(match[2]), Number(match[3]), Number(match[4])]);
  }
  return values;
}

function readToken(values: Map<string, Hsl>, name: string) {
  const value = values.get(name);
  if (!value) throw new Error(`Missing theme token: ${name}`);
  return value;
}

function hslToRgb([h, saturation, lightness]: Hsl) {
  const s = saturation / 100;
  const l = lightness / 100;
  const chroma = (1 - Math.abs(2 * l - 1)) * s;
  const segment = h / 60;
  const secondary = chroma * (1 - Math.abs((segment % 2) - 1));
  const [red, green, blue] = segment < 1 ? [chroma, secondary, 0]
    : segment < 2 ? [secondary, chroma, 0]
      : segment < 3 ? [0, chroma, secondary]
        : segment < 4 ? [0, secondary, chroma]
          : segment < 5 ? [secondary, 0, chroma]
            : [chroma, 0, secondary];
  const offset = l - chroma / 2;
  return [red + offset, green + offset, blue + offset];
}

function relativeLuminance(hsl: Hsl) {
  const [red, green, blue] = hslToRgb(hsl).map((channel) =>
    channel <= 0.03928 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4,
  );
  return 0.2126 * red + 0.7152 * green + 0.0722 * blue;
}

function contrast(foreground: Hsl, background: Hsl) {
  const first = relativeLuminance(foreground);
  const second = relativeLuminance(background);
  return (Math.max(first, second) + 0.05) / (Math.min(first, second) + 0.05);
}

describe("theme token contrast", () => {
  it.each(["light", "dark"] as const)("keeps %s text pairs readable", (theme) => {
    const variables = parseVariables(themeBlock(theme));
    for (const [foreground, background] of pairs) {
      expect(
        contrast(readToken(variables, foreground), readToken(variables, background)),
        `${theme}: ${foreground}/${background}`,
      ).toBeGreaterThanOrEqual(4.5);
    }
  });
});
```

Run `pnpm test -- src/theme/themeContrast.test.ts`; expect FAIL because the semantic surface, state, platform, and solid tokens do not exist yet.

- [ ] **Step 4: Replace the root token block**

In `src/styles.css`, keep existing layout and keyframe rules. Move geometry variables into a theme-independent `:root` block, then add matching `:root, .light` and `.dark` color blocks. Use these exact values:

```css
:root {
  --surface-radius: 8px;
  --shell-sidebar-width: 64px;
  --shell-header-height: 52px;
  --shell-page-gap: 16px;
}

:root,
.light {
  color-scheme: light;
  --background: 220 14% 97%;
  --foreground: 222 22% 12%;
  --surface: 0 0% 100%;
  --surface-subtle: 220 14% 97%;
  --surface-inset: 220 14% 95%;
  --popover: 0 0% 100%;
  --muted: 220 14% 96%;
  --muted-foreground: 220 9% 42%;
  --border: 220 13% 89%;
  --border-strong: 220 10% 78%;
  --input: 220 13% 84%;
  --ring: 211 100% 52%;
  --hover: 220 14% 94%;
  --selected: 220 20% 90%;
  --selected-foreground: 222 22% 12%;
  --scrim: 222 22% 12%;
  --control-thumb: 0 0% 100%;
  --on-solid: 0 0% 100%;
  --primary: 211 100% 42%;
  --primary-solid: 211 100% 42%;
  --primary-foreground: 0 0% 100%;
  --accent: var(--primary);
  --accent-foreground: var(--primary-foreground);
  --success-surface: 152 60% 95%; --success-foreground: 158 64% 27%; --success-border: 151 38% 80%;
  --warning-surface: 48 100% 96%; --warning-foreground: 32 85% 28%; --warning-border: 45 80% 75%;
  --danger-surface: 0 86% 97%; --danger-foreground: 0 72% 40%; --danger-border: 0 72% 85%; --danger-solid: 350 75% 44%;
  --info-surface: 210 100% 96%; --info-foreground: 211 75% 40%; --info-border: 211 75% 84%;
  --platform-anthropic-surface: 28 100% 96%; --platform-anthropic-foreground: 24 80% 36%; --platform-anthropic-border: 25 75% 84%;
  --platform-openai-surface: 151 55% 95%; --platform-openai-foreground: 158 60% 28%; --platform-openai-border: 151 40% 82%;
  --platform-gemini-surface: 213 100% 96%; --platform-gemini-foreground: 217 75% 42%; --platform-gemini-border: 215 75% 85%;
  --platform-grok-surface: 220 10% 92%; --platform-grok-foreground: 220 12% 25%; --platform-grok-border: 220 10% 80%;
  --platform-image-surface: 260 100% 97%; --platform-image-foreground: 258 60% 45%; --platform-image-border: 260 65% 87%;
  --platform-generic-surface: 220 14% 96%; --platform-generic-foreground: 220 9% 42%; --platform-generic-border: 220 13% 84%;
  --surface-shadow: 0 8px 24px rgb(15 23 42 / 0.05);
  --surface-shadow-hover: 0 12px 30px rgb(15 23 42 / 0.08);
  --popover-shadow: 0 18px 48px rgb(15 23 42 / 0.14);
  --dialog-shadow: 0 24px 70px rgb(15 23 42 / 0.18);
}

.dark {
  color-scheme: dark;
  --background: 220 8% 10%;
  --foreground: 210 10% 94%;
  --surface: 220 7% 13%;
  --surface-subtle: 220 7% 16%;
  --surface-inset: 220 8% 9%;
  --popover: 220 7% 14%;
  --muted: 220 6% 19%;
  --muted-foreground: 215 8% 68%;
  --border: 220 6% 24%;
  --border-strong: 220 6% 32%;
  --input: 220 6% 30%;
  --ring: 211 90% 62%;
  --hover: 220 7% 19%;
  --selected: 220 7% 23%;
  --selected-foreground: 210 10% 96%;
  --scrim: 0 0% 0%;
  --control-thumb: 0 0% 92%;
  --on-solid: 0 0% 100%;
  --primary: 211 95% 70%;
  --primary-solid: 211 85% 45%;
  --primary-foreground: 0 0% 100%;
  --accent: var(--primary);
  --accent-foreground: var(--primary-foreground);
  --success-surface: 154 40% 16%; --success-foreground: 150 55% 70%; --success-border: 153 38% 30%;
  --warning-surface: 36 45% 16%; --warning-foreground: 42 80% 70%; --warning-border: 38 50% 32%;
  --danger-surface: 0 38% 18%; --danger-foreground: 0 75% 72%; --danger-border: 0 50% 35%; --danger-solid: 350 75% 47%;
  --info-surface: 211 42% 18%; --info-foreground: 210 85% 72%; --info-border: 211 55% 36%;
  --platform-anthropic-surface: 25 36% 17%; --platform-anthropic-foreground: 28 72% 72%; --platform-anthropic-border: 25 45% 34%;
  --platform-openai-surface: 154 35% 16%; --platform-openai-foreground: 151 52% 70%; --platform-openai-border: 153 36% 31%;
  --platform-gemini-surface: 216 38% 18%; --platform-gemini-foreground: 214 82% 74%; --platform-gemini-border: 216 48% 36%;
  --platform-grok-surface: 220 6% 19%; --platform-grok-foreground: 215 8% 76%; --platform-grok-border: 220 6% 34%;
  --platform-image-surface: 260 32% 19%; --platform-image-foreground: 258 72% 76%; --platform-image-border: 260 42% 38%;
  --platform-generic-surface: 220 6% 19%; --platform-generic-foreground: 215 8% 68%; --platform-generic-border: 220 6% 32%;
  --surface-shadow: 0 8px 28px rgb(0 0 0 / 0.28);
  --surface-shadow-hover: 0 12px 34px rgb(0 0 0 / 0.34);
  --popover-shadow: 0 18px 48px rgb(0 0 0 / 0.42);
  --dialog-shadow: 0 24px 70px rgb(0 0 0 / 0.52);
}
```

- [ ] **Step 5: Map every token through Tailwind**

In `tailwind.config.ts`, introduce:

```ts
const token = (name: string) => `hsl(var(--${name}) / <alpha-value>)`;
```

Map all core, state, and platform variables under `theme.extend.colors`. Use this structure so Tailwind emits names used by later tasks:

```ts
colors: {
  background: token("background"), foreground: token("foreground"),
  surface: token("surface"), "surface-subtle": token("surface-subtle"),
  "surface-inset": token("surface-inset"), popover: token("popover"),
  muted: token("muted"), "muted-foreground": token("muted-foreground"),
  border: token("border"), "border-strong": token("border-strong"),
  input: token("input"), ring: token("ring"), hover: token("hover"),
  selected: token("selected"), "selected-foreground": token("selected-foreground"),
  scrim: token("scrim"), "control-thumb": token("control-thumb"),
  "on-solid": token("on-solid"),
  primary: { DEFAULT: token("primary"), solid: token("primary-solid"), foreground: token("primary-foreground") },
  success: { surface: token("success-surface"), foreground: token("success-foreground"), border: token("success-border") },
  warning: { surface: token("warning-surface"), foreground: token("warning-foreground"), border: token("warning-border") },
  danger: { surface: token("danger-surface"), foreground: token("danger-foreground"), border: token("danger-border"), solid: token("danger-solid") },
  info: { surface: token("info-surface"), foreground: token("info-foreground"), border: token("info-border") },
  platform: Object.fromEntries(
    ["anthropic", "openai", "gemini", "grok", "image", "generic"].map((platform) => [
      platform,
      {
        surface: token(`platform-${platform}-surface`),
        foreground: token(`platform-${platform}-foreground`),
        border: token(`platform-${platform}-border`),
      },
    ]),
  ),
  accent: token("accent"),
  "accent-foreground": token("accent-foreground"),
},
```

Keep the `accent` compatibility mapping until Task 13; it prevents unmigrated intermediate pages from losing focus and primary colors. Add:

```ts
boxShadow: {
  surface: "var(--surface-shadow)",
  "surface-hover": "var(--surface-shadow-hover)",
  popover: "var(--popover-shadow)",
  dialog: "var(--dialog-shadow)",
}
```

Keep `darkMode: ["class"]` unchanged.

- [ ] **Step 6: Verify token compilation and commit**

```powershell
pnpm test -- src/theme/themeContrast.test.ts
pnpm build
pnpm theme:audit -- src/theme
git add -- scripts/theme-audit.mjs src/theme/themeContrast.test.ts src/styles.css tailwind.config.ts package.json
git commit -m "feat: define semantic application colors"
```

Expected: build passes; the scoped theme runtime audit passes; the full application audit remains RED until migration completes.

---

### Task 6: Migrate shared UI components and extend SegmentedControl

**Files:**
- Modify: `src/components/ui/ActivityList.tsx`
- Modify: `src/components/ui/button.tsx`
- Modify: `src/components/ui/Card.tsx`
- Modify: `src/components/ui/ConfirmDialog.tsx`
- Modify: `src/components/ui/DataTableLite.tsx`
- Modify: `src/components/ui/Dialog.tsx`
- Modify: `src/components/ui/EmptyState.tsx`
- Modify: `src/components/ui/InspectorPanel.tsx`
- Modify: `src/components/ui/KeyValueRow.tsx`
- Modify: `src/components/ui/layout.ts`
- Modify: `src/components/ui/MaskedSecret.tsx`
- Modify: `src/components/ui/MetricCard.tsx`
- Modify: `src/components/ui/MetricPanel.tsx`
- Modify: `src/components/ui/ObjectRow.tsx`
- Modify: `src/components/ui/PageForm.tsx`
- Modify: `src/components/ui/PropertyList.tsx`
- Modify: `src/components/ui/SectionCard.tsx`
- Modify: `src/components/ui/SegmentedControl.tsx`
- Create: `src/components/ui/SegmentedControl.test.tsx`
- Modify: `src/components/ui/SelectControl.tsx`
- Modify: `src/components/ui/StatusBadge.tsx`
- Modify: `src/components/ui/SwitchControl.tsx`
- Modify: `src/components/ui/ToastProvider.tsx`
- Modify: `src/components/ui/Toolbar.tsx`

- [ ] **Step 1: Write the failing optional-icon test**

Create `src/components/ui/SegmentedControl.test.tsx`:

```tsx
// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { Monitor, Moon, Sun } from "lucide-react";
import { describe, expect, it, vi } from "vitest";
import { SegmentedControl } from "./SegmentedControl";

type Mode = "light" | "dark" | "system";

const options: Array<{ value: Mode; label: string; icon: typeof Sun }> = [
  { value: "light", label: "日间", icon: Sun },
  { value: "dark", label: "夜间", icon: Moon },
  { value: "system", label: "跟随系统", icon: Monitor },
];

describe("SegmentedControl icons", () => {
  it("keeps labels accessible and layout tracks stable", async () => {
    const onChange = vi.fn();
    const host = document.createElement("div");
    const root = createRoot(host);
    await act(async () => root.render(
      <SegmentedControl ariaLabel="外观模式" options={options} value="light" onChange={onChange} />,
    ));
    const group = host.querySelector<HTMLElement>('[role="radiogroup"]')!;
    const columns = group.style.gridTemplateColumns;
    const radios = [...host.querySelectorAll<HTMLElement>('[role="radio"]')];
    expect(radios.map((radio) => radio.textContent)).toEqual(["日间", "夜间", "跟随系统"]);
    expect([...host.querySelectorAll("svg")].every((icon) => icon.getAttribute("aria-hidden") === "true")).toBe(true);
    await act(async () => group.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true })));
    expect(onChange).toHaveBeenCalledWith("dark");
    await act(async () => root.render(
      <SegmentedControl ariaLabel="外观模式" options={options} value="dark" onChange={onChange} />,
    ));
    expect(host.querySelector<HTMLElement>('[role="radiogroup"]')!.style.gridTemplateColumns).toBe(columns);
    await act(async () => root.unmount());
  });
});
```

Run `pnpm test -- src/components/ui/SegmentedControl.test.tsx`; expect TypeScript/transform failure because `icon` is not accepted.

- [ ] **Step 2: Extend the option type without breaking existing callers**

```tsx
import type { LucideIcon } from "lucide-react";

type SegmentedControlOption<T extends string> = {
  value: T;
  label: string;
  icon?: LucideIcon;
  disabled?: boolean;
};

// Inside each option button:
const Icon = option.icon;
return (
  <span className="flex min-w-0 items-center justify-center gap-1.5">
    {Icon ? <Icon aria-hidden="true" className="h-3.5 w-3.5 shrink-0" /> : null}
    <span className="truncate">{option.label}</span>
  </span>
);
```

- [ ] **Step 3: Centralize exact shared component semantics**

Use these mappings in shared components:

```ts
const buttonVariants = {
  primary: "bg-primary-solid text-primary-foreground shadow-surface hover:bg-primary-solid/90",
  secondary: "border border-border bg-surface text-foreground hover:bg-hover",
  ghost: "text-muted-foreground hover:bg-hover hover:text-foreground",
  outline: "border border-border bg-surface text-foreground hover:bg-hover",
  danger: "border border-danger-border bg-danger-surface text-danger-foreground hover:bg-danger-surface/80",
};

const toneClassName = {
  healthy: "border-success-border bg-success-surface text-success-foreground",
  warning: "border-warning-border bg-warning-surface text-warning-foreground",
  error: "border-danger-border bg-danger-surface text-danger-foreground",
  disabled: "border-border bg-muted text-muted-foreground",
  info: "border-info-border bg-info-surface text-info-foreground",
};
```

Use `bg-scrim/45 shadow-dialog` for dialogs, `bg-popover shadow-popover` for menus, `bg-control-thumb` for switch thumbs, and semantic focus classes `focus-visible:ring-ring/30`.

- [ ] **Step 4: Migrate every shared file listed in this task**

Apply the Semantic Class Contract to every listed file. Remove `layout.cardShadow`; shadows must come from semantic Tailwind classes/CSS variables. Preserve component props, dimensions, aria behavior, Portal targets, animation durations, and page activity behavior.

- [ ] **Step 5: Verify the shared boundary is clean**

```powershell
pnpm test -- src/components/ui/SegmentedControl.test.tsx
pnpm theme:audit -- src/components/ui
pnpm build
```

Expected: all commands exit 0 and the audit reports zero raw colors under `src/components/ui`.

- [ ] **Step 6: Commit shared components**

```powershell
git add -- src/components/ui
git commit -m "refactor: theme shared interface components"
```

---

### Task 7: Migrate the application shell and page host visuals

**Files:**
- Modify: `src/components/shell/AppShell.tsx`
- Modify: `src/components/shell/PageScaffold.tsx`
- Modify: `src/app/ShellPageErrorBoundary.tsx`
- Modify: `src/styles.css`

- [ ] **Step 1: Convert Shell selection and status surfaces**

Replace the sidebar and navigation semantics without changing its 64px geometry:

```text
aside bg-white                         -> bg-surface
inactive text-slate-500                -> text-muted-foreground
inactive hover                         -> hover:bg-hover hover:text-foreground
active bg-slate-900 text-white         -> bg-selected text-selected-foreground
proxy healthy/idle raw dots            -> text-success-foreground / text-warning-foreground
```

Keep `data-navigation-route-id`, unread count behavior, current route logic, focus behavior, and `shellLayout.sidebarWidth` unchanged.

- [ ] **Step 2: Migrate page scaffold and error boundary**

Use `bg-background`, `bg-surface`, `text-foreground`, `text-muted-foreground`, and semantic danger classes. Keep existing transition layer positioning and scroll contracts unchanged.

- [ ] **Step 3: Verify Shell in isolation**

```powershell
pnpm theme:audit -- src/components/shell src/app/ShellPageErrorBoundary.tsx
pnpm build
```

Expected: audit and build exit 0.

- [ ] **Step 4: Commit Shell migration**

```powershell
git add -- src/components/shell/AppShell.tsx src/components/shell/PageScaffold.tsx src/app/ShellPageErrorBoundary.tsx src/styles.css
git commit -m "refactor: theme the desktop application shell"
```

---

### Task 8: Add the settings appearance control

**Files:**
- Create: `src/features/settings/ThemeSettings.tsx`
- Create: `src/features/settings/ThemeSettings.test.tsx`
- Modify: `src/features/settings/SettingsPage.tsx`

- [ ] **Step 1: Write the failing settings control tests**

Create `src/features/settings/ThemeSettings.test.tsx`:

```tsx
// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  setPreference: vi.fn(() => ({ persisted: true })),
  toastError: vi.fn(),
}));

vi.mock("@/theme/ThemeProvider", () => ({
  useTheme: () => ({
    preference: "light",
    resolvedTheme: "light",
    setPreference: mocks.setPreference,
  }),
}));

vi.mock("@/components/ui", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/components/ui")>();
  return { ...actual, useToast: () => ({ error: mocks.toastError }) };
});

import { ThemeSettings } from "./ThemeSettings";

describe("ThemeSettings", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.setPreference.mockReturnValue({ persisted: true });
  });

  it("renders three accessible choices with decorative icons", async () => {
    const host = document.createElement("div");
    const root = createRoot(host);
    await act(async () => root.render(<ThemeSettings />));
    const radios = [...host.querySelectorAll<HTMLElement>('[role="radio"]')];
    expect(radios.map((radio) => radio.textContent)).toEqual(["日间", "夜间", "跟随系统"]);
    expect([...host.querySelectorAll("svg")].every((icon) => icon.getAttribute("aria-hidden") === "true")).toBe(true);
    await act(async () => radios[1].dispatchEvent(new MouseEvent("click", { bubbles: true })));
    expect(mocks.setPreference).toHaveBeenCalledWith("dark");
    expect(mocks.toastError).not.toHaveBeenCalled();
    await act(async () => root.unmount());
  });

  it("reports one persistence failure while keeping the session change", async () => {
    mocks.setPreference.mockReturnValue({ persisted: false });
    const host = document.createElement("div");
    const root = createRoot(host);
    await act(async () => root.render(<ThemeSettings />));
    const dark = host.querySelectorAll<HTMLElement>('[role="radio"]')[1];
    await act(async () => dark.dispatchEvent(new MouseEvent("click", { bubbles: true })));
    expect(mocks.toastError).toHaveBeenCalledTimes(1);
    expect(mocks.toastError).toHaveBeenCalledWith(
      "主题偏好无法保存",
      "主题已切换，但偏好无法保存；重启后可能恢复上次设置",
    );
    await act(async () => root.unmount());
  });
});
```

Run `pnpm test -- src/features/settings/ThemeSettings.test.tsx`; expect FAIL because the component does not exist.

- [ ] **Step 2: Implement the focused appearance section**

Create `ThemeSettings.tsx`:

```tsx
import { Monitor, Moon, Sun } from "lucide-react";
import { SectionCard, SegmentedControl, useToast } from "@/components/ui";
import { useTheme } from "@/theme/ThemeProvider";
import type { ThemePreference } from "@/theme/theme";

const themeOptions = [
  { value: "light", label: "日间", icon: Sun },
  { value: "dark", label: "夜间", icon: Moon },
  { value: "system", label: "跟随系统", icon: Monitor },
] satisfies Array<{ value: ThemePreference; label: string; icon: typeof Sun }>;

export function ThemeSettings() {
  const toast = useToast();
  const { preference, setPreference } = useTheme();

  function handleChange(next: ThemePreference) {
    const result = setPreference(next);
    if (!result.persisted) {
      toast.error("主题偏好无法保存", "主题已切换，但偏好无法保存；重启后可能恢复上次设置");
    }
  }

  return (
    <SectionCard contentClassName="px-5 py-4" title="外观">
      <SegmentedControl
        ariaLabel="外观模式"
        className="w-full max-w-[360px]"
        options={themeOptions}
        value={preference}
        onChange={handleChange}
      />
    </SectionCard>
  );
}
```

- [ ] **Step 3: Insert it before backend-dependent settings**

Import `ThemeSettings` in `SettingsPage.tsx` and render it as the first section inside the settings grid. Do not add it to `SettingsFormState`, `fallbackSettings`, `settingsToForm`, `commitSettingsForm`, `SETTINGS_UPDATED_EVENT`, or React Query.

- [ ] **Step 4: Migrate every SettingsPage color role**

Apply the semantic class contract to data directory, local access key, inputs, error text, update card, badges, and hover/focus states. Keep the existing settings save/refresh behavior unchanged.

- [ ] **Step 5: Verify and commit**

```powershell
pnpm test -- src/features/settings/ThemeSettings.test.tsx
pnpm theme:audit -- src/features/settings
pnpm build
git add -- src/features/settings/ThemeSettings.tsx src/features/settings/ThemeSettings.test.tsx src/features/settings/SettingsPage.tsx
git commit -m "feat: add application appearance settings"
```

Expected: tests, audit, and build exit 0.

---

### Task 9: Migrate dashboard, changes, logs, and updater surfaces

**Files:**
- Modify: `src/features/dashboard/DashboardPage.tsx`
- Modify: `src/features/changes/ChangeCenterPage.tsx`
- Modify: `src/features/logs/LogsPage.tsx`
- Modify: `src/features/logs/RequestLogTable.tsx`
- Modify: `src/features/updater/UpdateDialog.tsx`

- [ ] **Step 1: Replace Dashboard arbitrary colors and gradients**

Use this exact intent mapping:

```text
quick action default       bg-surface border-border text-muted-foreground
quick action active        bg-primary-solid text-primary-foreground border-primary
summary and empty cards    bg-surface border-border shadow-surface
neutral metric icon        bg-info-surface text-info-foreground
healthy/warning/error      corresponding semantic state triples
focus ring                 ring-ring/30
```

Remove `#EFF0F3`, `#0060DF`, `#0052bf`, raw `rgba()` shadows, and raw slate/status classes. Preserve card sizes, grid tracks, query behavior, import-to-CCSwitch behavior, and loading states.

- [ ] **Step 2: Migrate Changes and Logs**

Use surface tokens for table/list containers, status tokens for change severity and request outcomes, and muted tokens for metadata. Do not change event filtering, pagination, clearing, or request-log formatting.

- [ ] **Step 3: Migrate updater Dialog**

Use shared dialog/surface/shadow tokens. Preserve updater state transitions, progress display, cancel/restart behavior, and Portal layering.

- [ ] **Step 4: Verify the domain batch and commit**

```powershell
pnpm theme:audit -- src/features/dashboard src/features/changes src/features/logs src/features/updater/UpdateDialog.tsx
pnpm build
git add -- src/features/dashboard/DashboardPage.tsx src/features/changes/ChangeCenterPage.tsx src/features/logs/LogsPage.tsx src/features/logs/RequestLogTable.tsx src/features/updater/UpdateDialog.tsx
git commit -m "refactor: theme dashboard and activity views"
```

Expected: audit and build exit 0.

---

### Task 10: Migrate channels, collectors, and routing with typed tones

**Files:**
- Create: `src/features/channels/channelStatusViewModel.test.ts`
- Modify: `src/features/channels/channelStatusViewModel.ts`
- Modify: `src/features/channels/ChannelMonitorForm.tsx`
- Modify: `src/features/channels/ChannelMonitoringTab.tsx`
- Modify: `src/features/channels/ChannelMonitorTemplateManager.tsx`
- Modify: `src/features/channels/ChannelStatusTab.tsx`
- Modify: `src/features/collectors/CollectorAdvancedSettings.tsx`
- Modify: `src/features/collectors/CollectorsPage.tsx`
- Modify: `src/features/routing/LocalRoutingCandidateRow.tsx`
- Modify: `src/features/routing/LocalRoutingEditTab.tsx`
- Modify: `src/features/routing/LocalRoutingSettingsEditor.tsx`
- Modify: `src/features/routing/LocalRoutingSettingsFields.tsx`
- Modify: `src/features/routing/LocalRoutingStatusCandidateRow.tsx`
- Modify: `src/features/routing/LocalRoutingStatusTab.tsx`

- [ ] **Step 1: Write a failing typed availability test**

Create `src/features/channels/channelStatusViewModel.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { availabilityTone } from "./channelStatusViewModel";

describe("availabilityTone", () => {
  it.each([
    [{ status: "disabled", availabilityPercent: 99 }, "muted"],
    [{ status: "healthy", availabilityPercent: null }, "muted"],
    [{ status: "healthy", availabilityPercent: 49.9 }, "danger"],
    [{ status: "healthy", availabilityPercent: 50 }, "warning"],
    [{ status: "healthy", availabilityPercent: 74.9 }, "warning"],
    [{ status: "healthy", availabilityPercent: 75 }, "success"],
  ] as const)("maps %o to %s", (channel, tone) => {
    expect(availabilityTone(channel)).toBe(tone);
  });
});
```

Require this semantic return type:

```ts
export type AvailabilityTone = "muted" | "danger" | "warning" | "success";
export function availabilityTone(channel: ChannelAvailabilityState): AvailabilityTone;
```

Run `pnpm test -- src/features/channels/channelStatusViewModel.test.ts`; expect FAIL because only `availabilityToneClassName` exists.

- [ ] **Step 2: Move color choice out of the view model**

Implement `availabilityTone` with the existing thresholds and map it in `ChannelStatusTab.tsx`:

```ts
const availabilityToneClassName: Record<AvailabilityTone, string> = {
  muted: "text-muted-foreground",
  danger: "text-danger-foreground",
  warning: "text-warning-foreground",
  success: "text-success-foreground",
};
```

Update all callers and remove the old class-returning function.

- [ ] **Step 3: Migrate channel, collector, and routing UI**

Apply semantic surfaces and status tokens. Use `primary` for selected monitor/routing choices, not teal. Preserve drag handles, monitor scheduling, cooldown clocks, candidate ordering, form validation, query activation, and routing simulation behavior.

- [ ] **Step 4: Verify and commit**

```powershell
pnpm test -- src/features/channels/channelStatusViewModel.test.ts
pnpm theme:audit -- src/features/channels src/features/collectors src/features/routing
pnpm build
git add -- src/features/channels src/features/collectors src/features/routing
git commit -m "refactor: theme channel and routing workflows"
```

Expected: tests, scoped audit, and build exit 0.

---

### Task 11: Migrate Key Pool and pricing workflows

**Files:**
- Modify: `src/features/key-pool/AddKeyPage.tsx`
- Modify: `src/features/key-pool/EditKeyPage.tsx`
- Modify: `src/features/key-pool/KeyPoolPage.tsx`
- Modify: `src/features/pricing/ModelBasePricesPage.tsx`
- Modify: `src/features/pricing/PricingPage.tsx`

- [ ] **Step 1: Migrate Key Pool forms, rows, and health states**

Convert inputs to `bg-surface-inset border-input text-foreground`, selected provider/key choices to primary/selected tokens, and health/balance states to success/warning/danger/info. Keep Station Key CRUD, reorder, filter, drag, key masking, and optimistic state unchanged.

- [ ] **Step 2: Migrate pricing tables and popovers**

Convert price cells, editable inputs, pagination, menus, and comparison states. Replace raw accent HSL and teal pagination styles with `primary`, `selected`, `hover`, `ring`, `popover`, and semantic shadows. Preserve price normalization, sorting, editing, pagination, and model base price reset behavior.

- [ ] **Step 3: Verify and commit**

```powershell
pnpm theme:audit -- src/features/key-pool src/features/pricing
pnpm build
git add -- src/features/key-pool src/features/pricing
git commit -m "refactor: theme key pool and pricing workflows"
```

Expected: scoped audit and build exit 0.

---

### Task 12: Migrate station assets and platform identity colors

**Files:**
- Create: `src/features/stations/groupVisualStyles.ts`
- Create: `src/features/stations/groupVisualMeta.test.ts`
- Modify: `src/features/stations/groupVisualMeta.ts`
- Modify: `src/features/stations/AddProviderPage.tsx`
- Modify: `src/features/stations/StationDetailPage.tsx`
- Modify: `src/features/stations/StationsPage.tsx`
- Modify: `src/features/stations/components/CreateRemoteKeyDialog.tsx`
- Modify: `src/features/stations/components/RemoteKeyDiscoveryList.tsx`
- Modify: `src/features/stations/components/StationDetailContent.tsx`
- Modify: `src/features/stations/components/StationDetailPanel.tsx`
- Modify: `src/features/stations/components/StationGroupChip.tsx`
- Modify: `src/features/stations/components/StationGroupRowsEditor.tsx`
- Modify: `src/features/stations/components/StationKeyRowsEditor.tsx`
- Modify: `src/features/stations/components/StationListItem.tsx`
- Modify: `src/features/stations/components/StationStatusDot.tsx`
- Modify: `src/features/pricing/PricingPage.tsx`

- [ ] **Step 1: Write a failing platform metadata boundary test**

Create `src/features/stations/groupVisualMeta.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { groupVisualMetaFor } from "./groupVisualMeta";

describe("groupVisualMetaFor", () => {
  it.each([
    ["claude", "anthropic"],
    ["gpt", "openai"],
    ["gemini", "gemini"],
    ["grok", "grok"],
    ["image-generation", "image"],
    ["embedding", "generic"],
  ] as const)("maps %s to %s without visual classes", (groupName, platform) => {
    const meta = groupVisualMetaFor(groupName);
    expect(meta.platform).toBe(platform);
    expect(meta).not.toHaveProperty("badgeClassName");
    expect(meta).not.toHaveProperty("iconClassName");
    expect(meta).not.toHaveProperty("rateBadgeClassName");
  });
});
```

Run `pnpm test -- src/features/stations/groupVisualMeta.test.ts`; expect FAIL because raw classes are still part of the object.

- [ ] **Step 2: Separate inference from visual class mapping**

Reduce `StationGroupVisualMeta` to:

```ts
export type StationGroupVisualMeta = {
  platform: StationGroupVisualPlatform;
  label: string;
};
```

Create `groupVisualStyles.ts`:

```ts
import type { StationGroupVisualPlatform } from "./groupVisualMeta";

export const groupVisualClassNames: Record<StationGroupVisualPlatform, {
  badge: string;
  icon: string;
  rateBadge: string;
}> = {
  anthropic: { badge: "border-platform-anthropic-border bg-platform-anthropic-surface text-platform-anthropic-foreground", icon: "text-platform-anthropic-foreground", rateBadge: "bg-platform-anthropic-surface text-platform-anthropic-foreground" },
  openai: { badge: "border-platform-openai-border bg-platform-openai-surface text-platform-openai-foreground", icon: "text-platform-openai-foreground", rateBadge: "bg-platform-openai-surface text-platform-openai-foreground" },
  gemini: { badge: "border-platform-gemini-border bg-platform-gemini-surface text-platform-gemini-foreground", icon: "text-platform-gemini-foreground", rateBadge: "bg-platform-gemini-surface text-platform-gemini-foreground" },
  grok: { badge: "border-platform-grok-border bg-platform-grok-surface text-platform-grok-foreground", icon: "text-platform-grok-foreground", rateBadge: "bg-platform-grok-surface text-platform-grok-foreground" },
  image: { badge: "border-platform-image-border bg-platform-image-surface text-platform-image-foreground", icon: "text-platform-image-foreground", rateBadge: "bg-platform-image-surface text-platform-image-foreground" },
  generic: { badge: "border-platform-generic-border bg-platform-generic-surface text-platform-generic-foreground", icon: "text-platform-generic-foreground", rateBadge: "bg-platform-generic-surface text-platform-generic-foreground" },
};
```

The explicit map is required because Tailwind cannot detect runtime-composed class names.

- [ ] **Step 3: Update group consumers**

Update `StationGroupChip.tsx` and `PricingPage.tsx` to look up `groupVisualClassNames[meta.platform]`. `Sub2ApiPlatformIcon.tsx` continues consuming only the platform type and does not need a style lookup. Keep platform inference and display labels unchanged.

- [ ] **Step 4: Migrate all station surfaces**

Apply the semantic contract to station list rows, selected-row background, drag handles, inspectors, login/key/group editors, detail panels, remote-key Dialog, status dots, empty/error states, and transient Add/Edit Provider forms. Replace the selected-row hex gradient with `bg-selected border-primary/45`; preserve row height, drawer geometry, drag behavior, capture flows, and station CRUD.

- [ ] **Step 5: Verify and commit**

```powershell
pnpm test -- src/features/stations/groupVisualMeta.test.ts
pnpm theme:audit -- src/features/stations src/features/pricing/PricingPage.tsx
pnpm build
git add -- src/features/stations src/features/pricing/PricingPage.tsx
git commit -m "refactor: theme station asset workflows"
```

Expected: tests, scoped audit, and build exit 0; platform chips retain distinct semantic identities in both themes.

---

### Task 13: Enforce the full audit and run end-to-end verification

**Files:**
- Modify: `package.json`
- Modify: any task-scope UI file found by the final audit or visual verification

- [ ] **Step 1: Run the full audit before enforcing it**

```powershell
pnpm theme:audit
```

Expected: exit 0 and `theme audit passed`; if violations remain, replace each with an existing semantic token or add a justified semantic token to `styles.css` and `tailwind.config.ts`, then rerun until zero.

- [ ] **Step 2: Make audit a build gate**

After the full audit passes, run:

```powershell
rg -n "(?:bg|text|border|ring)-accent|hsl\(var\(--accent|--accent" src tailwind.config.ts
```

Expected: only the compatibility definitions in `src/styles.css` and `tailwind.config.ts` remain. Remove those `accent`/`accent-foreground` mappings and CSS aliases, rerun the audit and compiler, then change the build script to:

```json
"build": "pnpm theme:audit && tsc --noEmit && vite build"
```

- [ ] **Step 3: Run all automated verification**

```powershell
pnpm test
pnpm theme:audit
pnpm build
pnpm tauri info
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: every command exits 0; no unhandled Promise rejection; audit reports zero raw colors.

- [ ] **Step 4: Verify browser theme flows at the product viewport**

Start:

```powershell
pnpm dev
```

Open `http://127.0.0.1:1430` at 1180x760 and verify:

```text
localStorage absent                         -> system preference
select 日间 / 夜间 / 跟随系统                -> immediate class and color-scheme update
reload after each selection                 -> preference persists
invalid stored value                        -> system fallback
emulated prefers-color-scheme light/dark    -> system mode updates only
manual light/dark + media change             -> manual choice remains
all data-navigation-route-id buttons         -> no raw light panels or unreadable text
all transient add/edit/detail views          -> themed surfaces and stable layout
Dialog / Select / Toast / Inspector           -> themed Portal content
hover / focus / disabled / selected / error   -> visible and semantically correct
```

Capture desktop screenshots for every shell route in light and dark. Do not commit screenshots unless the user explicitly asks for them.

- [ ] **Step 5: Verify the real Windows/Tauri chain**

Run:

```powershell
pnpm tauri:dev
```

Verify the main window title bar follows explicit light/dark choices, `system` returns control to Windows, live Windows theme changes update the WebView, rapid `light -> dark -> system` ends in system mode, and application restart restores the stored preference. Open a `capture-*` authorization window and confirm the third-party page is not recolored.

- [ ] **Step 6: Check accessibility and layout invariants**

For the final token values and screenshots, verify:

```text
normal text contrast >= 4.5:1
focus/control/non-text contrast >= 3:1
status is not conveyed by color alone
no overlap or clipping at 1180x760
theme switching changes no element dimensions, table tracks, scroll position, or dialog geometry
```

If a token fails contrast, change the token value centrally in `styles.css`, rerun all screenshots, `pnpm test`, and `pnpm build`.

- [ ] **Step 7: Commit final enforcement and QA fixes**

Stage only `package.json` plus the exact UI/token files changed by final verification:

```powershell
git add -- package.json src/styles.css tailwind.config.ts
git status --short
git commit -m "test: enforce complete theme coverage"
```

Before committing, add each additional verified fix by its explicit file path and confirm `git diff --cached --name-only` contains no unrelated Rust, database, plan, log, screenshot, or local configuration files.

---

## Final Spec Coverage Check

| Spec requirement | Implemented by |
|---|---|
| Three-state model and system default | Tasks 1-2 |
| Safe device-only persistence | Task 2 |
| Pre-React bootstrap and no wrong React frame | Tasks 2 and 4 |
| Single Provider and conditional system listener | Task 4 |
| Strict Mode and native race protection | Tasks 3-4 |
| Main-window-only Tauri permission | Task 4 |
| Semantic light/dark tokens and shadows | Task 5 |
| Platform identity tokens | Tasks 5 and 12 |
| Optional icons and accessible segmented control | Tasks 6 and 8 |
| Settings-only immediate entry | Task 8 |
| Storage failure feedback | Task 8 |
| Shared UI, Portal, Shell, all shell pages and transient pages | Tasks 6-12 |
| Typed view-model tones | Tasks 10 and 12 |
| Zero raw palette regressions | Tasks 5 and 13 |
| Unit, build, Cargo, browser, accessibility and real Tauri verification | Task 13 |
| Capture windows remain untouched | Tasks 4 and 13 |

## Completion Contract

Implementation is complete only when `pnpm test`, `pnpm theme:audit`, `pnpm build`, `pnpm tauri info`, and `cargo check --manifest-path src-tauri/Cargo.toml` all exit 0; browser screenshots cover every route in both effective themes; and real Tauri verification confirms title-bar/system behavior. A partial page migration, passing TypeScript with audit violations, or browser-only verification is not completion.
