# Request Log Meta Tags Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Render the request-log group, type, and billing mode as compact light-blue metadata tags.

**Architecture:** Add one local presentational component to `RequestLogTable.tsx` and wrap the three existing display values. Keep all view-model and backend behavior unchanged.

**Tech Stack:** React, TypeScript, Tailwind CSS, Node.js source-contract test, Vite

---

### Task 1: Add Metadata Tags

**Files:**
- Modify: `scripts/request-log-observability-table.test.mjs`
- Modify: `src/features/logs/RequestLogTable.tsx`

- [x] **Step 1: Write the failing contract test**

Require `LogMetaTag`, three `<LogMetaTag value={...} />` renderers, and these stable classes:

```text
h-5 max-w-full rounded-[4px] bg-blue-50 px-2 text-blue-700
```

- [x] **Step 2: Verify RED**

Run: `node scripts/request-log-observability-table.test.mjs`

Expected: FAIL because the three columns still render plain strings.

- [x] **Step 3: Implement the local component**

Add:

```tsx
function LogMetaTag({ value }: { value: string }) {
  return (
    <span
      className="inline-flex h-5 max-w-full items-center rounded-[4px] bg-blue-50 px-2 text-xs font-medium text-blue-700"
      title={value}
    >
      <span className="truncate">{value}</span>
    </span>
  );
}
```

Wrap `formatGroupName(row, keyById)`, `row.stream ? "流式" : "非流式"`, and `billingModeLabel(row.billingMode)` with this component.

- [x] **Step 4: Verify GREEN**

Run: `node scripts/request-log-observability-table.test.mjs`

Run: `node scripts/logs-pricing-status-display.test.mjs`

Run: `pnpm.cmd build`

Expected: all commands exit successfully.

- [x] **Step 5: Verify the live desktop layout**

Reload the current-source desktop app, open 请求日志, and confirm all three columns use compact light-blue tags without changing row height or overlapping adjacent columns.
