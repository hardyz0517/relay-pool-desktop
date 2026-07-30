import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const targets = [
  resolve("src/lib/types/localRouting.ts"),
  resolve("src-tauri/src/ipc/dto/proxy_workspace_reads.typescript.txt"),
];

const requestLogTargets = [
  {
    target: resolve("src-tauri/src/application/request_finalization.rs"),
    required: [
      "upstream_base_url: None",
      "request_terminal_mapping_preserves_safe_annotations_and_redacts_upstream_base_url",
    ],
    forbidden: ["upstream_base_url: annotations.upstream_base_url"],
  },
  {
    target: resolve("src-tauri/src/persistence/stores/request_log_store.rs"),
    required: ["Option::<&str>::None", "upstream_base_url = ?"],
    forbidden: [".bind(record.annotations.upstream_base_url.as_deref())"],
  },
  {
    target: resolve("src-tauri/src/ipc/dto/change_logs.rs"),
    required: ["upstream_base_url: None"],
    forbidden: ['upstream_base_url: Some("https://provider.invalid/v1".into())'],
  },
  {
    target: resolve("src/features/logs/LogsPage.tsx"),
    required: ["selected.upstreamBaseUrl ??"],
    forbidden: ["station.apiBaseUrl", "websiteUrl", "apiBaseUrl"],
  },
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

for (const contract of requestLogTargets) {
  const source = readFileSync(contract.target, "utf8");
  for (const text of contract.required) {
    if (!source.includes(text)) {
      throw new Error(`Missing request-log redaction contract text in ${contract.target}: ${text}`);
    }
  }
  for (const text of contract.forbidden) {
    if (source.includes(text)) {
      throw new Error(`Forbidden request-log URL backfill/full-URL persistence text in ${contract.target}: ${text}`);
    }
  }
}

console.log("local routing redaction type contract ok");
