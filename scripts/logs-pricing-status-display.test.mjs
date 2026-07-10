import { readFile } from "node:fs/promises";

const logsSource = await readFile("src/features/logs/LogsPage.tsx", "utf8");

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

assert(
  logsSource.includes("pricingStatusLabel") &&
    logsSource.includes('"missing_model_price"') &&
    logsSource.includes('"unsupported_billing_mode"') &&
    logsSource.includes('"legacy_estimate"'),
  "logs page should map pricing cost statuses to explicit visible labels",
);

assert(
  logsSource.includes("缺模型基准价") &&
    logsSource.includes("不支持计费") &&
    logsSource.includes("未定价") &&
    logsSource.includes("旧估算"),
  "logs page should render distinct Chinese labels for missing/unpriced/legacy pricing states",
);

assert(
  !logsSource.includes('return log.costStatus === "unknown_usage" ? "未知" : "暂无";'),
  "logs cost formatting should not hide missing pricing states behind a generic placeholder",
);

assert(
  logsSource.includes("function CostCell") && logsSource.includes("pricingStatusTone"),
  "logs table should show both cost value and pricing status tone in the cost column",
);
