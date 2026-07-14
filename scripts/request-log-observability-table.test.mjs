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
  pageSource.includes("<RequestLogTable") &&
    dashboardSource.includes("最近使用") &&
    dashboardSource.includes("requestLogs.slice(0, 5)"),
  "logs page should use the new table without changing dashboard recent usage",
);

assert.ok(
  viewModelSource.includes("formatGroupName") &&
    viewModelSource.includes("key.groupName") &&
    tableSource.includes("formatGroupName(row, keyById)"),
  "request log group cells should prefer the current key's readable group name",
);

assert.ok(
  viewModelSource.includes('none: "None"') &&
    viewModelSource.includes('minimal: "Minimal"') &&
    viewModelSource.includes('low: "Low"') &&
    viewModelSource.includes('medium: "Medium"') &&
    viewModelSource.includes('high: "High"') &&
    viewModelSource.includes('xhigh: "XHigh"') &&
    viewModelSource.includes('max: "Max"'),
  "request log reasoning effort values should use English level labels",
);

assert.ok(
  tableSource.includes('from "lucide-react"') &&
    tableSource.includes("ArrowDown") &&
    tableSource.includes("ArrowUp") &&
    tableSource.includes("Database") &&
    tableSource.includes("<TokenUsageCell log={row}") &&
    tableSource.includes("<LatencyCell log={row}"),
  "request log token and latency columns should use dedicated compact visual cells",
);

assert.ok(
  tableSource.includes("bg-emerald-400") &&
    /h-9[^\"]*w-1|w-1[^\"]*h-9/.test(tableSource),
  "request log latency cell should include a stable teal vertical timing bar",
);

assert.equal(
  tableSource.match(/<LogMetaTag value=/g)?.length,
  3,
  "request log group, type, and billing cells should use metadata tags",
);

assert.ok(
  tableSource.includes("function LogMetaTag") &&
    tableSource.includes("h-5 max-w-full") &&
    tableSource.includes("rounded-[4px]") &&
    tableSource.includes("bg-blue-50") &&
    tableSource.includes("text-blue-700") &&
    tableSource.includes('className="truncate"') &&
    tableSource.includes("title={value}"),
  "request log metadata tags should match the compact light-blue reference style",
);

assert.ok(
  tableSource.includes('key: "endpoint"') &&
    tableSource.includes("render: (row) => row.path") &&
    !tableSource.includes("`${row.method} ${row.path}`"),
  "request log endpoints should omit the HTTP method",
);

assert.ok(
  tableSource.includes('row.stream ? "流式" : "同步"') &&
    !tableSource.includes('row.stream ? "流式" : "非流式"'),
  "request log type should distinguish streaming from synchronous requests",
);

assert.ok(
  viewModelSource.includes('token: "按量"') &&
    viewModelSource.includes('per_request: "按次"') &&
    viewModelSource.includes('image: "按量"') &&
    viewModelSource.includes('video: "按量"'),
  "request log billing modes should use the compact usage or request labels",
);

assert.ok(
  viewModelSource.includes('return `$${log.estimatedTotalCost.toFixed(6)}`') &&
    !tableSource.includes("pricingStatusLabel") &&
    !tableSource.includes("pricingStatusTone") &&
    !tableSource.includes("<StatusBadge"),
  "request log costs should use a single dollar amount without a pricing badge",
);

assert.ok(
  !tableSource.includes('key: "status"') && !tableSource.includes('header: "状态"'),
  "request log table should not include a status column",
);

assert.ok(
  tableSource.includes("formatRequestTokenCount(log, log.promptTokens)") &&
    tableSource.includes("formatRequestTokenCount(log, log.completionTokens)") &&
    viewModelSource.includes('log.status === "failed"') &&
    viewModelSource.includes('return "0"'),
  "failed request records should display zero input and output tokens",
);

const inScopeSource = `${tableSource}\n${viewModelSource}\n${typeSource}`;
assert.ok(
  !/clientIp|client_ip|remoteAddr|remote_addr|header:\s*["']IP["']/i.test(inScopeSource),
  "request log observability must not collect or display IP addresses",
);
