import { useEffect, useMemo, useState, type ReactNode } from "react";
import { Activity, Bot, Loader2, MessageCircle, RotateCw } from "lucide-react";
import { Button, Dialog, SelectControl } from "@/components/ui";
import type { StationKeyCapabilities } from "@/lib/types/routing";
import type { KeyPoolItem, StationKeyConnectivityTestResult } from "@/lib/types/stationKeys";
import { cn } from "@/lib/utils";

export const DEFAULT_KEY_CONNECTIVITY_TEST_MODEL = "gpt-5.5";

const defaultKeyConnectivityModelOptions = [
  { value: "gpt-5.5", label: "GPT-5.5" },
  { value: "gpt-5.4", label: "GPT-5.4" },
  { value: "gpt-4.1", label: "GPT-4.1" },
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
  onTest: (model: string) => void;
}) {
  const [model, setModel] = useState(DEFAULT_KEY_CONNECTIVITY_TEST_MODEL);
  const open = item !== null;
  const modelOptions = useMemo(
    () => buildKeyConnectivityModelOptions(capabilities),
    [capabilities],
  );
  const completed = Boolean(result || error);
  const selectedModelLabel = modelOptions.find((option) => option.value === model)?.label ?? model;
  const fullResponseText = result
    ? result.ok
      ? result.message || `Hi! What can I help you with? (${formatConnectivityDuration(result.durationMs)})`
      : `${result.statusCode || "网络"} · ${result.message}`
    : error ?? "";
  const responseTypingComplete = completed && displayedResponseText === fullResponseText;

  useEffect(() => {
    if (open) {
      setModel(modelOptions[0]?.value ?? DEFAULT_KEY_CONNECTIVITY_TEST_MODEL);
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
            onClick={() => onTest(model)}
          >
            {testing ? <Loader2 className="h-3.5 w-3.5 animate-spin motion-reduce:animate-none" /> : <RotateCw className="h-3.5 w-3.5" />}
            {testing ? "测试中..." : completed ? "重试" : "测试模型"}
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

        <div className="rounded-[10px] border border-border bg-surface-inset p-4 font-mono text-[12px] leading-5 text-muted-foreground shadow-inner">
          {buildConnectivityConsoleLines({
            item,
            model,
            selectedModelLabel,
            testing,
            result,
            error,
            displayedResponseText,
            streamFallbackReason,
            progressLabel,
            responseTypingComplete,
          }).map((line, index) => (
            <div key={`${line.text}-${index}`} className={line.className}>
              {testing && index === 0 ? (
                <span className="inline-flex items-center gap-1.5">
                  <Loader2
                    data-testid="key-connectivity-console-spinner"
                    className="h-3.5 w-3.5 animate-spin motion-reduce:animate-none"
                  />
                  {line.text}
                </span>
              ) : (
                line.text
              )}
            </div>
          ))}
        </div>

        <div className="flex items-center justify-between text-[11px] text-muted-foreground">
          <span className="inline-flex items-center gap-1.5">
            <Bot className="h-3.5 w-3.5" />
            测试模型
          </span>
          <span className="inline-flex items-center gap-1.5">
            <MessageCircle className="h-3.5 w-3.5" />
            提示词："hi"
          </span>
        </div>
      </div>
    </Dialog>
  );
}

export function buildConnectivityConsoleLines({
  item,
  model,
  selectedModelLabel,
  testing,
  result,
  error,
  displayedResponseText,
  streamFallbackReason,
  progressLabel,
  responseTypingComplete,
}: {
  item: KeyPoolItem | null;
  model: string;
  selectedModelLabel: string;
  testing: boolean;
  result: StationKeyConnectivityTestResult | null;
  error: string | null;
  displayedResponseText: string;
  streamFallbackReason: string | null;
  progressLabel: string | null;
  responseTypingComplete: boolean;
}) {
  const lines = [
    { text: testing ? "连接 API 中..." : `开始测试密钥：${item?.name ?? "密钥"}`, className: "text-info-foreground" },
    { text: `使用模型：${selectedModelLabel}`, className: "font-semibold text-info-foreground" },
    { text: '发送测试消息："hi"', className: "text-muted-foreground" },
  ];
  const responseLabelLine = { text: "响应：", className: "font-semibold text-warning-foreground" };

  if (testing) {
    const progressLines = progressLabel
      ? [{ text: progressLabel, className: "text-info-foreground" }]
      : [];
    const fallbackLines = streamFallbackReason
      ? [
          { text: "流式失败，已清空部分输出并回退非流式请求。", className: "text-warning-foreground" },
          { text: `原因：${streamFallbackReason}`, className: "text-warning-foreground/80" },
        ]
      : [];
    const responseLines = displayedResponseText
      ? [{ text: displayedResponseText, className: "font-semibold text-success-foreground" }]
      : [{ text: "等待流式片段...", className: "text-muted-foreground" }];
    return [...lines, responseLabelLine, ...progressLines, ...fallbackLines, ...responseLines];
  }
  if (result) {
    return [
      ...lines,
      {
        text: result.responseMode === "stream" ? "响应模式：流式响应" : "响应模式：非流式回退",
        className: result.responseMode === "stream" ? "text-success-foreground" : "text-warning-foreground",
      },
      ...(result.streamFallbackReason
        ? [{ text: `回退原因：${result.streamFallbackReason}`, className: "text-warning-foreground/80" }]
        : []),
      responseLabelLine,
      {
        text: displayedResponseText,
        className: result.ok ? "font-semibold text-success-foreground" : "font-semibold text-danger-foreground",
      },
      ...(responseTypingComplete
        ? [
            {
              text: result.ok ? "测试完成！" : "测试未通过。",
              className: result.ok
                ? "mt-2 border-t border-border pt-2 text-success-foreground"
                : "mt-2 border-t border-border pt-2 text-danger-foreground",
            },
          ]
        : []),
    ];
  }
  if (error) {
    return [
      ...lines,
      responseLabelLine,
      { text: displayedResponseText, className: "font-semibold text-danger-foreground" },
      ...(responseTypingComplete
        ? [{ text: "测试失败。", className: "mt-2 border-t border-border pt-2 text-danger-foreground" }]
        : []),
    ];
  }
  return [...lines, { text: `待测试模型 ${model}`, className: "text-muted-foreground" }];
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
