# GitHub README Refresh Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the outdated phase-oriented README with a truthful, polished GitHub product page for the Relay Pool Desktop technical preview.

**Architecture:** Keep the change documentation-only and centered on `README.md`. Derive product claims from current code and canonical project documents, then mechanically validate commands, links, Markdown structure, and scope before closeout.

**Tech Stack:** GitHub Flavored Markdown, PowerShell verification, pnpm package metadata, Tauri 2 / React / TypeScript project documentation

---

### Task 1: Rewrite The Product Narrative

**Files:**
- Modify: `README.md`
- Reference: `docs/PROJECT_PLAN.md`
- Reference: `docs/PRODUCT_MODEL.md`
- Reference: `docs/SECURITY_EXPORT_IMPORT.md`
- Reference: `package.json`
- Reference: `src-tauri/tauri.conf.json`

- [ ] **Step 1: Replace the phase-log opening with a product header**

Write a Chinese-first header containing the product name, one concise English subtitle, and a visible technical-preview notice. State that Relay Pool Desktop is a local desktop relay and key-pool manager that exposes a stable local OpenAI-compatible entry point while managing multiple upstream relay services.

- [ ] **Step 2: Explain the product workflow**

Add a compact text flow showing local clients such as Codex, Claude Code, Gemini CLI, or CCSwitch connecting to Relay Pool Desktop, which then selects an eligible upstream Station Key using capability, health, priority, and pricing facts.

- [ ] **Step 3: Group verified capabilities by user outcome**

Document currently implemented station and key management, Sub2API / NewAPI / OpenAI-compatible collection adapters, local gateway and fallback routing, pricing and balance facts, channel monitoring, request logs, SQLite persistence, and local credential protection. Avoid claiming universal upstream compatibility or stable production readiness.

- [ ] **Step 4: Add source-based quick start and project boundaries**

Document Node.js, pnpm, Rust, Tauri system prerequisites, then use only the existing commands `pnpm install`, `pnpm tauri:dev`, `pnpm build`, and `pnpm tauri:build`. Explicitly state that there is no stable downloadable release yet and that the project does not include accounts, payments, cloud sync, or multi-user administration.

- [ ] **Step 5: Add architecture, security, roadmap, and contribution sections**

Summarize the React/Tauri/SQLite architecture, link to security and planning documents, keep future work labeled as roadmap, and invite issues or pull requests without inventing a license declaration when the repository has no license file.

### Task 2: Verify README Integrity

**Files:**
- Verify: `README.md`
- Verify: `package.json`

- [ ] **Step 1: Check package scripts referenced by the README**

Run:

```powershell
$readme = Get-Content -Raw README.md
$package = Get-Content -Raw package.json | ConvertFrom-Json
@('dev', 'build', 'tauri:dev', 'tauri:build') | ForEach-Object {
  if (-not $package.scripts.PSObject.Properties.Name.Contains($_)) { throw "Missing package script: $_" }
}
```

Expected: exit code `0` with no missing-script error.

- [ ] **Step 2: Check all relative Markdown links**

Run:

```powershell
$content = Get-Content -Raw README.md
$links = [regex]::Matches($content, '\[[^\]]+\]\((?!https?://|#)([^)]+)\)')
foreach ($link in $links) {
  $path = [uri]::UnescapeDataString($link.Groups[1].Value.Split('#')[0])
  if ($path -and -not (Test-Path -LiteralPath $path)) { throw "Broken README link: $path" }
}
```

Expected: exit code `0` with no broken-link error.

- [ ] **Step 3: Check Markdown fences and whitespace**

Run:

```powershell
$content = Get-Content -Raw README.md
$fenceCount = ([regex]::Matches($content, '(?m)^```')).Count
if ($fenceCount % 2 -ne 0) { throw 'Unbalanced Markdown code fences' }
git diff --check -- README.md
```

Expected: exit code `0`, balanced fences, and no whitespace errors.

- [ ] **Step 4: Review truthfulness and scope**

Run:

```powershell
rg -n "稳定版|生产就绪|下载安装|Release|Phase [0-9]|P[0-9] 已完成|SaaS|账号系统|云同步" README.md
git diff -- README.md
git status --short -- README.md docs/superpowers/specs/2026-07-11-github-readme-refresh-design.md docs/superpowers/plans/2026-07-11-github-readme-refresh.md
```

Expected: any `rg` hits are explicit limitations or roadmap context, the diff contains only the approved README rewrite, and unrelated working-tree changes remain untouched.
