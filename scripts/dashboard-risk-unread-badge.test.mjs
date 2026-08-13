import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");
const riskHeaderStart = dashboardSource.indexOf("当前风险");
const riskCardsStart = dashboardSource.indexOf('<div className="grid min-w-0 grid-cols-3 gap-3">', riskHeaderStart);
const riskHeader = dashboardSource.slice(riskHeaderStart, riskCardsStart);

assert.doesNotMatch(dashboardSource, /无未读提醒/);
assert.ok(riskHeaderStart >= 0 && riskCardsStart > riskHeaderStart);
assert.doesNotMatch(riskHeader, /StatusBadge|unreadReminders/);
