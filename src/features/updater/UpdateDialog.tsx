import { AlertTriangle, Download, Loader2, RotateCcw } from "lucide-react";
import { Button } from "@/components/ui";
import type { UpdaterState } from "./updateState";

type UpdateDialogProps = {
  open: boolean;
  state: UpdaterState;
  onDismiss: () => void;
  onInstall: () => void;
  onRetry: () => void;
};

export function UpdateDialog({
  open,
  state,
  onDismiss,
  onInstall,
  onRetry,
}: UpdateDialogProps) {
  if (!open) return null;
  const busy = state.phase === "downloading" || state.phase === "cleaning" || state.phase === "installing";
  const checkFailed = state.phase === "failed" && state.failedOperation === "check";
  const showReleaseDetails = state.phase === "available" || busy ||
    (state.phase === "failed" && !checkFailed);
  const percent = state.totalBytes && state.totalBytes > 0
    ? Math.min(100, Math.round((state.downloadedBytes / state.totalBytes) * 100))
    : null;

  return (
    <div className="fixed inset-0 z-[80] flex items-center justify-center bg-slate-900/30 p-4 backdrop-blur-[2px]">
      <div className="w-full max-w-lg overflow-hidden rounded-[var(--surface-radius)] border border-border bg-white shadow-[0_24px_70px_rgba(15,23,42,0.18)]">
        <div className="border-b border-border px-5 py-4">
          <div className="text-[15px] font-semibold text-slate-900">
            {state.phase === "checking"
              ? "正在检查更新"
              : checkFailed
                ? "更新检查未完成"
                : state.phase === "failed"
                  ? "更新未完成"
                  : "发现新版本"}
          </div>
          {showReleaseDetails && (
            <div className="mt-1 text-xs text-muted-foreground">
              {state.currentVersion} → {state.version ?? "新版本"}
            </div>
          )}
        </div>

        <div className="grid gap-4 px-5 py-5">
          {state.phase === "checking" ? (
            <div className="text-sm leading-6 text-slate-600" aria-live="polite">
              正在读取更新清单…
            </div>
          ) : checkFailed ? (
            <div className="text-sm leading-6 text-rose-700">{state.error}</div>
          ) : showReleaseDetails ? (
            <>
              {state.notes ? (
                <div className="max-h-40 overflow-auto whitespace-pre-wrap text-sm leading-6 text-slate-600">
                  {state.notes}
                </div>
              ) : (
                <div className="text-sm text-slate-500">此版本没有附加发行说明。</div>
              )}

              <div className="flex items-start gap-2 border-l-2 border-amber-400 bg-amber-50 px-3 py-2.5 text-xs leading-5 text-amber-900">
                <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" />
                <span>安装前会等待本地代理请求结束，再停止代理并安装更新。</span>
              </div>

              {busy && (
                <div className="grid gap-2" aria-live="polite">
                  <div className="flex items-center justify-between text-xs text-slate-600">
                    <span>{phaseLabel(state.phase)}</span>
                    <span>{percent === null ? formatBytes(state.downloadedBytes) : `${percent}%`}</span>
                  </div>
                  <div className="h-2 overflow-hidden rounded-sm bg-slate-100">
                    <div
                      className={percent === null ? "h-full w-1/3 animate-pulse bg-cyan-500" : "h-full bg-cyan-500 transition-[width]"}
                      style={percent === null ? undefined : { width: `${percent}%` }}
                    />
                  </div>
                  {state.totalBytes !== null && (
                    <div className="text-right text-[11px] text-muted-foreground">
                      {formatBytes(state.downloadedBytes)} / {formatBytes(state.totalBytes)}
                    </div>
                  )}
                </div>
              )}

              {state.error && <div className="text-sm leading-6 text-rose-700">{state.error}</div>}
            </>
          ) : null}
        </div>

        <div className="flex justify-end gap-2 border-t border-border px-5 py-4">
          {state.phase === "checking" ? (
            <Button disabled type="button" variant="secondary">
              <Loader2 className="h-4 w-4 animate-spin" />
              正在检查
            </Button>
          ) : state.phase === "failed" ? (
            <>
              <Button type="button" variant="outline" onClick={onDismiss}>关闭</Button>
              <Button type="button" onClick={onRetry}>
                <RotateCcw className="h-4 w-4" />
                重新检查
              </Button>
            </>
          ) : busy ? (
            <Button disabled type="button" variant="secondary">
              <Loader2 className="h-4 w-4 animate-spin" />
              正在处理
            </Button>
          ) : (
            <>
              <Button type="button" variant="outline" onClick={onDismiss}>稍后更新</Button>
              <Button type="button" onClick={onInstall}>
                <Download className="h-4 w-4" />
                立即更新
              </Button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

function phaseLabel(phase: UpdaterState["phase"]) {
  if (phase === "cleaning") return "正在停止本地代理";
  if (phase === "installing") return "正在安装并准备重启";
  return "正在下载更新";
}

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
