// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { Wallet } from "lucide-react";
import { describe, expect, it } from "vitest";
import { MetricPanel } from "./MetricPanel";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

describe("MetricPanel theme accents", () => {
  it("uses dedicated metric accent tokens for dashboard color", async () => {
    const host = document.createElement("div");
    const root = createRoot(host);

    await act(async () => root.render(
      <MetricPanel
        title="Metrics"
        metrics={[
          {
            label: "Balance",
            value: "¥35.14",
            icon: Wallet,
            accent: "emerald",
          },
        ]}
      />,
    ));

    const iconSurface = host.querySelector<HTMLElement>("svg")!.parentElement!;
    const value = host.querySelector<HTMLElement>(".text-\\[22px\\]")!;

    expect(iconSurface.className).toContain("bg-metric-emerald-surface");
    expect(iconSurface.className).toContain("text-metric-emerald-foreground");
    expect(value.className).toContain("text-metric-emerald-foreground");

    await act(async () => root.unmount());
  });
});
