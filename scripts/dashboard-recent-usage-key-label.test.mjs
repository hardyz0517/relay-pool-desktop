import { readFile } from "node:fs/promises";

const dashboardSource = await readFile("src/features/dashboard/DashboardPage.tsx", "utf8");

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}

assert(
  dashboardSource.includes("requestKeyNameById") &&
    dashboardSource.includes("request.stationKeyId") &&
    dashboardSource.includes("Key："),
  "dashboard recent usage rows should name the station key that handled each request",
);

assert(
  /<div className="flex min-w-0 items-baseline gap-2">[\s\S]*?\{request\.model \?\? request\.path\}[\s\S]*?Key：\{requestKeyName\}[\s\S]*?<\/div>\s*<div className="mt-0\.5 truncate text-xs text-slate-500">\s*\{formatDateTime\(request\.startedAt\)\}/.test(
    dashboardSource,
  ),
  "dashboard recent usage rows should show the key inline after the model and keep time on the next line",
);
