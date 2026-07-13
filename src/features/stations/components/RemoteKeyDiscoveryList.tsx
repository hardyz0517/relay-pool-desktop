import { useEffect, useMemo, useState } from "react";
import { Link2 } from "lucide-react";
import { Button, SelectControl, StatusBadge, SwitchControl, type StatusTone } from "@/components/ui";
import { effectiveRateMultiplierForCredit } from "@/lib/formatters";
import type { RemoteKeyMatchStatus, RemoteStationKey, StationKey } from "@/lib/types/stationKeys";
import { cn } from "@/lib/utils";
import { formatMultiplier } from "../groupOptionViewModels";

type RemoteKeyDiscoveryListProps = {
  keys: RemoteStationKey[];
  localKeys: StationKey[];
  creditPerCny?: number;
  loading?: boolean;
  localKeyIdsCreatedByRemote?: Record<string, string>;
  onBind: (remoteKeyId: string, stationKeyId: string) => void;
  onLocalKeyToggle: (remoteKey: RemoteStationKey, checked: boolean) => void;
};

const matchStatusLabel: Record<RemoteKeyMatchStatus, string> = {
  matched: "已匹配",
  possible: "可能匹配",
  unbound: "未绑定",
};

const matchStatusTone: Record<RemoteKeyMatchStatus, StatusTone> = {
  matched: "healthy",
  possible: "warning",
  unbound: "disabled",
};

const selectClassName =
  "h-7 min-w-[150px] max-w-[190px] px-2 text-xs shadow-none";

