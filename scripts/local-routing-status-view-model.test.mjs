import assert from "node:assert/strict";
import {
  buildCooldownDisplay,
  buildLatestDecisionDisplay,
  formatPreviewRejectReason,
} from "../src/features/routing/localRoutingStatusViewModel.ts";

assert.deepEqual(
  buildCooldownDisplay("ready", 120_000, 60_000),
  { active: false, label: "无", remainingSeconds: null },
  "healthState must override a stale future cooldown timestamp",
);

assert.deepEqual(buildCooldownDisplay("cooldown", 125_000, 60_000), {
  active: true,
  label: "01:05",
  remainingSeconds: 65,
});
assert.equal(buildCooldownDisplay("cooldown", 3_721_000, 60_000).label, "1:01:01");
assert.equal(buildCooldownDisplay("cooldown", null, 60_000).label, "冷却中");
assert.equal(buildCooldownDisplay("cooldown", Number.NaN, 60_000).label, "冷却中");
assert.equal(buildCooldownDisplay("cooldown", 59_999, 60_000).label, "即将结束");

assert.equal(formatPreviewRejectReason("routing_group_mismatch"), "分组不匹配");
assert.equal(formatPreviewRejectReason("multiplier_evidence_low_confidence"), "费率可信度不足");
assert.equal(formatPreviewRejectReason("routing_multiplier_limit_not_configured"), "倍率上限未设置");

const latestDecision = {
  id: "decision-1",
  decidedAt: "2026-07-13T01:29:38.000Z",
  endpoint: "chat_completions",
  model: null,
  selectedStationKeyId: "key-1",
  selectedStationId: "station-1",
  selectedStationName: "AI鸡神 / 鸡神",
  policy: "cost_stable_first",
  status: "selected",
  reason: "selected",
  fallbackCount: 0,
};

assert.equal(buildLatestDecisionDisplay(false, latestDecision).badge, "历史记录");
assert.equal(buildLatestDecisionDisplay(true, latestDecision).badge, "已选中");
assert.equal(buildLatestDecisionDisplay(true, null).title, "尚无路由记录");

console.log("local routing status view model ok");
