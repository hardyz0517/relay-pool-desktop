export function formatRequestCost(
  value: number | null | undefined,
  currency?: string | null,
  costStatus?: string | null,
) {
  if (value == null && costStatus === "usage_only") {
    return "未定价";
  }
  if (value == null) {
    return "-";
  }
  const symbol = currencySymbol(currency ?? "USD") || "$";
  const formattedValue = formatCostValue(value);
  if (formattedValue.startsWith("< ")) {
    return `< ${symbol}${formattedValue.slice(2)}`;
  }
  return `${symbol}${formattedValue}`;
}

export function requestBaseCostValue(request: {
  estimatedTotalCost: number | null;
  baseTotalCost: number | null;
  costStatus: string | null;
}) {
  if (request.baseTotalCost != null) {
    return request.baseTotalCost;
  }
  if (request.costStatus === "base_price_only") {
    return request.estimatedTotalCost;
  }
  return null;
}

function formatCostValue(value: number) {
  if (!Number.isFinite(value)) {
    return "0.0000";
  }
  const absValue = Math.abs(value);
  if (absValue > 0 && absValue < 0.00000001) {
    return "< 0.00000001";
  }
  if (absValue > 0 && absValue < 0.0001) {
    return trimTrailingZeros(value.toFixed(8));
  }
  return value.toFixed(4);
}

function trimTrailingZeros(value: string) {
  return value.replace(/(\.\d*?[1-9])0+$/, "$1");
}

function currencySymbol(currency?: string) {
  if (currency?.toUpperCase() === "USD") return "$";
  if (currency?.toUpperCase() === "CNY") return "¥";
  return "";
}
