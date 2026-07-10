import { readFile } from "node:fs/promises";

const logsSource = await readFile("src/features/logs/LogsPage.tsx", "utf8");
const tableSource = await readFile("src/features/logs/RequestLogTable.tsx", "utf8");
const viewModelSource = await readFile("src/features/logs/requestLogViewModels.ts", "utf8");
const presentationSource = `${logsSource}\n${tableSource}\n${viewModelSource}`;

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

assert(
  presentationSource.includes("pricingStatusLabel") &&
    presentationSource.includes('"missing_model_price"') &&
    presentationSource.includes('"unsupported_billing_mode"') &&
    presentationSource.includes('"legacy_estimate"'),
  "logs page should map pricing cost statuses to explicit visible labels",
);

assert(
  presentationSource.includes("缺模型基准价") &&
    presentationSource.includes("不支持计费") &&
    presentationSource.includes("未定价") &&
    presentationSource.includes("旧估算"),
  "logs page should render distinct Chinese labels for missing/unpriced/legacy pricing states",
);

assert(
  !presentationSource.includes('return log.costStatus === "unknown_usage" ? "未知" : "暂无";'),
  "logs cost formatting should not hide missing pricing states behind a generic placeholder",
);

assert(
  tableSource.includes("formatRequestCost") && tableSource.includes("pricingStatusTone"),
  "logs table should show both cost value and pricing status tone in the cost column",
);
