export function formatRecentRequestCost(
  value: number | null | undefined,
  currency?: string | null,
  costStatus?: string | null,
) {
  return formatCurrencyCost(value, currency, costStatus, formatRecentCostValue);
}

function formatCurrencyCost(
  value: number | null | undefined,
  currency: string | null | undefined,
  costStatus: string | null | undefined,
  formatValue: (value: number) => string,
) {
  if (value == null && costStatus === "usage_only") {
    return "未定价";
  }
  if (value == null) {
    return "-";
  }
  const symbol = currencySymbol(currency ?? "USD") || "$";
  const formattedValue = formatValue(value);
  if (formattedValue.startsWith("< ")) {
    return `< ${symbol}${formattedValue.slice(2)}`;
  }
  return `${symbol}${formattedValue}`;
}

function formatRecentCostValue(value: number) {
  if (!Number.isFinite(value)) {
    return "0.0000";
  }
  const absValue = Math.abs(value);
  if (absValue > 0 && absValue < 0.0001) {
    return "< 0.0001";
  }
  return value.toFixed(4);
}

function currencySymbol(currency?: string) {
  if (currency?.toUpperCase() === "USD") return "$";
  if (currency?.toUpperCase() === "CNY") return "¥";
  return "";
}
