import { useMemo } from "react";
import { CircleMinus, Download, Trash2 } from "lucide-react";
import { IconButton, StatusBadge } from "@/components/ui";
import { effectiveRateMultiplierForCredit } from "@/lib/formatters";
import type { RemoteStationKey, StationKey } from "@/lib/types/stationKeys";
import { cn } from "@/lib/utils";
import { formatMultiplier } from "@/lib/groupOptionViewModels";

type RemoteKeyDiscoveryListProps = {
  keys: RemoteStationKey[];
  localKeys: StationKey[];
  creditPerCny?: number;
  loading?: boolean;
  readOnly?: boolean;
  deleteDisabled?: boolean;
  localKeyIdsCreatedByRemote?: Record<string, string>;
  onDelete: (remoteKey: RemoteStationKey) => void;
  onDeleteImportedLocalKey: (remoteKey: RemoteStationKey) => void;
  onImport: (remoteKey: RemoteStationKey) => void;
};
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
  onDelete,
  onDeleteImportedLocalKey,
  onImport,
}: RemoteKeyDiscoveryListProps) {
  const localKeyById = useMemo(
    () => new Map(localKeys.map((key) => [key.id, key] as const)),
    [localKeys],
  );

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
              const identityVerified = Boolean(key.apiKeyFingerprint);
              const remoteCreatedLocalKeyId = localKeyIdsCreatedByRemote[key.id] ?? null;
              const hasImportedLocalKey = Boolean(
                remoteCreatedLocalKeyId && localKeyById.has(remoteCreatedLocalKeyId),
              );

              return (
                <div
                  key={key.id}
                  className={cn("grid min-h-9 items-center gap-2 rounded-[var(--surface-radius)] px-1 text-xs text-foreground", remoteKeyGridClassName)}
                >
                  <span className="min-w-0 truncate font-medium text-foreground">
                    {key.remoteKeyName?.trim() || key.remoteKeyIdHash || "未命名 Key"}
                  </span>
                  <StatusBadge
                    tone={isMatched ? "healthy" : identityVerified ? "disabled" : "warning"}
                    className="h-5 px-1.5 text-[11px]"
                  >
                    {isMatched ? "已匹配" : identityVerified ? "无匹配" : "未验证"}
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
                    ) : (
                      <span className="w-full truncate text-center text-muted-foreground">
                        {identityVerified ? "无对应本地 Key" : "无法校验密钥"}
                      </span>
                    )}
                  </div>
                  <div className="flex min-w-0 justify-center">
                    {hasImportedLocalKey ? (
                      <StatusBadge tone="healthy" className="h-5 px-1.5 text-[11px]">已导入</StatusBadge>
                    ) : isMatched ? (
                      <StatusBadge tone="info" className="h-5 px-1.5 text-[11px]">已存在</StatusBadge>
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
