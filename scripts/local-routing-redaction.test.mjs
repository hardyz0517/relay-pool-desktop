import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const targets = [
  resolve("src/lib/types/localRouting.ts"),
  resolve("src-tauri/src/services/proxy/routing_types.rs"),
];

const requiredSubstrings = [
  "export type LocalRoutingWorkspace =",
  "proxyStatus: ProxyStatus",
  "candidates: LocalRoutingCandidateRow[]",
  "latestDecision: RouteDecisionSummary | null",
];

const forbiddenSubstrings = [
  "apiKey:",
  "api_key:",
  "api_key",
  "authorization:",
  "authorization",
  "cookie:",
  "cookie",
  "setCookie:",
  "set_cookie:",
  "set_cookie",
  "rawBody:",
  "raw_body:",
  "raw_body",
  "requestBody:",
  "request_body:",
  "request_body",
  "upstreamErrorBody:",
  "upstream_error_body:",
  "upstream_error_body",
];

const sources = targets.map((target) => ({
  target,
  source: readFileSync(target, "utf8"),
}));

for (const text of requiredSubstrings) {
  const typeSource = sources[0].source;
  if (!typeSource.includes(text)) {
    throw new Error(`Missing required local routing type contract text: ${text}`);
  }
}

for (const { target, source } of sources) {
  for (const text of forbiddenSubstrings) {
    if (source.includes(text)) {
      throw new Error(`Forbidden raw or secret-bearing field present in ${target}: ${text}`);
    }
  }
}

console.log("local routing redaction type contract ok");
