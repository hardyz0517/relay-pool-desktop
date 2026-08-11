import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { RequestStatusCode } from "./RequestLogTable";

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
