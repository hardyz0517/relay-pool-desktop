# Motion Transient Page Transition Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 用一个集中式 Framer Motion 宿主管理所有内部子页面的淡入淡出，同时保留父 shell 页面实例、滚动位置、数据激活隔离和实体级状态隔离。

**Architecture:** `App.tsx` 只负责导航、保活 shell、父子路由状态和 transient descriptor；`TransientPageHost.tsx` 独占 Framer Motion import、presence 生命周期、退出内容隔离和点击屏障。父 shell 使用 `active | background | inactive` 三态，子页打开时父页保持可见但不可交互，关闭时由 `AnimatePresence` 保留原子页直到 200ms 淡出完成。

**Tech Stack:** React 18、TypeScript、Framer Motion 12、Vite、Node.js contract scripts、Playwright/system Chrome

---

## File Map

- Create: `src/app/TransientPageHost.tsx` - 唯一 Motion 边界，管理 transient presence、opacity 动画、退出隔离与 pointer shield。
- Create: `src/components/ui/InteractionActivity.tsx` - 向普通 DOM 和 portal 统一传播页面交互状态。
- Create: `scripts/motion-page-transition.test.mjs` - 固化依赖版本、唯一 import、Motion 配置和纯 opacity 契约。
- Create: `scripts/page-transition-focus-scroll.test.mjs` - 固化独立滚动、进入焦点和返回焦点生命周期。
- Modify: `src/app/App.tsx` - 删除手工退出状态机，构造实体级 descriptor，并为 shell 输出三态。
- Modify: `src/app/pageTransitionPolicy.ts` - 删除未消费的 direction 元数据。
- Modify: `src/components/shell/AppShell.tsx` - 将页面滚动所有权移交给 transition layer。
- Modify: `src/components/shell/PageActivity.tsx` - 通过共享交互上下文继续隔离失活页面。
- Modify: `src/components/ui/SelectControl.tsx` - 页面失活时关闭 portal 菜单。
- Modify: `src/features/stations/StationDetailPage.tsx` - 将 seed 重置改为 passive effect，保留首帧 seed。
- Modify: `src/lib/types/navigation.ts` - 导出穷尽的 `TransientPageId`。
- Modify: `scripts/page-transition-container.test.mjs` - 固化 App/Host 分工、三态 shell、旧状态机彻底删除和单一 Host。
- Modify: `scripts/page-transition-policy.test.mjs` - 固化精简 policy 和 direction 字段删除。
- Modify: `scripts/page-activation-refresh.test.mjs` - 让激活测试匹配 shell 三态，并继续禁止 background 页面刷新。
- Modify: `scripts/station-detail-transition-performance.test.mjs` - 将实体隔离契约从页面局部 `key` 迁移到 Host `instanceKey`。
- Modify: `src/styles.css` - 保留 shell 几何和主导航动画，删除 transient keyframes，增加 background 与 pointer-shield 样式。
- Modify: `scripts/page-transition-styles.test.mjs` - 固化父页可见、退出遮罩拦截、纯 opacity Motion 和旧 keyframes 删除。
- Modify: `package.json` - 增加 `framer-motion` 运行时依赖。
- Modify: `pnpm-lock.yaml` - 锁定 Motion 及其传递依赖。

业务 API、数据选择器、Tauri/Rust 和数据库保持不变；实现复核允许且实际包含上列交互上下文、portal 收口和 `StationDetailPage` effect 调度调整，不改变业务语义。

## Implementation review amendments

以下 review 结论覆盖原计划中冲突的代码片段和范围声明：

- `InteractionActivityProvider` 必须覆盖 portal 消费者，`SelectControl` 在页面失活时同步关闭 portal 菜单，避免 transient 退出后留下脱离 inert 层的交互面。
- `StationDetailPage` 的 seed 重置必须保留在 passive `useEffect`；layout effect 会在首帧前清空 seed，破坏无空白过渡前提。
- descriptor 使用从 `navigation.ts` 导入的穷尽 `TransientPageId`，policy 只保留 `pageId`、`kind`、`parentRouteId`，删除 direction 类型和字段。
- retained shell layer 以 `z-index: 0` 和 `isolation: isolate` 建立堆叠上下文；transient overlay 使用 `z-index: 1` 覆盖。
- `main` 改为 `overflow-hidden`；全高 stack 下每个 shell/transient layer 独立 `overflow-y: auto`，分别持有并保留自身 `scrollTop`。
- `App` 集中捕获 active shell 的调用控件，Host 在 transient 挂载时聚焦首个 actionable control，并仅在最终关闭到 shell 后用 `preventScroll` 恢复精确调用控件。
- controller 的 real Chrome 复核必须包含非零 shell scroll、独立 detail scroll、进入焦点和返回焦点/滚动不变；本实现线程只运行自动化和构建检查。

