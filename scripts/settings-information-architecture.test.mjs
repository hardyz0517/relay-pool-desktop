import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const settings = await readFile("src/features/settings/SettingsPage.tsx", "utf8");
const scaffold = await readFile("src/components/shell/PageScaffold.tsx", "utf8");
const app = await readFile("src/app/App.tsx", "utf8");
const pricing = await readFile("src/features/pricing/PricingPage.tsx", "utf8");
const collectors = await readFile("src/features/collectors/CollectorsPage.tsx", "utf8");

for (const label of [
  "倍率上限",
  "默认路由分组",
  "低余额阈值",
  "余额采集周期（分钟）",
  "分组 / 倍率采集周期（分钟）",
  "模型采集周期（分钟）",
  "价格刷新周期（分钟）",
  "模型基准价格",
  "采集超时（秒）",
  "采集并发数",
  "允许余额耗尽兜底",
]) {
  assert.ok(!settings.includes(`label="${label}"`), `settings should not render ${label}`);
}

assert.match(settings, /title="网络与代理"/);
assert.match(settings, /label="默认网络出口"/);
assert.match(settings, /title="数据"/);
assert.match(settings, /title="高级"/);
assert.match(settings, /label="显示高级工具"/);
assert.match(settings, /在侧边栏显示采集中心/);
assert.doesNotMatch(settings, /onOpenModelBasePrices/);

assert.match(scaffold, /max-w-\[1080px\]/);
assert.doesNotMatch(app, /<SettingsPage onOpenModelBasePrices=/);
assert.match(app, /return <SettingsPage \/>/);
assert.match(pricing, /模型基准价格/);
assert.match(collectors, /<CollectorAdvancedSettings \/>/);

console.log("settings information architecture contract ok");
