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
  it("renders health in edit and policy score in the status table", async () => {
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
      "密钥评分",
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
    const healthCell = metricCell(headers[1], "健康状态");
    const scoreCell = metricCell(headers[3], "密钥评分");
    expect(participationCell.textContent).toContain("可参与");
    expect(participationCell.textContent).not.toContain("未知");
    expect(healthCell.textContent).toContain("未知");
    expect(scoreCell.textContent).toContain("—");

    await act(async () => {
      (headers[3].querySelector('button[aria-label*="评分计算"]') as HTMLButtonElement).click();
    });
    expect(document.body.textContent).toContain("暂无评分明细");

    await act(async () => root.unmount());
  });

  it("formats the backend utility score as a compact percentage-like value", async () => {
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);

    await act(async () => {
      root.render(
        <LocalRoutingStatusCandidateRow
          candidate={candidate({ score: 8_575 })}
          order={1}
          nowMs={0}
        />,
      );
    });

    expect(metricCell(host.firstElementChild!, "密钥评分").textContent).toContain("86 分");
    await act(async () => root.unmount());
  });

  it("opens the score calculation dialog from the score cell", async () => {
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);

    await act(async () => {
      root.render(
        <LocalRoutingStatusCandidateRow
          candidate={candidate({
            score: 8_510,
            scoreDetails: {
              total: 8_510,
              reliability: { score: 9_000, weight: 4_000, contribution: 3_600, inputs: [{ label: "成功请求", value: "19" }] },
              responsiveness: { score: 8_000, weight: 2_500, contribution: 2_000, inputs: [{ label: "最近平均延迟", value: "240 ms" }] },
              cost: { score: 9_302, weight: 2_000, contribution: 1_860, inputs: [{ label: "密钥有效倍率", value: "0.0750x" }, { label: "倍率代理成本分", value: "93.0%" }] },
              preference: { score: 7_000, weight: 1_500, contribution: 1_050, inputs: [{ label: "候选优先级", value: "3000" }] },
            },
          })}
          order={1}
          nowMs={0}
        />,
      );
    });

    await act(async () => {
      (host.querySelector('button[aria-label*="评分计算"]') as HTMLButtonElement).click();
    });
    expect(document.body.textContent).toContain("评分因子");
    expect(document.body.textContent).toContain("可靠性");
    expect(document.body.textContent).toContain("根据历史成功率计算");
    expect(document.body.textContent).toContain("成功请求 19");
    expect(document.body.textContent).toContain("密钥有效倍率 0.0750x");
    expect(document.body.textContent).not.toContain("成功次数 + 先验成功");
    const detailButton = Array.from(document.querySelectorAll("button")).find(
      (button) => button.textContent === "查看计算详情",
    );
    expect(detailButton).toBeTruthy();
    await act(async () => {
      detailButton?.click();
    });
    expect(document.body.textContent).toContain("成功次数 + 先验成功");
    const secondDetailButton = Array.from(document.querySelectorAll("button")).find(
      (button) => button.textContent === "查看计算详情" && button !== detailButton,
    );
    await act(async () => {
      secondDetailButton?.click();
    });
    expect(document.body.textContent).toContain("最近延迟 / 延迟上限");
    for (let index = 0; index < 2; index += 1) {
      const nextDetailButton = Array.from(document.querySelectorAll("button")).find(
        (button) => button.textContent === "查看计算详情",
      );
      await act(async () => {
        nextDetailButton?.click();
      });
    }
    expect(document.body.textContent).toContain("候选优先级");
    expect(document.body.textContent).toContain("240 ms");
    expect(document.body.textContent).toContain("倍率代理成本分");
    expect(document.body.textContent).toContain("最终评分");

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
              scoreStatus: "excluded",
              plannerExclusionCodes: ["group_mismatch"],
              previewEligible: false,
              previewRejectReasons: ["group_mismatch"],
              routingGroupMatch: false,
            })}
          />
          <LocalRoutingCandidateRow
            candidate={candidate({
              schedulable: false,
              scoreStatus: "excluded",
              plannerExclusionCodes: ["candidate_unschedulable"],
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
    score: null,
    scoreDetails: null,
    currentConcurrency: null,
    lastSuccessAt: null,
    lastFailureAt: null,
    cooldownUntil: null,
    routingGroupScope: "all_groups",
    routingGroupMatch: true,
    scoreStatus: "scored",
    plannerExclusionCodes: [],
    assessmentSnapshotId: null,
    assessmentDurableRevision: null,
    assessmentRequestContextFingerprint: null,
    previewEligible: true,
    previewRejectReasons: [],
    facts: [],
    ...overrides,
  };
}
