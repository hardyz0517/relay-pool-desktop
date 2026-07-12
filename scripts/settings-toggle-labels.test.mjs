import { readFile } from "node:fs/promises";

const source = await readFile("src/features/settings/SettingsPage.tsx", "utf8");
const switchControlSource = await readFile("src/components/ui/SwitchControl.tsx", "utf8");

const toggleLabels = [
  ["允许余额耗尽兜底", /ariaLabel="允许余额耗尽兜底"[\s\S]*?\/>/],
  ["开发者模式", /ariaLabel="开发者模式"[\s\S]*?\/>/],
];

for (const [label, pattern] of toggleLabels) {
  const match = source.match(pattern);
  if (!match) {
    console.error(`${label} switch should be present in settings page.`);
    process.exit(1);
  }
  const switchSource = match[0];
  if (switchSource.includes('offLabel="关闭"') || switchSource.includes('onLabel="开启"')) {
    console.error(`${label} switch should not render inline state text.`);
    process.exit(1);
  }
  if (!switchSource.includes("showLabel={false}")) {
    console.error(`${label} switch should explicitly hide inline state text.`);
    process.exit(1);
  }
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
