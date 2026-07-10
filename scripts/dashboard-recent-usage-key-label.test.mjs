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
