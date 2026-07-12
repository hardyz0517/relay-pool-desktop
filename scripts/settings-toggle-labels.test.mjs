import { readFile } from "node:fs/promises";

const settingsSource = await readFile("src/features/settings/SettingsPage.tsx", "utf8");
const routingFieldsSource = await readFile(
  "src/features/routing/LocalRoutingSettingsFields.tsx",
  "utf8",
);
const switchControlSource = await readFile("src/components/ui/SwitchControl.tsx", "utf8");

const settingsSwitch = settingsSource.match(/ariaLabel="显示高级工具"[\s\S]*?\/>/);
if (!settingsSwitch) {
  console.error("显示高级工具 switch should be present in settings page.");
  process.exit(1);
}
if (!settingsSwitch[0].includes("showLabel={false}")) {
  console.error("显示高级工具 switch should explicitly hide inline state text.");
  process.exit(1);
}

const routingSwitch = routingFieldsSource.match(/ariaLabel="余额耗尽兜底"[\s\S]*?\/>/);
if (!routingSwitch) {
  console.error("余额耗尽兜底 switch should be present in routing fields.");
  process.exit(1);
}
if (!routingSwitch[0].includes("showLabel={false}")) {
  console.error("余额耗尽兜底 switch should explicitly hide inline state text.");
  process.exit(1);
}

if (settingsSource.includes("允许余额耗尽兜底") || settingsSource.includes("开发者模式")) {
  console.error("settings page should not render old routing/developer toggle labels.");
  process.exit(1);
}

if (
  switchControlSource.includes(
    '"inline-flex h-8 items-center justify-between gap-2 rounded-full border border-border bg-white px-2',
  )
) {
  console.error("Unlabeled switches should not inherit the labeled switch wrapper shell.");
  process.exit(1);
}

console.log("Settings toggles render without inline state text.");