### Task 1: 建立唯一 Motion 宿主边界

**Files:**
- Create: `scripts/motion-page-transition.test.mjs`
- Create: `src/app/TransientPageHost.tsx`
- Modify: `package.json`
- Modify: `pnpm-lock.yaml`

- [ ] **Step 1: 写入失败的 Motion 边界测试**

创建 `scripts/motion-page-transition.test.mjs`：

```js
import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import path from "node:path";

async function readSourceFiles(root) {
  const entries = await readdir(root, { withFileTypes: true });
  const nested = await Promise.all(
    entries.map(async (entry) => {
      const entryPath = path.join(root, entry.name);
      if (entry.isDirectory()) {
        return readSourceFiles(entryPath);
      }
      return /\.[cm]?[jt]sx?$/.test(entry.name) ? [entryPath] : [];
    }),
  );
  return nested.flat();
}

const packageJson = JSON.parse(await readFile("package.json", "utf8"));
const hostPath = path.normalize("src/app/TransientPageHost.tsx");
const hostSource = await readFile(hostPath, "utf8");
const sourceFiles = await readSourceFiles("src");
const motionImporters = [];

for (const sourcePath of sourceFiles) {
  const source = await readFile(sourcePath, "utf8");
  if (/from ["']framer-motion["']/.test(source)) {
    motionImporters.push(path.normalize(sourcePath));
  }
}

assert.equal(
  packageJson.dependencies?.["framer-motion"],
  "^12.23.25",
  "framer-motion should be a pinned runtime dependency",
);
assert.deepEqual(
  motionImporters,
  [hostPath],
  "TransientPageHost should be the only source module importing framer-motion",
);
assert.ok(
  hostSource.includes('<MotionConfig reducedMotion="user">') &&
    hostSource.includes('<AnimatePresence initial={false} mode="wait">'),
  "the host should centralize reduced-motion and wait-mode presence behavior",
);
assert.ok(
  hostSource.includes("useIsPresent()") &&
    hostSource.includes("active={isPresent}") &&
    hostSource.includes('inert={isPresent ? undefined : ""}') &&
    hostSource.includes("aria-hidden={!isPresent}"),
  "exiting page content should become inactive, inert, and hidden from assistive technology",
);
assert.ok(
  hostSource.includes("initial={{ opacity: 0 }}") &&
    hostSource.includes("animate={{ opacity: 1 }}") &&
    hostSource.includes("exit={{ opacity: 0 }}") &&
    hostSource.includes("duration: 0.2"),
  "transient pages should use one 200ms opacity-only transition",
);
assert.ok(
  !/\b(?:x|y|scale|filter|backdropFilter)\s*:/.test(hostSource),
  "the Motion host should not add movement, scale, or blur",
);

console.log("motion page transition contract ok");
```

- [ ] **Step 2: 运行测试并确认 RED**

Run: `node scripts/motion-page-transition.test.mjs`

Expected: FAIL，报错指向缺少 `src/app/TransientPageHost.tsx`；若文件读取顺序变化，也可先报 `framer-motion` 依赖不存在。失败必须来自尚未实现的新契约。

- [ ] **Step 3: 安装锁定版本的运行时依赖**

Run: `pnpm add framer-motion@^12.23.25`

Expected: `package.json` 的 `dependencies` 出现 `"framer-motion": "^12.23.25"`，`pnpm-lock.yaml` 同步更新，不修改其他依赖版本。

- [ ] **Step 4: 创建集中式 transient 宿主**

创建 `src/app/TransientPageHost.tsx`：

