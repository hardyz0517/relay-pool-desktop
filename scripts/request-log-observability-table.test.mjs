import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

async function readOptional(path) {
  try {
    return await readFile(path, "utf8");
  } catch (error) {
    if (error?.code === "ENOENT") return "";
    throw error;
  }
}

const [pageSource, tableSource, viewModelSource, typeSource, dashboardSource] = await Promise.all([
  readFile("src/features/logs/LogsPage.tsx", "utf8"),
  readOptional("src/features/logs/RequestLogTable.tsx"),
  readOptional("src/features/logs/requestLogViewModels.ts"),
  readFile("src/lib/types/proxy.ts", "utf8"),
  readFile("src/features/dashboard/DashboardPage.tsx", "utf8"),
]);

for (const label of [
  "密钥",
  "模型",
  "推理强度",
  "端点",
  "分组",
  "类型",
  "计费模式",
  "Token",
  "费用",
  "延迟",
  "状态",
  "时间",
]) {
  assert.ok(tableSource.includes(`header: "${label}"`), `request log table should include ${label}`);
}

assert.ok(
  tableSource.includes("overflow-x-auto") && tableSource.includes("min-w-[1320px]"),
  "wide request log table should scroll instead of compressing columns",
);

assert.ok(
  viewModelSource.includes("缓存读") &&
    viewModelSource.includes("缓存写") &&
    viewModelSource.includes("首字") &&
    viewModelSource.includes("总耗时") &&
    viewModelSource.includes("推理强度"),
  "request log view model should expose structured token, latency, and reasoning labels",
);

assert.ok(
  typeSource.includes("reasoningEffort: string | null") &&
    typeSource.includes("cacheCreationTokens: number | null") &&
    typeSource.includes("cacheReadTokens: number | null") &&
    typeSource.includes("firstTokenMs: number | null") &&
    typeSource.includes("billingMode: string | null"),
  "frontend request log contract should include all observability fields",
);

assert.ok(
  pageSource.includes("<RequestLogTable") && dashboardSource.includes('title="最近使用"'),
  "logs page should use the new table without changing dashboard recent usage",
);

const inScopeSource = `${tableSource}\n${viewModelSource}\n${typeSource}`;
assert.ok(
  !/clientIp|client_ip|remoteAddr|remote_addr|header:\s*["']IP["']/i.test(inScopeSource),
  "request log observability must not collect or display IP addresses",
);
