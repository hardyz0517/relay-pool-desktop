import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile("src/components/shell/PageScaffold.tsx", "utf8");

const stickyHeaderClassMatch = source.match(
  /stickyHeader &&\s*"(?<className>[^"]+)"/,
);

assert.ok(
  stickyHeaderClassMatch?.groups?.className,
  "PageScaffold should keep a centralized sticky header class contract",
);

const stickyHeaderClassName = stickyHeaderClassMatch.groups.className;

assert.match(
  stickyHeaderClassName,
  /\bsticky\b/,
  "sticky page headers should remain fixed inside the page scroll container",
);
assert.match(
  stickyHeaderClassName,
  /\btop-0\b/,
  "sticky page headers should anchor at the top of their own scroll container",
);
assert.doesNotMatch(
  stickyHeaderClassName,
  /\btop-\[calc\(var\(--shell-page-gap\)\*-1\)\]/,
  "sticky page headers should not bleed above the page padding",
);
assert.match(
  stickyHeaderClassName,
  /(?:^|\s)-mt-\[var\(--shell-page-gap\)\](?:\s|$)/,
  "sticky page headers should consume the inner content wrapper's top gutter",
);
assert.match(
  stickyHeaderClassName,
  /(?:^|\s)-mx-\[var\(--shell-page-gap\)\](?:\s|$)/,
  "sticky page headers should continue spanning the page gutter horizontally",
);

console.log("page scaffold sticky header contract ok");
