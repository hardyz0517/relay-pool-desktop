// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it } from "vitest";
import type { LocalRoutingCandidateRow as LocalRoutingCandidate } from "@/lib/types/localRouting";
import {
  LocalRoutingCandidateHeader,
  LocalRoutingCandidateRow,
} from "./LocalRoutingCandidateRow";
import { LocalRoutingStatusCandidateHeader } from "./LocalRoutingStatusCandidateRow";

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
    expect(headerLabels(headers[2])).toEqual([
      "候选密钥",
      "参与状态",
      "健康状态",
      "有效倍率",
      "余额",
      "冷却",
    ]);

    const row = headers[1];
    const participationCell = metricCell(row, "参与状态");
    const healthCell = metricCell(row, "健康状态");
    expect(participationCell.textContent).toContain("可参与");
    expect(participationCell.textContent).not.toContain("未知");
    expect(healthCell.textContent).toContain("未知");

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

function candidate(): LocalRoutingCandidate {
  return {
    stationKeyId: "key-1",
    stationId: "station-1",
    stationName: "Station",
    keyName: "Key",
    endpoint: "chat_completions",
    priority: 0,
    enabled: true,
    schedulable: true,
    healthState: "unknown",
    lastSuccessAt: null,
    lastFailureAt: null,
    cooldownUntil: null,
    routingGroupScope: "all_groups",
    routingGroupMatch: true,
    previewEligible: true,
    previewRejectReasons: [],
    facts: [],
  };
}
