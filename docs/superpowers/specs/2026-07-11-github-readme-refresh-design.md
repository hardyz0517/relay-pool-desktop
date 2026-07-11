# GitHub README Refresh Design

## Goal

Rewrite the repository README as a professional GitHub-facing product introduction for Relay Pool Desktop. The README should help a new visitor understand the product, its current maturity, its verified capabilities, and how to run it from source without reading internal phase documents first.

## Audience And Language

- Primary audience: Chinese-speaking developers and local AI tool users evaluating the project on GitHub.
- Primary language: Chinese.
- English content: one concise subtitle near the top for international context.
- Product status: explicitly label the project as a technical preview.

## Content Structure

The README will use this order:

1. Product name and concise English subtitle.
2. Technical-preview status notice.
3. Product positioning and the problem it solves.
4. A short explanation of how Relay Pool works with local AI clients and upstream relay services.
5. Verified, currently implemented capabilities grouped by user outcome.
6. Supported service types and explicit project boundaries.
7. Source-based quick start with prerequisites and validated commands.
8. High-level architecture and technology stack.
9. Local data and credential-security expectations.
10. A concise roadmap that links to detailed planning documents instead of repeating phase history.
11. Contribution and license/status information that reflects the repository as it exists today.

## Presentation Rules

- No screenshots or screenshot placeholders in this version.
- No fake download button, release badge, coverage badge, build badge, or stable-version claim.
- Do not present `package.json` version `0.0.0` as a public release version.
- Do not claim that an installer or packaged release exists while Tauri bundling is disabled.
- Avoid long phase-by-phase completion logs on the GitHub landing page.
- Prefer short sections, scannable capability lists, and one compact workflow diagram written in Markdown text rather than decorative HTML.
- Link to detailed documents only when they help users continue reading.

## Truthfulness Boundary

Capability claims must be supported by the current source tree or current project documentation. The README may describe the implemented local OpenAI-compatible gateway, station and key management, collection, routing and fallback, pricing and balance views, channel monitoring, request logs, local persistence, and credential protections only at the level currently present in the repository.

Planned or incomplete work must be labeled as roadmap material. The README must not imply stable releases, broad production readiness, cloud services, account systems, team features, or full compatibility with every upstream relay implementation.

## Files And Scope

- Modify: `README.md`
- Create for design traceability: `docs/superpowers/specs/2026-07-11-github-readme-refresh-design.md`
- Do not modify application code, packaging configuration, release automation, screenshots, or unrelated documentation.

## Verification

- Check every relative README link resolves to a tracked repository path.
- Check all documented commands exist in `package.json`.
- Check Markdown heading order, fenced code blocks, and relative links mechanically.
- Review the final diff to confirm only README content and the approved design record are involved.
- Confirm the final wording clearly distinguishes technical-preview capabilities from roadmap work.
