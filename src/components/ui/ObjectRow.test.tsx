// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it } from "vitest";
import { ObjectRow } from "./ObjectRow";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

describe("ObjectRow metrics", () => {
  it("supports centering an individual metric", async () => {
    const host = document.createElement("div");
    const root = createRoot(host);

    await act(async () => root.render(
      <ObjectRow
        title="Key"
        metrics={[{ label: "当前并发", value: "0", align: "center" }]}
      />,
    ));

    const metric = host.querySelector<HTMLElement>(".min-w-\\[72px\\]")!;
    expect(metric.className).toContain("text-center");
    expect(metric.className).not.toContain("text-right");

    await act(async () => root.unmount());
  });
});
