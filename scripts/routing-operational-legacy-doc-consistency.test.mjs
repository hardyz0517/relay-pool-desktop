import { readFileSync } from "node:fs";

const checkedFiles = [
  "docs/PROJECT_PLAN.md",
  "docs/superpowers/specs/2026-07-30-routing-operational-unification-upgrade-spec.md",
  "docs/superpowers/plans/2026-07-30-routing-operational-unification-upgrade.md",
  "docs/superpowers/audits/routing-operational-deletion-ledger.md",
  "docs/superpowers/audits/routing-operational-boundary-manifest.json",
];

const stalePatterns = [
  {
    pattern: /`RELAY_POOL_PROXY_RUNTIME=legacy`\s*只按/,
    reason: "legacy runtime env switch must not be documented as an allowed process-start owner",
  },
  {
    pattern: /RELAY_POOL_PROXY_RUNTIME=legacy[^。\n]*(允许|回到上一完整|作为 process-start|process-start 级、debug-only 的完整旧 owner)/,
    reason: "legacy runtime env switch must not be documented as an allowed fallback",
  },
  {
    pattern: /debug-only legacy runtime[^。\n]*(均按目标规范执行|若仍保留|若仍在观察期|是完整隔离 owner|允许暂留)/,
    reason: "debug-only legacy runtime must be deleted, not conditionally retained",
  },
  {
    pattern: /process-start debug legacy runtime 若仍保留/,
    reason: "process-start debug legacy runtime must not remain as a conditional checklist item",
  },
  {
    pattern: /isolated debug legacy owner 调用/,
    reason: "request finalization compatibility must not depend on an isolated debug legacy owner",
  },
  {
    pattern: /若观察期仍需 debug legacy/,
    reason: "observation must not preserve a debug legacy runtime",
  },
  {
    pattern: /later debug legacy runtime deletion ticket/,
    reason: "Task 28 deletion has been applied and is no longer a later ticket",
  },
  {
    pattern: /debug runtime 的真实删除继续遵守/,
    reason: "debug runtime deletion is no longer deferred",
  },
  {
    pattern: /debug-only、process-start 级完整 legacy runtime/,
    reason: "the allowed-temporary list must not include a full debug legacy runtime",
  },
];

const allowedDeletionContext = /(已删除|删除|Deleted|deleted|forbid|forbidden|must not return|不得|禁止|不再|no longer|反回流|回流|not reintroduced|removed)/;

const failures = [];

for (const file of checkedFiles) {
  const lines = readFileSync(file, "utf8").split(/\r?\n/);

  lines.forEach((line, index) => {
    for (const { pattern, reason } of stalePatterns) {
      if (pattern.test(line) && !allowedDeletionContext.test(line)) {
        failures.push(`${file}:${index + 1}: ${reason}: ${line.trim()}`);
      }
    }
  });
}

if (failures.length > 0) {
  throw new Error(`routing legacy doc consistency failed:\n${failures.join("\n")}`);
}

console.log("routing operational legacy doc consistency ok");
