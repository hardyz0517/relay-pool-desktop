// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it } from "vitest";
import type { RoutingCandidateView as LocalRoutingCandidate } from "@/lib/types/routingWorkspace";
import {
  LocalRoutingCandidateHeader,
  LocalRoutingCandidateRow,
} from "./LocalRoutingCandidateRow";
import {
  LocalRoutingStatusCandidateHeader,
  LocalRoutingStatusCandidateRow,
} from "./LocalRoutingStatusCandidateRow";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => {
  document.body.innerHTML = "";
});

describe("local routing candidate rows", () => {
  it("renders health as a dedicated column in edit and status tables", async () => {
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);

    await act(async () => {
      root.render(
        <>
          <LocalRoutingCandidateHeader />
          <LocalRoutingCandidateRow candidate={candidate()} />
          <LocalRoutingStatusCandidateHeader />
          <LocalRoutingStatusCandidateRow
            candidate={candidate({ currentConcurrency: 3 })}
            order={1}
            nowMs={0}
          />
        </>,
      );
    });

    const headers = Array.from(host.children).filter((element) =>
      element.className.includes("grid-cols"),
    );
    expect(headerLabels(headers[0])).toEqual([
      "",
      "候选密钥",
      "参与状态",
      "健康状态",
      "有效倍率",
      "余额",
      "冷却",
    ]);

    expect(metricCell(headers[3], "当前并发").textContent).toContain("3");
    expect(headerLabels(headers[2])).toEqual([
      "候选密钥",
      "参与状态",
      "健康状态",
      "有效倍率",
      "余额",
      "冷却",
      "当前并发",
    ]);
    expect(
      Array.from(headers[2].children)
        .slice(1)
        .every((cell) => cell.className.includes("text-center")),
    ).toBe(true);
    expect(metricCell(headers[3], "参与状态").className).toContain("md:items-center");
    expect(metricCell(headers[3], "当前并发").className).toContain("md:text-center");

    const row = headers[1];
    const participationCell = metricCell(row, "参与状态");
    const healthCell = metricCell(row, "健康状态");
    expect(participationCell.textContent).toContain("可参与");
    expect(participationCell.textContent).not.toContain("未知");
    expect(healthCell.textContent).toContain("未知");

    await act(async () => root.unmount());
  });

  it("distinguishes request exclusion from an administratively paused key", async () => {
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);

    await act(async () => {
      root.render(
        <>
          <LocalRoutingCandidateRow
            candidate={candidate({
              previewEligible: false,
              previewRejectReasons: ["group_mismatch"],
              routingGroupMatch: false,
            })}
          />
          <LocalRoutingCandidateRow
            candidate={candidate({
              schedulable: false,
              previewEligible: false,
              previewRejectReasons: ["candidate_unschedulable"],
            })}
          />
        </>,
      );
    });

    const rows = Array.from(host.children);
    expect(metricCell(rows[0], "参与状态").textContent).toBe("参与状态不参与分组不匹配");
    expect(metricCell(rows[1], "参与状态").textContent).toBe(
      "参与状态已暂停路由密钥已暂停路由",
    );

    await act(async () => root.unmount());
  });
});

function headerLabels(element: Element) {
  return Array.from(element.children).map((child) => child.textContent ?? "");
}

function metricCell(row: Element, label: string) {
  const labelElement = Array.from(row.querySelectorAll("div")).find(
    (element) => element.children.length === 0 && element.textContent === label,
  );
  if (!labelElement?.parentElement) {
    throw new Error(`Missing metric cell: ${label}`);
  }
  return labelElement.parentElement;
}

function candidate(overrides: Partial<LocalRoutingCandidate> = {}): LocalRoutingCandidate {
  return {
    stationKeyId: "key-1",
    stationId: "station-1",
    stationName: "Station",
    keyName: "密钥",
    endpoint: "chat_completions",
    priority: 0,
    enabled: true,
    schedulable: true,
    healthState: "unknown",
    currentConcurrency: null,
    lastSuccessAt: null,
    lastFailureAt: null,
    cooldownUntil: null,
    routingGroupScope: "all_groups",
    routingGroupMatch: true,
    previewEligible: true,
    previewRejectReasons: [],
    facts: [],
    ...overrides,
  };
}
