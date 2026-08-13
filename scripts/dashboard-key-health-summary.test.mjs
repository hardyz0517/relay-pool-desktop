import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");
const summarySource = await readFile("src/features/dashboard/dashboardKeyHealth.ts", "utf8");
const registrySource = await readFile("src/app/shellPageRegistry.tsx", "utf8");

assert.match(
  dashboardSource,
  /const dashboardKeyHealthStatuses:[\s\S]*?"unchecked"[\s\S]*?"healthy"[\s\S]*?"warning"[\s\S]*?"error"/,
  "dashboard health summary should list only actionable health states",
);
assert.doesNotMatch(dashboardSource, /dashboardKeyHealthLabels[\s\S]*?disabled:\s*"禁用"/);
assert.doesNotMatch(summarySource, /summary\.disabled/);
assert.match(
  dashboardSource,
  /<section className="grid min-w-0 gap-3">\s*<h2[^>]*>\s*密钥健康\s*<\/h2>\s*<div className="flex min-w-0 flex-wrap items-center/,
  "key health title should sit outside its summary card",
);
assert.match(
  dashboardSource,
  /disabledKeyCount > 0[\s\S]*?启用 · \$\{disabledKeyCount\} 禁用[\s\S]*?启用/,
  "available key card should own the disabled-key count",
);
assert.match(
  registrySource,
  /<DashboardPage[\s\S]*?onOpenKeyPool=\{actions\.openKeyPool\}[\s\S]*?onOpenLocalRouting=\{actions\.openLocalRouting\}[\s\S]*?onOpenRequestLogs=\{actions\.openRequestLogs\}/,
);
