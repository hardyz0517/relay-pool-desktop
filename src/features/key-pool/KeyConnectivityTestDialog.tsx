import { useEffect, useMemo, useState, type ReactNode } from "react";
import { Activity, AlertTriangle, CheckCircle2, Loader2, RotateCw } from "lucide-react";
import { Button, Dialog, SelectControl } from "@/components/ui";
import type { StationKeyCapabilities } from "@/lib/types/routing";
import type {
  KeyPoolItem,
  StationKeyConnectivityClientProfile,
  StationKeyConnectivityTestResult,
} from "@/lib/types/stationKeys";
import { cn } from "@/lib/utils";

export const DEFAULT_KEY_CONNECTIVITY_TEST_MODEL = "gpt-5.5";

const defaultKeyConnectivityModelOptions = [
  { value: "gpt-5.5", label: "GPT-5.5" },
  { value: "gpt-5.4", label: "GPT-5.4" },
  { value: "gpt-4.1", label: "GPT-4.1" },
];

const connectivityProfileOptions: Array<{
  value: StationKeyConnectivityClientProfile;
  label: string;
}> = [
  { value: "standard_api", label: "标准 API 请求档案" },
  { value: "codex_cli_compat", label: "Codex CLI 兼容档案" },
];

export function KeyConnectivityTestDialog({
  item,
  capabilities,
  result,
  error,
  displayedResponseText,
  streamFallbackReason,
  progressLabel,
  onDisplayedResponseTextChange,
  testing,
  onClose,
  onTest,
}: {
  item: KeyPoolItem | null;
  capabilities: StationKeyCapabilities | null;
  result: StationKeyConnectivityTestResult | null;
  error: string | null;
  displayedResponseText: string;
  streamFallbackReason: string | null;
  progressLabel: string | null;
  onDisplayedResponseTextChange: (value: string) => void;
  testing: boolean;
  onClose: () => void;
  onTest: (model: string, clientProfile: StationKeyConnectivityClientProfile) => void;
}) {
  const [model, setModel] = useState(DEFAULT_KEY_CONNECTIVITY_TEST_MODEL);
  const [clientProfile, setClientProfile] = useState<StationKeyConnectivityClientProfile>("standard_api");
  const open = item !== null;
  const modelOptions = useMemo(
    () => buildKeyConnectivityModelOptions(capabilities),
    [capabilities],
  );
  const completed = Boolean(result || error);
  const fullResponseText = result
    ? result.ok
      ? result.message || `Hi! What can I help you with? (${formatConnectivityDuration(result.durationMs)})`
      : `${result.statusCode || "网络"} · ${result.message}`
    : error ?? "";
  const responseTypingComplete = completed && displayedResponseText === fullResponseText;

  useEffect(() => {
    if (open) {
      setModel(modelOptions[0]?.value ?? DEFAULT_KEY_CONNECTIVITY_TEST_MODEL);
      setClientProfile("standard_api");
      onDisplayedResponseTextChange("");
    }
  }, [modelOptions, onDisplayedResponseTextChange, open, item?.id]);

  useEffect(() => {
    if (completed) {
      onDisplayedResponseTextChange(fullResponseText);
    }
  }, [completed, fullResponseText, onDisplayedResponseTextChange]);

  return (
    <Dialog
      open={open}
      title="测试密钥连接"
      className="max-w-[460px] rounded-[14px]"
      onClose={onClose}
      footer={
        <div className="flex items-center justify-end gap-3">
          <Button variant="ghost" className="bg-muted text-foreground hover:bg-hover" onClick={onClose}>
            关闭
          </Button>
          <Button
            className={cn(
              "min-w-[74px] bg-primary-solid hover:bg-primary-solid",
              testing && "bg-primary-solid hover:bg-primary-solid",
            )}
            disabled={!item || testing}
            onClick={() => onTest(model, clientProfile)}
          >
            {testing ? <Loader2 className="h-3.5 w-3.5 animate-spin motion-reduce:animate-none" /> : <RotateCw className="h-3.5 w-3.5" />}
            {testing ? "测试中..." : completed ? "重新测试" : "测试连接"}
          </Button>
        </div>
      }
    >
      <div data-testid="key-connectivity-test-dialog" className="space-y-4 px-5 py-4">
        <div className="flex items-center justify-between gap-3 rounded-[10px] border border-border bg-surface-subtle p-3">
          <div className="flex min-w-0 items-center gap-3">
            <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[8px] bg-primary-solid text-primary-foreground shadow-surface">
              <Activity className="h-4 w-4" />
            </div>
            <div className="min-w-0">
              <div className="truncate text-[13px] font-semibold text-foreground">{item?.name ?? "密钥"}</div>
              <div className="mt-1 flex items-center gap-1.5 text-[11px] text-muted-foreground">
                <span className="rounded bg-hover px-1.5 py-0.5 text-[10px] font-semibold uppercase text-muted-foreground">APIKEY</span>
                <span>密钥</span>
              </div>
            </div>
          </div>
          <span className="rounded-full bg-success-surface px-2.5 py-1 text-[11px] font-semibold text-success-foreground">
            {item?.enabled ? "active" : "inactive"}
          </span>
        </div>

        <Field label="选择测试模型">
          <SelectControl
            value={model}
            options={modelOptions}
            ariaLabel="选择测试模型"
            className="h-9 w-full rounded-[10px] border-border bg-surface text-[13px]"
            menuClassName="text-[13px]"
            disabled={testing}
            onChange={setModel}
          />
        </Field>

        <Field label="Responses 请求档案">
          <SelectControl
            value={clientProfile}
            options={connectivityProfileOptions}
            ariaLabel="选择 Responses 请求档案"
            className="h-9 w-full rounded-[10px] border-border bg-surface text-[13px]"
            menuClassName="text-[13px]"
            disabled={testing}
            onChange={(value) => setClientProfile(value as StationKeyConnectivityClientProfile)}
          />
          <span className="text-[11px] font-normal text-muted-foreground">
            仅用于 Responses；失败时自动回退到 Chat Completions。
          </span>
        </Field>

        <div className="rounded-[10px] border border-border bg-surface-inset p-4 text-[12px] leading-5 text-muted-foreground shadow-inner">
          {buildConnectivityConsoleLines({
            testing,
            result,
            error,
            displayedResponseText,
            streamFallbackReason,
            progressLabel,
            responseTypingComplete,
          }).map((line, index) => (
            <div key={`${line.text}-${index}`} className={cn("flex items-start gap-2", line.className)}>
              {testing && index === 0 ? <Loader2 data-testid="key-connectivity-console-spinner" className="mt-0.5 h-3.5 w-3.5 shrink-0 animate-spin motion-reduce:animate-none" /> : null}
              {!testing && index === 0 && result?.ok ? <CheckCircle2 className="mt-0.5 h-3.5 w-3.5 shrink-0" /> : null}
              {!testing && index === 0 && (error || result?.ok === false) ? <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" /> : null}
              <span>{line.text}</span>
            </div>
          ))}
        </div>

      </div>
    </Dialog>
  );
}

export function buildConnectivityConsoleLines({
  testing,
  result,
  error,
  displayedResponseText,
  streamFallbackReason,
  progressLabel,
  responseTypingComplete,
}: {
  testing: boolean;
  result: StationKeyConnectivityTestResult | null;
  error: string | null;
  displayedResponseText: string;
  streamFallbackReason: string | null;
  progressLabel: string | null;
  responseTypingComplete: boolean;
}) {
  const responseLabelLine = { text: "响应内容", className: "mt-3 border-t border-border pt-3 font-semibold text-muted-foreground" };

  if (testing) {
    return [
      { text: progressLabel || "正在测试连接...", className: "font-semibold text-info-foreground" },
      ...(streamFallbackReason
        ? [
            { text: "流式响应失败，正在回退到非流式请求。", className: "text-warning-foreground" },
            { text: `原因：${streamFallbackReason}`, className: "text-warning-foreground/80" },
          ]
        : []),
      responseLabelLine,
      { text: displayedResponseText || "等待响应...", className: "font-semibold text-muted-foreground" },
    ];
  }
  if (result) {
    const protocolFallback = result.validatedProtocol === "chat_completions";
    const streamFallback = result.responseMode === "non_stream_fallback";
    const statusLine = protocolFallback
      ? "已回退到 Chat Completions，测试成功"
      : streamFallback
        ? "已降级为非流式 Responses，测试成功"
      : result.ok
        ? "测试成功"
        : "测试未通过";
    return [
      { text: statusLine, className: result.ok && !protocolFallback && !streamFallback ? "font-semibold text-success-foreground" : "font-semibold text-warning-foreground" },
      { text: `协议 ${protocolLabel(result.validatedProtocol)} · ${connectivityProfileLabel(result.clientProfile)} · ${formatConnectivityDuration(result.durationMs)}`, className: "text-muted-foreground" },
      ...(streamFallbackReason || result.streamFallbackReason
        ? [{ text: `回退原因：${streamFallbackReason || result.streamFallbackReason}`, className: "text-warning-foreground/80" }]
        : []),
      responseLabelLine,
      {
        text: displayedResponseText,
        className: result.ok ? "font-semibold text-success-foreground" : "font-semibold text-danger-foreground",
      },
      ...(responseTypingComplete ? [{ text: result.ok ? "测试完成" : "请检查配置后重试", className: result.ok ? "text-success-foreground" : "text-danger-foreground" }] : []),
    ];
  }
  if (error) {
    return [
      { text: "测试未通过", className: "font-semibold text-danger-foreground" },
      responseLabelLine,
      { text: displayedResponseText, className: "font-semibold text-danger-foreground" },
    ];
  }
  return [{ text: "准备测试连接", className: "font-semibold text-muted-foreground" }];
}

function connectivityProfileLabel(profile: StationKeyConnectivityClientProfile) {
  return profile === "codex_cli_compat" ? "Codex CLI 兼容档案" : "标准 API 请求档案";
}

function protocolLabel(protocol: StationKeyConnectivityTestResult["validatedProtocol"]) {
  return protocol === "chat_completions" ? "Chat Completions" : "Responses";
}

function formatConnectivityDuration(durationMs: number) {
  return durationMs > 0 ? `${durationMs}ms` : "预览模式";
}

export function buildKeyConnectivityModelOptions(capabilities: StationKeyCapabilities | null) {
  const scopedModels =
    capabilities?.modelAllowlist.length
      ? capabilities.modelAllowlist
      : capabilities?.preferredModels.length
        ? capabilities.preferredModels
        : [];
  const sourceModels = scopedModels.length > 0
    ? scopedModels
    : defaultKeyConnectivityModelOptions.map((option) => option.value);
  const seen = new Set<string>();
  return sourceModels.flatMap((model) => {
    const trimmed = model.trim();
    if (!trimmed) {
      return [];
    }
    const normalized = trimmed.toLowerCase();
    if (seen.has(normalized)) {
      return [];
    }
    seen.add(normalized);
    return [{ value: trimmed, label: formatConnectivityModelLabel(trimmed) }];
  });
}

export function formatConnectivityModelLabel(model: string) {
  return defaultKeyConnectivityModelOptions.find((option) => option.value === model)?.label ?? model;
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="grid gap-1.5 text-xs font-medium text-muted-foreground">
      {label}
      {children}
    </label>
  );
}
