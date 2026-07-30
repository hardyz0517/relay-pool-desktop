import { useEffect, useMemo, useState } from "react";
import { CircleMinus, Download, Link2, Trash2, Unlink } from "lucide-react";
import { IconButton, SelectControl, StatusBadge, type StatusTone } from "@/components/ui";
import { effectiveRateMultiplierForCredit } from "@/lib/formatters";
import type { RemoteKeyMatchStatus, RemoteStationKey, StationKey } from "@/lib/types/stationKeys";
import { cn } from "@/lib/utils";
import { formatMultiplier } from "@/lib/groupOptionViewModels";
import { bindableLocalKeysForRemote } from "../pages/add-provider/keyGroupModel";

type RemoteKeyDiscoveryListProps = {
  keys: RemoteStationKey[];
  localKeys: StationKey[];
  creditPerCny?: number;
  loading?: boolean;
  readOnly?: boolean;
  deleteDisabled?: boolean;
  localKeyIdsCreatedByRemote?: Record<string, string>;
  onBind: (remoteKeyId: string, stationKeyId: string) => void;
  onDelete: (remoteKey: RemoteStationKey) => void;
  onDeleteImportedLocalKey: (remoteKey: RemoteStationKey) => void;
  onImport: (remoteKey: RemoteStationKey) => void;
  onUnbind: (remoteKey: RemoteStationKey) => void;
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
  "h-7 min-w-0 w-full px-2 text-xs shadow-none";
const remoteKeyGridClassName =
  "grid-cols-[minmax(7rem,1.2fr)_5.5rem_minmax(7rem,0.8fr)_6rem_4.5rem_9rem_6rem_7rem]";

export function RemoteKeyDiscoveryList({
  keys,
  localKeys,
  creditPerCny = 1,
  loading = false,
  readOnly = false,
  deleteDisabled = false,
  localKeyIdsCreatedByRemote = {},
  onBind,
  onDelete,
  onDeleteImportedLocalKey,
  onImport,
  onUnbind,
}: RemoteKeyDiscoveryListProps) {
  const [selectedLocalKeyIds, setSelectedLocalKeyIds] = useState<Record<string, string>>({});

  const localKeyById = useMemo(
    () => new Map(localKeys.map((key) => [key.id, key] as const)),
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
        <div className="min-w-[840px]">
          <div className={cn("grid h-7 items-center gap-2 border-b border-border px-1 text-[11px] font-medium text-muted-foreground", remoteKeyGridClassName)}>
            <span>远端名称</span>
            <span>状态</span>
            <span>密钥</span>
            <span>分组</span>
            <span>倍率</span>
            <span className="text-center">本地匹配</span>
            <span className="text-center">Key 池</span>
            <span className="text-center">操作</span>
          </div>

          <div className="grid gap-1.5 py-2">
            {keys.map((key) => {
              const matchedLocalKey = key.matchedStationKeyId
                ? localKeyById.get(key.matchedStationKeyId) ?? null
                : null;
              const isMatched = key.matchStatus === "matched" && Boolean(matchedLocalKey);
              const effectiveMatchStatus: RemoteKeyMatchStatus = isMatched
                ? "matched"
                : key.matchStatus === "possible"
                  ? "possible"
                  : "unbound";
              const remoteCreatedLocalKeyId = localKeyIdsCreatedByRemote[key.id] ?? null;
              const hasImportedLocalKey = Boolean(
                remoteCreatedLocalKeyId && localKeyById.has(remoteCreatedLocalKeyId),
              );
              const hasLocalKeyRelation = Boolean(key.matchedStationKeyId);
              const bindableLocalKeys = bindableLocalKeysForRemote(key.id, keys, localKeys);
              const localKeyOptions = [
                { value: "", label: "选择本地 Key", disabled: true },
                ...bindableLocalKeys.map((localKey) => ({
                  value: localKey.id,
                  label: localKey.name,
                  description: localKey.apiKeyMasked,
                })),
              ];
              const selectedLocalKeyId = selectedLocalKeyIds[key.id];
              const effectiveSelectedLocalKeyId =
                selectedLocalKeyId && localKeyById.has(selectedLocalKeyId)
                  ? selectedLocalKeyId
                  : remoteCreatedLocalKeyId && localKeyById.has(remoteCreatedLocalKeyId)
                    ? remoteCreatedLocalKeyId
                  : key.matchedStationKeyId && localKeyById.has(key.matchedStationKeyId)
                    ? key.matchedStationKeyId
                    : bindableLocalKeys.length === 1
                      ? bindableLocalKeys[0].id
                      : "";
              const bindDisabled = loading || readOnly || !effectiveSelectedLocalKeyId;

              return (
                <div
                  key={key.id}
                  className={cn("grid min-h-9 items-center gap-2 rounded-[var(--surface-radius)] px-1 text-xs text-foreground", remoteKeyGridClassName)}
                >
                  <span className="min-w-0 truncate font-medium text-foreground">
                    {key.remoteKeyName?.trim() || key.remoteKeyIdHash || "未命名 Key"}
                  </span>
                  <StatusBadge tone={matchStatusTone[effectiveMatchStatus]} className="h-5 px-1.5 text-[11px]">
                    {matchStatusLabel[effectiveMatchStatus]}
                  </StatusBadge>
                  <span className="min-w-0 truncate font-mono text-[11px] text-muted-foreground">
                    {key.apiKeyMasked || key.apiKeyFingerprint || "未提供"}
                  </span>
                  <span className="min-w-0 truncate">{key.groupName || "默认分组"}</span>
                  <span className="tabular-nums">
                    {formatRemoteKeyRate(key.rateMultiplier, creditPerCny)}
                  </span>
                  <div className="flex min-w-0 items-center gap-1">
                    {isMatched && matchedLocalKey ? (
                      <span className="min-w-0 flex-1 truncate text-center text-foreground">
                        {matchedLocalKey.name}
                      </span>
                    ) : bindableLocalKeys.length > 0 ? (
                      <>
                        {bindableLocalKeys.length > 1 ? (
                          <SelectControl
                            ariaLabel={`选择 ${key.remoteKeyName ?? "远端 Key"} 的本地 Key`}
                            className={selectClassName}
                            disabled={loading || readOnly}
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
                          <span className="min-w-0 flex-1 truncate text-muted-foreground">
                            {bindableLocalKeys[0].name}
                          </span>
                        )}
                        <IconButton
                          className="h-7 w-7 shrink-0 text-muted-foreground"
                          disabled={bindDisabled}
                          label={`绑定 ${key.remoteKeyName ?? "远端 Key"} 到本地 Key`}
                          onClick={() =>
                            effectiveSelectedLocalKeyId &&
                            onBind(key.id, effectiveSelectedLocalKeyId)
                          }
                        >
                          <Link2 className="h-3.5 w-3.5" />
                        </IconButton>
                      </>
                    ) : (
                      <span className="w-full truncate text-center text-muted-foreground">
                        {localKeys.length > 0 ? "无可用本地 Key" : "暂无本地 Key"}
                      </span>
                    )}
                  </div>
                  <div className="flex min-w-0 justify-center">
                    {hasImportedLocalKey ? (
                      <StatusBadge tone="healthy" className="h-5 px-1.5 text-[11px]">已导入</StatusBadge>
                    ) : isMatched ? (
                      <StatusBadge tone="info" className="h-5 px-1.5 text-[11px]">已关联</StatusBadge>
                    ) : hasLocalKeyRelation ? (
                      <StatusBadge tone="warning" className="h-5 px-1.5 text-[11px]">待确认</StatusBadge>
                    ) : (
                      <IconButton
                        className="h-7 w-7 text-muted-foreground"
                        disabled={loading || readOnly}
                        label={`导入 ${key.remoteKeyName ?? "远端 Key"} 到 Key 池`}
                        onClick={() => onImport(key)}
                      >
                        <Download className="h-3.5 w-3.5" />
                      </IconButton>
                    )}
                  </div>
                  <div className="flex min-w-0 justify-center gap-0.5">
                    {key.matchedStationKeyId && (
                      <IconButton
                        className="h-7 w-7 text-muted-foreground"
                        disabled={loading || readOnly}
                        label={`解除 ${key.remoteKeyName ?? "远端 Key"} 的本地关联`}
                        onClick={() => onUnbind(key)}
                      >
                        <Unlink className="h-3.5 w-3.5" />
                      </IconButton>
                    )}
                    {hasImportedLocalKey && (
                      <IconButton
                        className="h-7 w-7 text-muted-foreground hover:bg-danger-surface hover:text-danger-foreground"
                        disabled={loading || readOnly}
                        label={`从 Key 池移除 ${key.remoteKeyName ?? "远端 Key"}`}
                        onClick={() => onDeleteImportedLocalKey(key)}
                      >
                        <CircleMinus className="h-3.5 w-3.5" />
                      </IconButton>
                    )}
                    <IconButton
                      className="h-7 w-7 shrink-0 text-muted-foreground hover:bg-danger-surface hover:text-danger-foreground"
                      disabled={loading || readOnly || deleteDisabled}
                      label={`删除远端 Key ${key.remoteKeyName?.trim() || key.remoteKeyIdHash || "未命名"}`}
                      onClick={() => onDelete(key)}
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </IconButton>
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
    <div className="rounded-[var(--surface-radius)] border border-dashed border-border bg-surface-subtle px-3 py-2 text-xs text-muted-foreground">
      {children}
    </div>
  );
}
