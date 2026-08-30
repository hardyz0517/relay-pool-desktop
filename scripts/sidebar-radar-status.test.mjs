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
    radarIconSource.includes("local-proxy-globe") &&
    radarIconSource.includes("local-proxy-globe--active") &&
    radarIconSource.includes("IntersectionObserver") &&
    radarIconSource.includes("visibilitychange"),
  "proxy status icon should expose the frozen globe sprite and pause when hidden",
);

assert.ok(
    stylesSource.includes("@keyframes localProxyGlobeSpin") &&
    stylesSource.includes(".local-proxy-globe--active") &&
    stylesSource.includes("animation: localProxyGlobeSpin 2000ms steps(16, end) infinite") &&
    stylesSource.includes("background-image: var(--local-proxy-globe-static-light)") &&
    stylesSource.includes("background-image: var(--local-proxy-globe-sprite-light)") &&
    stylesSource.includes("background-image: var(--local-proxy-globe-static-dark)") &&
    stylesSource.includes(".dark .local-proxy-globe") &&
    stylesSource.includes("@media (prefers-reduced-motion: reduce)"),
  "active globe status should use the frozen sprite with CSS steps and respect reduced motion",
);

assert.ok(!radarIconSource.includes("<svg") && !stylesSource.includes("local-proxy-radar"), "the old radar implementation should be removed");
