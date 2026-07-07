export function parseTimestampLikeDate(value: string) {
  const numeric = Number(value);
  return Number.isFinite(numeric) && numeric > 1000000000000 ? new Date(numeric) : new Date(value);
}

export function toTimestampMillis(value: string) {
  return parseTimestampLikeDate(value).getTime();
}
