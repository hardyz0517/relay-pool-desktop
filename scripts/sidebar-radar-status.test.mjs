import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const appShellSource = await readFile("src/components/shell/AppShell.tsx", "utf8");
const radarIconSource = await readFile(
  "src/components/shell/LocalProxyRadarIcon.tsx",
  "utf8",
);
const stylesSource = await readFile("src/styles.css", "utf8");

assert.ok(
    appShellSource.includes("LocalProxyRadarIcon") &&
    appShellSource.includes("active={proxyRunning}") &&
    appShellSource.includes('"h-6 w-6"') &&
    appShellSource.includes('proxyRunning ? "text-success-foreground" : "text-muted-foreground"'),
  "sidebar proxy status should render the shared radar icon with a muted stopped state",
);

assert.ok(
  !appShellSource.includes('import { Circle } from "lucide-react";') &&
    !appShellSource.includes("<Circle"),
  "sidebar proxy status should not use the old single centered dot icon",
);

assert.ok(
  radarIconSource.includes('aria-hidden="true"') &&
    radarIconSource.includes("local-proxy-radar") &&
    radarIconSource.includes("local-proxy-radar--active") &&
    radarIconSource.includes("local-proxy-radar__wave") &&
    radarIconSource.includes("local-proxy-radar__core"),
  "radar icon should expose a decorative radar mark with waves and a core point",
);

assert.ok(
    stylesSource.includes("@keyframes localProxyRadarBreathe") &&
    stylesSource.includes(".local-proxy-radar--active") &&
    stylesSource.includes("animation: localProxyRadarBreathe 2.8s ease-in-out infinite") &&
    stylesSource.includes("transform: scale(1.04)") &&
    stylesSource.includes("@media (prefers-reduced-motion: reduce)"),
  "active radar status should breathe slowly and respect reduced motion",
);