```tsx
import { AnimatePresence, motion, MotionConfig, useIsPresent } from "framer-motion";
import type { ReactNode } from "react";
import { PageActivityProvider } from "@/components/shell/PageActivity";
import type { AppPageId } from "@/lib/types/navigation";

declare module "react" {
  interface HTMLAttributes<T> {
    inert?: "" | undefined;
  }
}

export type TransientPageDescriptor = {
  pageId: AppPageId;
  instanceKey: string;
  node: ReactNode;
};

type TransientPageHostProps = {
  page: TransientPageDescriptor | null;
};

const transientPageTransition = {
  duration: 0.2,
};

function TransientPageLayer({ page }: { page: TransientPageDescriptor }) {
  const isPresent = useIsPresent();

  return (
    <motion.div
      className="app-page-transition-layer app-page-transition-overlay"
      data-page-transition-layer
      data-page-transition-kind="transient"
      data-page-transition-page-id={page.pageId}
      data-page-transition-state={isPresent ? "active" : "exiting"}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={transientPageTransition}
    >
      <PageActivityProvider active={isPresent}>
        <div
          aria-hidden={!isPresent}
          className="app-page-transition-content"
          inert={isPresent ? undefined : ""}
        >
          {page.node}
        </div>
      </PageActivityProvider>
    </motion.div>
  );
}

export function TransientPageHost({ page }: TransientPageHostProps) {
  return (
    <MotionConfig reducedMotion="user">
      <AnimatePresence initial={false} mode="wait">
        {page ? <TransientPageLayer key={page.instanceKey} page={page} /> : null}
      </AnimatePresence>
    </MotionConfig>
  );
}
```

外层 `motion.div` 不设置 `inert`，因此退出阶段仍是 main 区域的 pointer shield；只有内部内容进入 `inert`，避免按钮、表单和 Tab 焦点在淡出期间继续响应。

- [ ] **Step 5: 运行测试并确认 GREEN**

Run: `node scripts/motion-page-transition.test.mjs`

Expected: PASS，输出 `motion page transition contract ok`。

Run: `pnpm exec tsc --noEmit --pretty false`

Expected: 允许只出现当前工作区既有的 `src/features/stations/AddProviderPage.tsx:309` 空值类型错误；不得出现 `TransientPageHost.tsx` 或 `framer-motion` 相关错误。

- [ ] **Step 6: 精确暂存并提交宿主边界**

```powershell
git add -- package.json pnpm-lock.yaml scripts/motion-page-transition.test.mjs src/app/TransientPageHost.tsx
git diff --cached --name-only
git commit -m "feat: add motion transient page host"
```

Expected staged paths: exactly the four paths above.

### Task 2: 将 App 的手工退出状态机迁移到 Host

**Files:**
- Modify: `scripts/page-transition-container.test.mjs`
- Modify: `scripts/page-activation-refresh.test.mjs`
- Modify: `scripts/station-detail-transition-performance.test.mjs`
- Modify: `src/app/App.tsx`

- [ ] **Step 1: 将 container contract 改写为新架构的失败测试**

用以下内容替换 `scripts/page-transition-container.test.mjs`：