export function RemoteKeyDiscoveryList({
  keys,
  localKeys,
  creditPerCny = 1,
  loading = false,
  localKeyIdsCreatedByRemote = {},
  onBind,
  onLocalKeyToggle,
}: RemoteKeyDiscoveryListProps) {
  const [selectedLocalKeyIds, setSelectedLocalKeyIds] = useState<Record<string, string>>({});

  const localKeyById = useMemo(
    () => new Map(localKeys.map((key) => [key.id, key] as const)),
    [localKeys],
  );
  const localKeyOptions = useMemo(
    () => [
      { value: "", label: "选择本地 Key", disabled: true },
      ...localKeys.map((key) => ({
        value: key.id,
        label: key.name,
        description: key.apiKeyMasked,
      })),
    ],
    [localKeys],
  );

  useEffect(() => {
    setSelectedLocalKeyIds((current) => {
      const nextEntries = Object.entries(current).filter(([, selectedId]) =>
        localKeyById.has(selectedId),
      );
      if (nextEntries.length === Object.keys(current).length) {
        return current;
      }
      return Object.fromEntries(nextEntries);
    });
  }, [localKeyById]);

  if (loading && keys.length === 0) {
    return <RemoteKeyEmptyState>正在获取远端 Key...</RemoteKeyEmptyState>;
  }

  if (keys.length === 0) {
    return <RemoteKeyEmptyState>暂无远端发现，先点击获取所有 Key。</RemoteKeyEmptyState>;
  }

  return (
    <div className="grid gap-2">
      <div className="overflow-x-auto">
        <div className="min-w-[960px]">
          <div className="grid h-7 grid-cols-[minmax(8rem,1fr)_5.5rem_minmax(8rem,1fr)_minmax(7rem,0.8fr)_5rem_minmax(8rem,1fr)_6.5rem_minmax(13rem,1.1fr)] items-center gap-2 border-b border-border px-1 text-[11px] font-medium text-muted-foreground">
            <span>远端名称</span>
            <span>状态</span>
            <span>密钥</span>
            <span>分组</span>
            <span>倍率</span>
            <span>本地匹配</span>
            <span>作为本地秘钥</span>
            <span className="text-right">绑定</span>
          </div>

          <div className="grid gap-1.5 py-2">
            {keys.map((key) => {
              const matchedLocalKey = key.matchedStationKeyId
                ? localKeyById.get(key.matchedStationKeyId) ?? null
                : null;
              const remoteCreatedLocalKeyId = localKeyIdsCreatedByRemote[key.id] ?? null;
              const hasLocalKey = Boolean(remoteCreatedLocalKeyId || matchedLocalKey);
              const selectedLocalKeyId = selectedLocalKeyIds[key.id];
              const effectiveSelectedLocalKeyId =
                selectedLocalKeyId && localKeyById.has(selectedLocalKeyId)
                  ? selectedLocalKeyId
                  : key.matchedStationKeyId && localKeyById.has(key.matchedStationKeyId)
                    ? key.matchedStationKeyId
                    : localKeys.length === 1
                      ? localKeys[0].id
                      : "";
              const canBind = key.matchStatus !== "matched";
              const bindDisabled = loading || !effectiveSelectedLocalKeyId;

              return (
                <div
                  key={key.id}
                  className="grid min-h-9 grid-cols-[minmax(8rem,1fr)_5.5rem_minmax(8rem,1fr)_minmax(7rem,0.8fr)_5rem_minmax(8rem,1fr)_6.5rem_minmax(13rem,1.1fr)] items-center gap-2 rounded-[var(--surface-radius)] px-1 text-xs text-slate-700"
                >
                  <span className="min-w-0 truncate font-medium text-slate-900">
                    {key.remoteKeyName?.trim() || key.remoteKeyIdHash || "未命名 Key"}
                  </span>
                  <StatusBadge tone={matchStatusTone[key.matchStatus]} className="h-5 px-1.5 text-[11px]">
                    {matchStatusLabel[key.matchStatus]}
                  </StatusBadge>
                  <span className="min-w-0 truncate font-mono text-[11px] text-slate-500">
                    {key.apiKeyMasked || key.apiKeyFingerprint || "未提供"}
                  </span>
                  <span className="min-w-0 truncate">{key.groupName || "默认分组"}</span>
                  <span className="tabular-nums">
                    {formatRemoteKeyRate(key.rateMultiplier, creditPerCny)}
                  </span>
                  <span
                    className={cn(
                      "min-w-0 truncate",
                      matchedLocalKey ? "text-slate-800" : "text-muted-foreground",
                    )}
                  >
                    {matchedLocalKey ? matchedLocalKey.name : key.matchStatus === "possible" ? "待确认" : "未绑定"}
                  </span>
                  <SwitchControl
                    ariaLabel={`${key.remoteKeyName ?? "远端 Key"} 作为本地秘钥`}
                    checked={hasLocalKey}
                    className="h-7 w-11 border-0 bg-transparent p-0 shadow-none"
                    disabled={loading}
                    showLabel={false}
                    onCheckedChange={() => onLocalKeyToggle(key, !hasLocalKey)}
                  />
                  <div className="flex min-w-0 justify-end gap-2">
                    {canBind ? (
                      localKeys.length > 0 ? (
                        <>
                          {localKeys.length > 1 ? (
                            <SelectControl
                              ariaLabel={`选择 ${key.remoteKeyName ?? "远端 Key"} 的本地 Key`}
                              className={selectClassName}
                              disabled={loading}
                              menuClassName="text-xs"
                              options={localKeyOptions}
                              value={effectiveSelectedLocalKeyId}
                              onChange={(stationKeyId) =>
                                setSelectedLocalKeyIds((current) => ({
                                  ...current,
                                  [key.id]: stationKeyId,
                                }))
                              }
                            />
                          ) : (
                            <span className="flex h-7 min-w-0 items-center truncate text-muted-foreground">
                              {localKeys[0].name}
                            </span>
                          )}
                          <Button
                            size="sm"
                            variant="outline"
                            disabled={bindDisabled}
                            onClick={() =>
                              effectiveSelectedLocalKeyId &&
                              onBind(key.id, effectiveSelectedLocalKeyId)
                            }
                          >
                            <Link2 className="h-3.5 w-3.5" />
                            绑定
                          </Button>
                        </>
                      ) : (
                        <span className="flex h-7 items-center text-muted-foreground">暂无本地 Key</span>
                      )
                    ) : (
                      <span className="flex h-7 items-center text-emerald-600">已关联</span>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      </div>
    </div>
  );
}

function formatRemoteKeyRate(rateMultiplier: number | null, creditPerCny: number) {
  const effectiveRate = effectiveRateMultiplierForCredit(rateMultiplier, creditPerCny);
  return effectiveRate === null ? "未采集" : `${formatMultiplier(effectiveRate)}x`;
}

function RemoteKeyEmptyState({ children }: { children: string }) {
  return (
    <div className="rounded-[var(--surface-radius)] border border-dashed border-border bg-slate-50 px-3 py-2 text-xs text-muted-foreground">
      {children}
    </div>
  );
}
