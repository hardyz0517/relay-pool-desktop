import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const targets = [
  resolve("src/lib/types/localRouting.ts"),
  resolve("src-tauri/src/ipc/dto/proxy_workspace_reads.typescript.txt"),
];

const requestLogTargets = [
  {
    target: resolve("src-tauri/src/application/request_finalization/mod.rs"),
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

const executionTargetContracts = [
  {
    target: resolve("src-tauri/src/application/operational_facts/target_resolver.rs"),
    required: [
      "pub(crate) struct ExecutionTargetHandle",
      "impl fmt::Debug for ExecutionTargetHandle",
      '.field("api_key", &"<redacted>")',
      "pub(crate) lease: CapacityLease",
      "pub(crate) api_key: SecretBytes",
    ],
    forbidden: [
      "Serialize",
      "Deserialize",
      "Bearer ",
      "authorization",
      "cookie",
    ],
  },
  {
    target: resolve("src-tauri/src/persistence/stores/routing_store.rs"),
    required: [
      "load_operational_execution_target_refs",
      "k.api_key_secret_id",
      "CASE WHEN TRIM(k.api_key) != '' THEN 1 ELSE 0 END AS inline_api_key_present",
    ],
    forbidden: [
      "SELECT k.api_key,",
      "SELECT\n                k.api_key",
      "api_key AS",
    ],
  },
];

const requestLogSanitizerContracts = [
  {
    target: resolve("src-tauri/src/persistence/migrations/0018_request_log_url_sanitizer.sql"),
    required: [
      "CREATE TABLE IF NOT EXISTS request_log_url_sanitizer_progress",
      "request_logs_upstream_base_url_v1",
      "UPDATE persistence_schema_compatibility",
      "SET schema_version = 18",
    ],
    forbidden: [
      "UPDATE schema_compatibility",
      "upstream_base_url TEXT",
    ],
  },
  {
    target: resolve("src-tauri/src/persistence/maintenance/request_log_url_sanitizer.rs"),
    required: [
      "pub(crate) fn sanitize_legacy_upstream_url",
      "pub(crate) fn sanitize_legacy_upstream_url_bytes",
      "Url::parse(input.trim())",
      "CAST(upstream_base_url AS BLOB)",
      "url.set_query(None)",
      "url.set_fragment(None)",
      "url.set_path(\"\")",
      "SET upstream_base_url = NULL",
      "PRAGMA wal_checkpoint(TRUNCATE)",
      "VACUUM",
      "request_log_url_sanitizer_progress",
    ],
    forbidden: [
      "SET upstream_base_url = ?",
      "set_query(Some",
      "set_fragment(Some",
    ],
  },
  {
    target: resolve("src-tauri/src/persistence/migrations.rs"),
    required: [
      "sanitize_request_log_upstream_urls(&pool, RequestLogUrlSanitizerOptions::default()).await?",
      "sanitize_request_log_upstream_urls_before_schema18",
      "if (5..18).contains(&schema_version)",
      "if schema_version >= 18",
      "readable_schema: 1..=18",
      "writable_schema: BTreeSet::from([18])",
    ],
    forbidden: [],
  },
  {
    target: resolve("src-tauri/src/persistence/runtime.rs"),
    required: [
      "if sqlx_version >= 18",
      "assert_request_log_url_sanitizer_complete_on_connection",
    ],
    forbidden: [],
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

for (const contract of executionTargetContracts) {
  const source = readFileSync(contract.target, "utf8");
  for (const text of contract.required) {
    if (!source.includes(text)) {
      throw new Error(`Missing execution-target redaction contract text in ${contract.target}: ${text}`);
    }
  }
  for (const text of contract.forbidden) {
    if (source.includes(text)) {
      throw new Error(`Forbidden execution-target secret/serialization text in ${contract.target}: ${text}`);
    }
  }
}

for (const contract of requestLogSanitizerContracts) {
  const source = readFileSync(contract.target, "utf8");
  for (const text of contract.required) {
    if (!source.includes(text)) {
      throw new Error(`Missing request-log sanitizer contract text in ${contract.target}: ${text}`);
    }
  }
  for (const text of contract.forbidden) {
    if (source.includes(text)) {
      throw new Error(`Forbidden request-log sanitizer contract text in ${contract.target}: ${text}`);
    }
  }
}

console.log("local routing redaction type contract ok");
