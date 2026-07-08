import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const target = resolve("src/lib/types/localRouting.ts");
const source = readFileSync(target, "utf8");

const requiredSubstrings = [
  "export type LocalRoutingWorkspace =",
  "proxyStatus: ProxyStatus",
  "candidates: LocalRoutingCandidateRow[]",
  "latestDecision: RouteDecisionSummary | null",
];

const forbiddenSubstrings = [
  "apiKey:",
  "api_key:",
  "authorization:",
  "cookie:",
  "setCookie:",
  "rawBody:",
  "requestBody:",
  "upstreamErrorBody:",
];

for (const text of requiredSubstrings) {
  if (!source.includes(text)) {
    throw new Error(`Missing required local routing type contract text: ${text}`);
  }
}

for (const text of forbiddenSubstrings) {
  if (source.includes(text)) {
    throw new Error(`Forbidden raw or secret-bearing field present: ${text}`);
  }
}

console.log("local routing redaction type contract ok");