```js
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const appSource = await readFile("src/app/App.tsx", "utf8");
const hostSource = await readFile("src/app/TransientPageHost.tsx", "utf8");

assert.ok(
  appSource.includes('from "@/app/TransientPageHost"') &&
    appSource.includes("<TransientPageHost page={activeTransientPage} />"),
  "App should delegate transient rendering and presence cleanup to one host",
);
assert.equal(
  appSource.match(/<TransientPageHost\b/g)?.length ?? 0,
  1,
  "App should render exactly one transient host",
);

for (const legacyIdentifier of [
  "TRANSIENT_EXIT_TIMEOUT_MS",
  "RenderedTransientPage",
  "lastActiveTransientPageRef",
  "transientExitTimeoutRef",
  "exitingTransientPage",
  "pendingExitingTransientPage",
  "handleTransientExitComplete",
  "handleTransientExitAnimationEnd",
  "useLayoutEffect",
  "onAnimationEnd",
]) {
  assert.ok(
    !appSource.includes(legacyIdentifier),
    `App should remove the manual transient lifecycle: ${legacyIdentifier}`,
  );
}

assert.ok(
  appSource.includes('type ShellPageState = "active" | "background" | "inactive";') &&
    appSource.includes('isCurrentTransientPage ? "background" : "active"') &&
    appSource.includes('const active = shellPageState === "active";') &&
    appSource.includes('const inert = shellPageState !== "active";'),
  "shell pages should have explicit active, visible-background, and inactive states",
);
assert.ok(
  appSource.includes("<PageActivityProvider key={routeId} active={active}>") &&
    appSource.includes("data-page-transition-state={shellPageState}") &&
    appSource.includes('inert={inert ? "" : undefined}') &&
    appSource.includes("aria-hidden={inert}"),
  "background and inactive shells should remain mounted without refreshing or accepting focus",
);
assert.ok(
  appSource.includes("mountedRouteIds.has(activeShellRouteId)") &&
    appSource.includes("[...mountedRouteIds, activeShellRouteId]"),
  "a transient route should always have its retained parent shell rendered beneath it",
);
assert.ok(
  appSource.includes("const isReturningFromTransient") &&
    appSource.includes(
      'data-page-transition-handoff={isReturningFromTransient ? "transient-exit" : "none"}',
    ),
  "returning from a transient page should not retrigger the shell entry animation",
);

for (const instanceKey of [
  'instanceKey: "addProvider"',
  'instanceKey: `editProvider:${editingStationId ?? "edit-provider-empty"}`',
  'instanceKey: `stationDetail:${detailStationId ?? "station-detail-empty"}`',
  'instanceKey: `addKey:${initialKeyStationId ?? "add-key-unscoped"}`',
  'instanceKey: `editKey:${editingKeyId ?? "edit-key-empty"}`',
  'instanceKey: "modelBasePrices"',
]) {
  assert.ok(appSource.includes(instanceKey), `App should define stable identity: ${instanceKey}`);
}

assert.ok(
  hostSource.includes("key={page.instanceKey}") &&
    hostSource.includes('data-page-transition-state={isPresent ? "active" : "exiting"}'),
  "the host should use descriptor identity and Motion presence as its only exit state",
);

console.log("page transition container contract ok");
```

- [ ] **Step 2: 更新激活和详情实体契约**

在 `scripts/page-activation-refresh.test.mjs` 中只替换第一个 `assert.ok(...)` 为：

```js
assert.ok(
  appSource.includes("PageActivityProvider") &&
    appSource.includes('const active = shellPageState === "active";') &&
    appSource.includes('isCurrentTransientPage ? "background" : "active"') &&
    appSource.includes("<PageActivityProvider key={routeId} active={active}>"),
  "kept-alive shell pages should refresh only in active state, never while serving as a transient background",
);
```

在 `scripts/station-detail-transition-performance.test.mjs` 中只替换最后一个 `assert.ok(...)` 为：

```js
assert.ok(
  appSource.includes(
    'instanceKey: `stationDetail:${detailStationId ?? "station-detail-empty"}`',
  ),
  "rapidly opening another station should remount detail state through host-level entity identity",
);
```

- [ ] **Step 3: 运行三个测试并确认 RED**

Run: `node scripts/page-transition-container.test.mjs`

Expected: FAIL，指出 `App` 尚未导入/渲染 `TransientPageHost` 或仍存在手工生命周期标识。

Run: `node scripts/page-activation-refresh.test.mjs`

Expected: FAIL，指出 shell 尚未使用 `ShellPageState` 三态激活契约。

Run: `node scripts/station-detail-transition-performance.test.mjs`

Expected: FAIL，指出详情实体 identity 尚未迁移为 descriptor `instanceKey`。

- [ ] **Step 4: 精简 App imports、类型和生命周期 state**

将 `src/app/App.tsx` 顶部 React import 改为：

```tsx
import { useCallback, useEffect, useMemo, useState } from "react";
import { AppShell } from "@/components/shell/AppShell";
import { PageActivityProvider } from "@/components/shell/PageActivity";
import {
  TransientPageHost,
  type TransientPageDescriptor,
} from "@/app/TransientPageHost";
```

保留现有 `pageTransitionPolicy` 和业务页面 imports。删除 React module augmentation、`TRANSIENT_EXIT_TIMEOUT_MS`、`RenderedTransientPage`、三个 transient ref/state；在 `NavigationState` 后增加：

```tsx
type ShellPageState = "active" | "background" | "inactive";
```

- [ ] **Step 5: 让 transient renderer 返回带实体 identity 的 descriptor**

用以下实现替换 `renderTransientPage()`：

