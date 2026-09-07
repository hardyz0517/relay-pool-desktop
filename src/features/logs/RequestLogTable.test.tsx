import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { RequestLog } from "@/lib/types/proxy";
import { RequestLogTable, RequestStatusCode } from "./RequestLogTable";

describe("RequestLogTable", () => {
  it("shows only the compact columns and omits the year by default", () => {
    const markup = renderToStaticMarkup(
      <RequestLogTable
        rows={[{ id: "log-1", path: "/v1/responses", startedAt: "2026-08-11T12:34:56" } as RequestLog]}
        keyById={new Map()}
        stationById={new Map()}
        selectedId="log-1"
        onSelect={() => undefined}
      />,
    );

    for (const header of ["密钥", "模型", "状态码", "分组", "倍率", "Token", "费用", "延迟", "时间"]) {
      expect(markup).toContain(`<th`);
      expect(markup).toContain(`>${header}</th>`);
    }
    expect(markup).not.toContain(">推理强度</th>");
    expect(markup).not.toContain(">端点</th>");
    expect(markup).not.toContain(">类型</th>");
    expect(markup).not.toContain(">计费模式</th>");
    expect(markup).toContain("min-w-[1040px]");
    expect(markup).toContain("w-[144px] min-w-[144px] tabular-nums");
    expect(markup).toContain("class=\"h-8 whitespace-nowrap px-2.5 text-center\">费用</th>");
    expect(markup).toContain("class=\"h-8 whitespace-nowrap px-2.5 w-[128px] min-w-[128px] max-w-[128px] text-center\">延迟</th>");
    expect(markup).toContain("[&amp;_td]:align-middle");
    expect(markup).toContain("[&amp;_td:last-child]:overflow-visible");
    expect(markup).toContain("[&amp;_td:last-child]:text-clip");
    expect(markup).not.toContain("bg-selected");
    expect(markup).not.toContain("text-selected-foreground");
    expect(markup).toContain("relative min-h-[36px] w-full min-w-0 overflow-hidden text-xs leading-4");
    expect(markup).toContain("absolute inset-y-0 left-3 right-0 grid content-center");
    expect(markup).toContain("grid w-full grid-cols-[auto_minmax(0,1fr)] items-center gap-x-2 gap-y-0.5");
    expect(markup).toContain("absolute left-0 top-1/2 h-9 w-1 -translate-y-1/2");
    expect(markup).toContain("text-left text-muted-foreground");
    expect(markup).toContain("text-left font-medium tabular-nums");
    expect(markup).toContain("08/11 12:34:56");
    expect(markup).not.toContain("2026/08/11 12:34:56");
  });

  it("restores every column and the full timestamp when compact display is off", () => {
    const markup = renderToStaticMarkup(
      <RequestLogTable
        rows={[{ id: "log-1", path: "/v1/responses", startedAt: "2026-08-11T12:34:56" } as RequestLog]}
        keyById={new Map()}
        stationById={new Map()}
        selectedId={null}
        onSelect={() => undefined}
        compact={false}
      />,
    );

    expect(markup).toContain(">推理强度</th>");
    expect(markup).toContain(">端点</th>");
    expect(markup).toContain(">类型</th>");
    expect(markup).toContain(">计费模式</th>");
    expect(markup).toContain("min-w-[1480px]");
    expect(markup).toContain("w-[176px] min-w-[176px] tabular-nums");
    expect(markup).toContain("2026/08/11 12:34:56");
  });

  it("shows the requested model and the actual mapped model", () => {
    const markup = renderToStaticMarkup(
      <RequestLogTable
        rows={[{
          id: "log-1",
          path: "/v1/responses",
          startedAt: "2026-08-11T12:34:56",
          model: "gpt-5.2",
          resolvedUpstreamModel: "grok-4.6",
        } as RequestLog]}
        keyById={new Map()}
        stationById={new Map()}
        selectedId="log-1"
        onSelect={() => undefined}
      />,
    );

    expect(markup).toContain("gpt-5.2");
    expect(markup).toContain("grok-4.6");
    expect(markup).toContain("gpt-5.2 → grok-4.6");
  });

  it("shows input tokens excluding cache reads while keeping the cache breakdown", () => {
    const markup = renderToStaticMarkup(
      <RequestLogTable
        rows={[{
          id: "log-1",
          path: "/v1/responses",
          startedAt: "2026-08-11T12:34:56",
          status: "success",
          promptTokens: 156_879,
          completionTokens: 197,
          totalTokens: 157_076,
          cacheReadTokens: 156_672,
          cacheCreationTokens: null,
        } as RequestLog]}
        keyById={new Map()}
        stationById={new Map()}
        selectedId="log-1"
        onSelect={() => undefined}
      />,
    );

    expect(markup).toContain('title="输入 Token"');
    expect(markup).toContain(">207</span>");
    expect(markup).toContain('title="缓存读取 Token"');
    expect(markup).toContain(">156.7K</span>");
  });

  it("uses Sub2API emerald/amber/orange bars instead of blue info tones", () => {
    const markup = renderToStaticMarkup(
      <RequestLogTable
        rows={[
          {
            id: "fast",
            path: "/v1/responses",
            startedAt: "2026-09-04T23:55:12",
            firstTokenMs: 3_770,
            durationMs: 5_510,
          } as RequestLog,
          {
            id: "notice",
            path: "/v1/responses",
            startedAt: "2026-09-04T23:55:12",
            firstTokenMs: 10_950,
            durationMs: 11_080,
          } as RequestLog,
          {
            id: "slow",
            path: "/v1/responses",
            startedAt: "2026-09-04T23:55:12",
            firstTokenMs: 57_590,
            durationMs: 60_880,
          } as RequestLog,
        ]}
        keyById={new Map()}
        stationById={new Map()}
        selectedId={null}
        onSelect={() => undefined}
      />,
    );

    expect(markup).toContain("bg-gradient-to-b from-40% to-60% from-latency-normal to-latency-normal");
    expect(markup).toContain("bg-gradient-to-b from-40% to-60% from-latency-notice to-latency-normal");
    expect(markup).toContain("bg-gradient-to-b from-40% to-60% from-latency-warning to-latency-notice");
    expect(markup).toContain("text-latency-normal-foreground");
    expect(markup).toContain("text-latency-notice-foreground");
    expect(markup).toContain("text-latency-warning-foreground");
    expect(markup).not.toContain("bg-info-foreground");
    expect(markup).not.toContain("bg-success-foreground");
  });

  it("renders timeout instead of a multi-hour duration that would overflow adjacent columns", () => {
    const markup = renderToStaticMarkup(
      <RequestLogTable
        rows={[{
          id: "log-1",
          path: "/v1/responses",
          startedAt: "2026-09-04T23:55:12",
          firstTokenMs: null,
          durationMs: 78_257_040,
        } as RequestLog]}
        keyById={new Map()}
        stationById={new Map()}
        selectedId="log-1"
        onSelect={() => undefined}
      />,
    );

    expect(markup).toContain(">超时</span>");
    expect(markup).not.toContain(">78257.04s</span>");
    expect(markup).toContain("title=\"超时（78257.04s）");
  });
});

describe("RequestStatusCode", () => {
  it.each([
    [200, "text-success-foreground"],
    [404, "text-warning-foreground"],
    [503, "text-danger-foreground"],
  ])("renders HTTP %i with the expected tone", (status, tone) => {
    const markup = renderToStaticMarkup(<RequestStatusCode value={status} />);

    expect(markup).toContain(`HTTP ${status}`);
    expect(markup).toContain(`>${status}</span>`);
    expect(markup).toContain(tone);
  });

  it("renders historical records without a stored status as unknown", () => {
    const markup = renderToStaticMarkup(<RequestStatusCode value={null} />);

    expect(markup).toContain("历史记录未保存 HTTP 状态码");
    expect(markup).toContain(">—</span>");
    expect(markup).toContain("text-muted-foreground");
  });

  it("renders an active request as processing before an HTTP status is available", () => {
    const markup = renderToStaticMarkup(<RequestStatusCode value={null} inProgress />);

    expect(markup).toContain("请求仍在处理中");
    expect(markup).toContain(">处理中</span>");
    expect(markup).toContain("text-info-foreground");
  });
});
