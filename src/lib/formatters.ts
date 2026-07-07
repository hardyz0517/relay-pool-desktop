export function formatCompactMultiplier(value: number | null | undefined, fallback = "未采集") {
  if (value === null || value === undefined) return fallback;
  return Number.isInteger(value) ? String(value) : Number(value.toFixed(6)).toString();
}

export function formatRate(value: number | null | undefined, fallback = "未知") {
  if (value === null || value === undefined || !Number.isFinite(value)) return fallback;
  return `${Number(value.toFixed(3)).toString()}x`;
}

export function formatTrimmedDecimal(value: number, fractionDigits: number) {
  return value.toFixed(fractionDigits).replace(/0+$/, "").replace(/\.$/, "");
}