```tsx
  function renderTransientPage(): TransientPageDescriptor | null {
    switch (activeRouteId) {
      case "addProvider":
        return {
          pageId: "addProvider",
          instanceKey: "addProvider",
          node: (
            <AddProviderPage onBack={returnToStations} onCreated={returnToStations} />
          ),
        };
      case "editProvider":
        return {
          pageId: "editProvider",
          instanceKey: `editProvider:${editingStationId ?? "edit-provider-empty"}`,
          node: (
            <AddProviderPage
              stationId={editingStationId}
              onBack={returnToStations}
              onUpdated={returnToStations}
            />
          ),
        };
      case "stationDetail":
        return {
          pageId: "stationDetail",
          instanceKey: `stationDetail:${detailStationId ?? "station-detail-empty"}`,
          node: (
            <StationDetailPage
              stationId={detailStationId}
              initialStation={detailStationPreview}
              onBack={returnToStations}
              onEditProvider={openEditProvider}
            />
          ),
        };
      case "addKey":
        return {
          pageId: "addKey",
          instanceKey: `addKey:${initialKeyStationId ?? "add-key-unscoped"}`,
          node: (
            <AddKeyPage
              initialStationId={initialKeyStationId}
              onBack={returnToKeyPool}
              onCreated={returnToKeyPool}
            />
          ),
        };
      case "editKey":
        return {
          pageId: "editKey",
          instanceKey: `editKey:${editingKeyId ?? "edit-key-empty"}`,
          node: (
            <EditKeyPage
              stationKeyId={editingKeyId}
              onBack={returnToKeyPool}
              onUpdated={returnToKeyPool}
            />
          ),
        };
      case "modelBasePrices":
        return {
          pageId: "modelBasePrices",
          instanceKey: "modelBasePrices",
          node: <ModelBasePricesPage onBack={() => navigateTo("pricing")} />,
        };
      default:
        return null;
    }
  }
```

- [ ] **Step 6: 用 descriptor 和 shell 三态替换手工生命周期派生**

保留现有 `activeTransitionPolicy`，将其后的 transient 派生、所有相关 effects/handlers 和 `shellRouteIds` 替换为：

```tsx
  const activeTransitionPolicy = getPageTransitionPolicy(activeRouteId);
  const activeTransientPage = useMemo<TransientPageDescriptor | null>(() => {
    if (activeTransitionPolicy.kind !== "transient") {
      return null;
    }
    return renderTransientPage();
  }, [
    activeRouteId,
    activeTransitionPolicy.kind,
    detailStationId,
    detailStationPreview,
    editingKeyId,
    editingStationId,
    initialKeyStationId,
  ]);
  const isCurrentTransientPage = activeTransitionPolicy.kind === "transient";
  const previousTransitionPolicy = previousRouteId
    ? getPageTransitionPolicy(previousRouteId)
    : null;
  const isReturningFromTransient =
    activeTransitionPolicy.kind === "shell" && previousTransitionPolicy?.kind === "transient";
  const shellRouteIds = mountedRouteIds.has(activeShellRouteId)
    ? [...mountedRouteIds]
    : [...mountedRouteIds, activeShellRouteId];
```

- [ ] **Step 7: 用三态 shell 和单一 Host 替换 return 内的页面层**

保留 `<AppShell>` 和 stack 外壳，将 stack 内容替换为：

```tsx
        {shellRouteIds.map((routeId) => {
          const shellPageState: ShellPageState =
            routeId !== activeShellRouteId
              ? "inactive"
              : isCurrentTransientPage
                ? "background"
                : "active";
          const active = shellPageState === "active";
          const inert = shellPageState !== "active";

          return (
            <PageActivityProvider key={routeId} active={active}>
              <div
                aria-hidden={inert}
                className="app-page-transition-layer"
                data-page-transition-layer
                data-page-transition-kind="shell"
                data-page-transition-state={shellPageState}
                inert={inert ? "" : undefined}
              >
                {renderShellPage(routeId)}
              </div>
            </PageActivityProvider>
          );
        })}

        <TransientPageHost page={activeTransientPage} />
```

stack 外壳继续保留：

```tsx
      <div
        className="app-page-transition-stack"
        data-page-transition-handoff={isReturningFromTransient ? "transient-exit" : "none"}
      >
```

- [ ] **Step 8: 运行测试并确认 GREEN**

Run: `node scripts/page-transition-container.test.mjs`

Expected: PASS，输出 `page transition container contract ok`。

Run: `node scripts/page-activation-refresh.test.mjs`

Expected: PASS，无额外输出且 exit code 为 0。

