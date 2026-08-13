import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");
const localMetricsStart = dashboardSource.indexOf('title="本地路由指标"');
const stationMetricsStart = dashboardSource.indexOf('title="中转站指标统计"');

assert.ok(localMetricsStart >= 0 && stationMetricsStart > localMetricsStart);

const localMetrics = dashboardSource.slice(localMetricsStart, stationMetricsStart);
const expectedOrder = ["今日请求", "今日消耗", "今日 Token", "可用密钥", "平均耗时", "实时流量"];
const positions = expectedOrder.map((label) => localMetrics.indexOf(`label: "${label}"`));

assert.ok(positions.every((position) => position >= 0), "local route metrics should define all six cards");
assert.deepEqual([...positions].sort((left, right) => left - right), positions, "local route metrics should follow the requested 3x2 reading order");
assert.equal((localMetrics.match(/label:\s*"/g) ?? []).length, 6, "local route metrics should contain exactly six cards");
assert.doesNotMatch(localMetrics, /label:\s*"总余额"|label:\s*"累计 Token"|label:\s*"性能概览"/);
assert.match(localMetrics, /label:\s*"今日 Token"[\s\S]*?· 累计 \$\{totalTokens === null/);

const stationMetrics = dashboardSource.slice(stationMetricsStart);
assert.match(stationMetrics, /label:\s*"总余额"[\s\S]*?formatBalance\(totalBalance, primaryBalanceCurrency\)/);
assert.doesNotMatch(stationMetrics, /label:\s*"站点累计 Token"/);
assert.match(stationMetrics, /label:\s*"站点今日 Token"[\s\S]*?detail:\s*`累计 \$\{formatCompactNumber\(stationUsage\.totalTokenCount\)\}`/);
