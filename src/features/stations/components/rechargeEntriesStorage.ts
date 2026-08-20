import type { RechargeEntry } from "./RechargeDialog";

const STORAGE_PREFIX = "relay-pool.recharge-entries.v1";

function storageKey(stationId: string): string {
  return `${STORAGE_PREFIX}:${stationId}`;
}

export function readRechargeEntries(stationId: string): RechargeEntry[] {
  if (typeof window === "undefined") return [];
  try {
    const raw = window.localStorage.getItem(storageKey(stationId));
    if (!raw) return [];
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    const seen = new Set<string>();
    return parsed.flatMap((value) => {
      if (!isRecord(value) || typeof value.url !== "string" || typeof value.label !== "string") return [];
      const url = sanitizeRechargeUrl(value.url);
      if (!url) return [];
      if (seen.has(url)) return [];
      seen.add(url);
      const provider = value.provider === "liandong" || value.provider === "cloudcat" ? value.provider : "custom";
      const source = value.source === "manual" ? "manual" : "confirmed";
      const paymentMethods = Array.isArray(value.paymentMethods)
        ? value.paymentMethods.filter((item): item is string => typeof item === "string")
        : [];
      return [{
        url,
        label: value.label.trim() || "充值入口",
        provider,
        paymentMethods,
        source,
        note: typeof value.note === "string" ? value.note.trim() : undefined,
      } satisfies RechargeEntry];
    });
  } catch {
    return [];
  }
}

export function writeRechargeEntries(stationId: string, entries: RechargeEntry[]): void {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(storageKey(stationId), JSON.stringify(entries));
  } catch {
    // Local storage is a convenience cache; the in-memory state remains usable.
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function sanitizeRechargeUrl(value: string): string | null {
  try {
    const url = new URL(value);
    if (!/^https?:$/.test(url.protocol) || url.username || url.password) return null;
    for (const key of [...url.searchParams.keys()]) {
      if (/^(?:token|access[_-]?token|refresh[_-]?token|auth(?:orization)?|session(?:[_-]?id)?|cookie|password|secret|code)$/i.test(key)) url.searchParams.delete(key);
    }
    return url.toString();
  } catch {
    return null;
  }
}