Run: `node scripts/station-detail-transition-performance.test.mjs`

Expected: PASS，输出 `station detail transition performance contract ok`。

Run: `node scripts/motion-page-transition.test.mjs`

Expected: PASS，证明 App 迁移没有把 Motion import 扩散到业务层。

- [ ] **Step 9: 精确暂存并提交 App 生命周期迁移**

```powershell
git add -- scripts/page-transition-container.test.mjs scripts/page-activation-refresh.test.mjs scripts/station-detail-transition-performance.test.mjs src/app/App.tsx
git diff --cached --name-only
git commit -m "refactor: delegate transient lifecycle to motion"
```

Expected staged paths: exactly the four paths above.

### Task 3: 将 CSS 收敛为几何与交互状态契约

**Files:**
- Modify: `scripts/page-transition-styles.test.mjs`
- Modify: `src/styles.css`

- [ ] **Step 1: 用新样式契约替换旧 keyframe 测试**

用以下内容替换 `scripts/page-transition-styles.test.mjs`：

```js
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const stylesSource = await readFile("src/styles.css", "utf8");

function readRule(selector) {
  const ruleStart = stylesSource.indexOf(`${selector} {`);
  assert.notEqual(ruleStart, -1, `styles should define ${selector}`);
  const bodyStart = stylesSource.indexOf("{", ruleStart) + 1;
  const bodyEnd = stylesSource.indexOf("}", bodyStart);
  assert.notEqual(bodyEnd, -1, `styles should close ${selector}`);
  return stylesSource.slice(bodyStart, bodyEnd);
}

const backgroundRule = readRule(
  '.app-page-transition-layer[data-page-transition-state="background"]',
);
const inactiveRule = readRule(
  '.app-page-transition-layer[data-page-transition-state="inactive"]',
);
const overlayRule = readRule(".app-page-transition-overlay");

assert.ok(
  stylesSource.includes(".app-page-transition-stack") &&
    stylesSource.includes("[data-page-transition-layer]") &&
    stylesSource.includes("min-height: 100%"),
  "the transition stack and layers should preserve full-height page geometry",
);
assert.ok(
  backgroundRule.includes("display: block") &&
    backgroundRule.includes("visibility: visible") &&
    backgroundRule.includes("pointer-events: none"),
  "the parent shell should stay visible and geometrically stable beneath a transient page",
);
assert.ok(
  inactiveRule.includes("display: none") &&
    inactiveRule.includes("visibility: hidden") &&
    inactiveRule.includes("pointer-events: none"),
  "unrelated retained shell pages should remain mounted but absent from layout",
);
assert.ok(
  overlayRule.includes("pointer-events: auto") &&
    overlayRule.includes("will-change: opacity") &&
    stylesSource.includes(".app-page-transition-content"),
  "the Motion overlay should stay a pointer shield while only its content becomes inert",
);
assert.ok(
  !stylesSource.includes("relayTransientEnter") &&
    !stylesSource.includes("relayTransientExit"),
  "legacy transient CSS animations should be fully removed",
);
assert.ok(
  stylesSource.includes("relayPageFadeUp") &&
    stylesSource.includes('data-page-transition-handoff="transient-exit"') &&
    stylesSource.includes("animation: none"),
  "the existing shell animation should remain disabled during a transient return handoff",
);
assert.ok(
  stylesSource.includes("@media (prefers-reduced-motion: reduce)") &&
    stylesSource.includes(
      '.app-page-transition-layer[data-page-transition-kind="shell"]',
    ) &&
    stylesSource.includes("animation-duration: 1ms"),
  "CSS reduced-motion handling should remain scoped to the shell animation",
);

console.log("page transition styles contract ok");
```

- [ ] **Step 2: 运行测试并确认 RED**

Run: `node scripts/page-transition-styles.test.mjs`

Expected: FAIL，指出缺少 `background` shell 状态，或旧 `relayTransientEnter` / `relayTransientExit` 仍存在。

- [ ] **Step 3: 用静态状态样式替换 transition CSS block**

在 `src/styles.css` 中，用以下完整 block 替换从 `.app-page-transition-stack` 到该 reduced-motion media block 的现有内容：

```css
.app-page-transition-stack {
  position: relative;
  min-height: 100%;
  isolation: isolate;
}

.app-page-transition-layer,
[data-page-transition-layer] {
  min-height: 100%;
  min-width: 0;
}

.app-page-transition-layer[data-page-transition-state="inactive"] {
  display: none;
  pointer-events: none;
  visibility: hidden;
}

.app-page-transition-layer[data-page-transition-state="background"] {
  display: block;
  pointer-events: none;
  visibility: visible;
}

.app-page-transition-layer[data-page-transition-state="active"] {
  display: block;
  pointer-events: auto;
  visibility: visible;
}

.app-page-transition-stack[data-page-transition-handoff="none"]
  .app-page-transition-layer[data-page-transition-kind="shell"][data-page-transition-state="active"] {
  animation: relayPageFadeUp 160ms ease-out;
}

.app-page-transition-stack[data-page-transition-handoff="transient-exit"]
  .app-page-transition-layer[data-page-transition-kind="shell"][data-page-transition-state="active"] {
  animation: none;
}

.app-page-transition-overlay {
  position: absolute;
  inset: 0;
  z-index: 1;
  min-height: 100%;
  background: hsl(var(--background));
  pointer-events: auto;
  will-change: opacity;
}

.app-page-transition-content {
  min-height: 100%;
}

@keyframes relayPageFadeUp {
  from {
    opacity: 0;
    transform: translateY(4px);
  }

  to {
    opacity: 1;
    transform: translateY(0);
  }
}

@media (prefers-reduced-motion: reduce) {
  .app-page-transition-layer[data-page-transition-kind="shell"] {
    animation-duration: 1ms !important;
    transform: none !important;
  }
}
```

- [ ] **Step 4: 运行样式和完整聚焦 contract tests**

Run: `node scripts/page-transition-styles.test.mjs`

Expected: PASS，输出 `page transition styles contract ok`。

Run: `node scripts/page-transition-policy.test.mjs`

Expected: PASS，输出 `page transition policy contract ok`。

Run: `node scripts/page-transition-container.test.mjs`

Expected: PASS。

Run: `node scripts/page-activation-refresh.test.mjs`

Expected: PASS。

Run: `node scripts/station-detail-transition-performance.test.mjs`

Expected: PASS。

Run: `node scripts/model-base-prices-page.test.mjs`

Expected: PASS。

- [ ] **Step 5: 精确暂存并提交样式迁移**

```powershell
git add -- scripts/page-transition-styles.test.mjs src/styles.css
git diff --cached --name-only
git commit -m "style: preserve shell beneath transient fades"
```

Expected staged paths: exactly the two paths above.

### Task 4: 构建、浏览器验收与架构复核

**Files:**
- Verify only; any required correction returns to the owning file/task and is committed with exact paths.

- [ ] **Step 1: 运行全部自动化验证**

逐条运行：

```powershell
node scripts/motion-page-transition.test.mjs
node scripts/page-transition-policy.test.mjs
node scripts/page-transition-container.test.mjs
node scripts/page-transition-styles.test.mjs
node scripts/page-activation-refresh.test.mjs
node scripts/station-detail-transition-performance.test.mjs
node scripts/model-base-prices-page.test.mjs
pnpm exec vite build
pnpm exec tsc --noEmit --pretty false
```

Expected:

- 七个 contract scripts 全部 exit code 0。
- `vite build` exit code 0；既有 chunk-size warning 可以记录但不作为失败。
- `tsc --noEmit` 允许只保留当前基线中的 `src/features/stations/AddProviderPage.tsx:309` 空值类型错误；本任务涉及文件不得新增错误。

- [ ] **Step 2: 检查任务文件 diff 质量和架构边界**

逐条运行：

```powershell
git diff --check -- package.json pnpm-lock.yaml scripts/motion-page-transition.test.mjs scripts/page-transition-container.test.mjs scripts/page-transition-styles.test.mjs scripts/page-activation-refresh.test.mjs scripts/station-detail-transition-performance.test.mjs src/app/App.tsx src/app/TransientPageHost.tsx src/styles.css
rg -n "framer-motion" src
rg -n "TRANSIENT_EXIT_TIMEOUT_MS|lastActiveTransientPageRef|transientExitTimeoutRef|exitingTransientPage|relayTransientEnter|relayTransientExit" src
git status --short
git diff --cached --name-only
```

Expected:

- `git diff --check` 无输出。
- Framer Motion import 只命中 `src/app/TransientPageHost.tsx`。
- 旧生命周期和 transient keyframes 在 `src` 中零命中。
- 暂存区为空；工作区仍可包含用户原有的无关改动，不回滚、不暂存。

- [ ] **Step 3: 启动当前源码的独立 Vite QA 服务**

先检查端口：

```powershell
Get-NetTCPConnection -LocalPort 1430,5174 -ErrorAction SilentlyContinue
```

若 `1430` 已占用，运行：

```powershell
pnpm exec vite --host 127.0.0.1 --port 5174 --strictPort
```

Expected: 服务在 `http://127.0.0.1:5174` 可访问；若两个端口都占用，选择首个空闲端口并记录实际 URL。保持该进程运行直到浏览器验收结束。

- [ ] **Step 4: 用 Playwright/system Chrome 验收父页几何和 opening/closing continuity**

在 `中转站` 页执行：

1. 记录列表滚动容器 `scrollTop` 和第一行 `getBoundingClientRect().top`。
2. 点击第一条中转站，立即确认父 shell 为 `data-page-transition-state="background"`、computed `display: block`、`pointer-events: none`、`aria-hidden="true"` 且存在 `inert`。
3. 确认 transient overlay 在 200ms 内从透明到不透明，首帧已有 seed 详情内容，无空白帧、横向/纵向移动或 scale。
4. 点击详情返回，立即确认父 shell 恢复 `active`，旧 overlay 为 `exiting` 且仍是 main 区域 hit-test 的最上层；退出内容不可 Tab 聚焦。
5. 220ms 后确认 overlay 已卸载，并再次读取列表 `scrollTop` 与第一行 top；两者分别与打开前相同，top 允许不超过 1 CSS px 的取整误差。

Expected: 无闪动、无列表上下抖动、无退出点击穿透、控制台无 error/warning。

- [ ] **Step 5: 验收快速实体切换、全部 transient 入口和 reduced motion**

依次验证：

1. 中转站详情 A -> 返回 -> 详情 B：B 不显示 A 的标题、表单或旧请求结果，退出后无残留 overlay。
2. 中转站编辑、添加中转站、添加 Key、编辑 Key、模型基础价格：均使用相同 200ms opacity-only transition。
3. 子页直接打开另一个子页（详情 -> 编辑）：旧实例先退出，新实例再进入，期间父 shell 始终 background，最终仅一个 transient layer。
4. 模拟 `prefers-reduced-motion: reduce`：所有入口可正常打开/关闭，无位移、scale 或依赖动画回调的卡死。
5. 退出动画期间点击 main 区域：不得触发父列表行、按钮或重复导航；侧栏仍可用于主导航。

Expected: 每个时刻最多一个可交互内容层；不同实体由 `instanceKey` 隔离；快速导航最终与最后一次 `activeRouteId` 一致。

- [ ] **Step 6: 按可靠性、可扩展性、可维护性做最终复核**

复核结论必须逐项有证据：

- 可靠性：presence 由 Motion 管理，无 timer/event 双清理路径；父 shell DOM/scroll 不卸载；退出内容 inert，外层 overlay 保持 pointer shield；seed-first 详情测试和浏览器几何检查通过。
- 可扩展性：新增 transient 页面只需要在 `pageTransitionPolicy.ts` 注册父 shell，并在 `App.tsx` 生成带稳定 `instanceKey` 的 descriptor；业务页不接触 Motion。
- 可维护性：Framer import 单点；动画参数单点；App 不保存 outgoing ReactNode；CSS 只管理几何、可见性和 hit testing；旧 keyframes/state/ref/timer 零残留。

若浏览器验收发现问题，只修改上述责任边界内的文件并重跑本 Task 全部步骤；不通过延长动画、隐藏父 shell 或在业务页面增加局部动画绕过问题。

---

## Scope Guard

- 不修改主导航 shell 动画策略、业务 API、数据加载顺序、Tauri/Rust、数据库或页面文案。
- 不复制 CCSwitch 组件代码；只采用其 MIT 项目中成熟的 `AnimatePresence mode="wait"` 与 200ms opacity 生命周期方向。
- 不引入第二套兼容状态机；Motion Host 合入时旧 transient refs、timer、animation event 和 keyframes 必须一次删除。
- 不使用 `git add .`、`git add -A` 或 `git commit -a`；不暂存工作区中其他用户改动，不 push。
