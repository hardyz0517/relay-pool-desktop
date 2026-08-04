import { readFile } from "node:fs/promises";
import { request } from "node:https";
import { resolve } from "node:path";
import { spawnSync } from "node:child_process";
import { pathToFileURL } from "node:url";

const REPO_ROOT = resolve(import.meta.dirname, "..");

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: REPO_ROOT,
    encoding: "utf8",
    shell: false,
    ...options,
  });
  if (result.status !== 0) {
    const detail = result.stderr?.trim() || result.stdout?.trim() || result.error?.message;
    throw new Error(`${command} ${args.join(" ")} failed${detail ? `: ${detail}` : ""}`);
  }
  if (result.stdout?.trim()) process.stdout.write(result.stdout);
  if (result.stderr?.trim()) process.stderr.write(result.stderr);
  return result.stdout;
}

async function assertReleaseNotes() {
  const pkg = JSON.parse(await readFile(resolve(REPO_ROOT, "package.json"), "utf8"));
  const notesPath = resolve(REPO_ROOT, "docs", "release", `v${pkg.version}.md`);
  const notes = await readFile(notesPath, "utf8");
  if (!notes.trim()) throw new Error(`release notes are empty: ${notesPath}`);
  console.log(`release notes verified: docs/release/v${pkg.version}.md`);
}

function githubJson(path, token) {
  return new Promise((resolvePromise, reject) => {
    const req = request(
      {
        hostname: "api.github.com",
        path,
        method: "GET",
        headers: {
          Accept: "application/vnd.github+json",
          Authorization: `Bearer ${token}`,
          "User-Agent": "relay-pool-desktop-release-preflight",
          "X-GitHub-Api-Version": "2022-11-28",
        },
      },
      (res) => {
        let body = "";
        res.setEncoding("utf8");
        res.on("data", (chunk) => {
          body += chunk;
        });
        res.on("end", () => {
          if (res.statusCode < 200 || res.statusCode >= 300) {
            reject(new Error(`GitHub API ${res.statusCode}: ${body}`));
            return;
          }
          try {
            resolvePromise(JSON.parse(body));
          } catch (error) {
            reject(new Error(`GitHub API returned invalid JSON: ${error.message}`));
          }
        });
      },
    );
    req.on("error", reject);
    req.end();
  });
}

async function assertQualifiedByCi({ workflow, branch }) {
  const repository = process.env.GITHUB_REPOSITORY;
  const sha = process.env.GITHUB_SHA;
  const token = process.env.GITHUB_TOKEN;
  if (!repository) throw new Error("GITHUB_REPOSITORY is required to verify release CI qualification");
  if (!sha) throw new Error("GITHUB_SHA is required to verify release CI qualification");
  if (!token) throw new Error("GITHUB_TOKEN is required to verify release CI qualification");

  const query = new URLSearchParams({
    head_sha: sha,
    event: "push",
    per_page: "20",
  });
  const data = await githubJson(
    `/repos/${repository}/actions/workflows/${encodeURIComponent(workflow)}/runs?${query}`,
    token,
  );
  const successfulRun = data.workflow_runs?.find(
    (run) =>
      run.head_sha === sha &&
      run.head_branch === branch &&
      run.status === "completed" &&
      run.conclusion === "success",
  );
  if (!successfulRun) {
    throw new Error(
      `release commit ${sha} is not qualified: no successful ${workflow} push run on ${branch}`,
    );
  }
  console.log(`release CI qualification verified: ${workflow} #${successfulRun.run_number}`);
}

async function main(argv) {
  let requireCi = false;
  let workflow = "ci.yml";
  let branch = "master";

  for (let index = 0; index < argv.length; index += 1) {
    const argument = argv[index];
    const next = () => {
      const value = argv[++index];
      if (!value) throw new Error(`${argument} requires a value`);
      return value;
    };
    if (argument === "--require-ci") requireCi = true;
    else if (argument === "--workflow") workflow = next();
    else if (argument === "--branch") branch = next();
    else throw new Error(`unknown argument: ${argument}`);
  }

  run(process.execPath, ["scripts/verify-release-version.mjs", "--require-tag"], {
    env: process.env,
  });
  await assertReleaseNotes();
  if (requireCi) await assertQualifiedByCi({ workflow, branch });
  console.log("release preflight verified");
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(error.message);
    process.exitCode = 1;
  });
}
