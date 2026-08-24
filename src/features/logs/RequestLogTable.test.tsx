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
    expect(markup).toContain("class=\"h-8 whitespace-nowrap px-2.5 text-center\">延迟</th>");
    expect(markup).toContain("[&amp;_td]:align-middle");
    expect(markup).toContain("[&amp;_td:last-child]:overflow-visible");
    expect(markup).toContain("[&amp;_td:last-child]:text-clip");
    expect(markup).not.toContain("bg-selected");
    expect(markup).not.toContain("text-selected-foreground");
    expect(markup).toContain("relative min-h-[36px] w-full text-xs leading-4");
    expect(markup).toContain("absolute left-1/2 top-1/2 grid w-max -translate-x-1/2 -translate-y-1/2");
    expect(markup).toContain("absolute right-full top-0 mr-2.5 h-9 w-1");
    expect(markup).toContain("flex items-center gap-2 whitespace-nowrap");
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
