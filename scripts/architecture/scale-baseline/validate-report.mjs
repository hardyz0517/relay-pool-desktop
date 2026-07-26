import fs from "node:fs";
import path from "node:path";
import { assert, repoRoot, runMain } from "../lib.mjs";
import { canonicalJson, DATASET_SIZES, generateDataset, sha256 } from "./dataset.mjs";

function argument(name) {
  const index = process.argv.indexOf(name);
  assert(index >= 0 && process.argv[index + 1], `${name} <path> is required`);
  return path.resolve(repoRoot, process.argv[index + 1]);
}

runMain(() => {
  const reportPath = argument("--report");
  const fixturePath = argument("--fixtures");
  assert(reportPath.startsWith(path.join(repoRoot, "output") + path.sep), "report must be under output/");
  const report = JSON.parse(fs.readFileSync(reportPath, "utf8"));
  const fixtures = JSON.parse(fs.readFileSync(fixturePath, "utf8"));
  assert(report.schema_version === 1, "scale report schema_version must be 1");
  assert(report.qualification === "frontend-jsdom-current-query-topology-baseline-only", "scale report must identify the current jsdom query-topology boundary and not claim native/release qualification");
  assert(typeof report.provenance?.source_revision === "string" && /^[a-f0-9]{40}$/.test(report.provenance.source_revision), "scale report requires an exact Git revision");
  assert(report.method?.warmup_runs === 5 && report.method?.sample_runs === 30, "scale report method differs from capacity ledger");
  for (const size of DATASET_SIZES) {
    const expected = generateDataset(size);
    const expectedHash = sha256(canonicalJson(expected));
    assert(fixtures.datasets?.[size]?.sha256 === expectedHash, `fixture manifest hash mismatch for ${size}`);
    const measured = report.datasets?.[size];
    assert(measured?.fixture_sha256 === expectedHash, `report fixture hash mismatch for ${size}`);
    assert(measured.sample_runs === 30 && measured.samples?.length === 30, `dataset ${size} requires 30 raw samples`);
    for (const [index, sample] of measured.samples.entries()) {
      assert(sample.invoke_count === 4, `dataset ${size} sample ${index} must preserve the current bounded 4-command topology`);
      assert(sample.commands?.filter((call) => call.command === "list_stations").length === 1, `dataset ${size} sample ${index} lacks one station list command`);
      assert(sample.commands?.filter((call) => call.command === "list_latest_collector_snapshots").length === 1, `dataset ${size} sample ${index} lacks the aggregate latest-snapshot command`);
      assert(sample.commands?.filter((call) => call.command === "get_latest_collector_snapshot").length === 0, `dataset ${size} sample ${index} must not use the legacy per-station snapshot command`);
      assert(sample.projected_response_json_bytes > 0, `dataset ${size} sample ${index} projected bytes must be measured`);
      assert(sample.query_lifecycle?.some((event) => event.status === "success"), `dataset ${size} sample ${index} lacks Query success lifecycle`);
      assert(sample.react_profiler_commits?.length > 0, `dataset ${size} sample ${index} lacks React Profiler commits`);
      assert(sample.rendered_row_count === size, `dataset ${size} sample ${index} rendered row count drifted`);
      assert(Number.isFinite(sample.data_ready_commit_ms) && sample.data_ready_commit_ms >= 0, `dataset ${size} sample ${index} has invalid data-ready time`);
    }
    assert(measured.hidden_query_start_count === 0, `dataset ${size} hidden topology started work`);
    assert(measured.summary?.invoke_count?.min === 4 && measured.summary?.invoke_count?.max === 4, `dataset ${size} invoke summary drifted`);
    assert(Number.isFinite(measured.summary?.data_ready_commit_ms?.p50) && Number.isFinite(measured.summary?.data_ready_commit_ms?.p95), `dataset ${size} requires p50/p95 commit statistics`);
  }
  const expectedOwners = {
    backend_read_port_round_trips: "Task 11",
    backend_sql_statement_count_runtime: "Task 11",
    backend_query_duration_ms: "Task 11",
    real_tauri_ipc_payload_bytes: "Task 26",
    real_tauri_command_duration_ms: "Task 26",
    webview2_page_commit_ms: "Task 26",
  };
  for (const [name, owner] of Object.entries(expectedOwners)) {
    const metric = report.blocked_metrics?.[name];
    assert(metric?.value === null, `${name} must remain null until measured by its runtime owner`);
    assert(metric?.qualification === "blocked" && metric.owner === owner, `${name} requires blocked owner ${owner}`);
    assert(metric.owner_task === Number(owner.replace("Task ", "")) && metric.release_gate === 26, `${name} requires numeric owner_task and release_gate`);
    assert(typeof metric.reason === "string" && metric.reason.trim(), `${name} requires a blocking reason`);
  }
  console.log("Frontend scale baseline report passed fail-closed validation");
});
