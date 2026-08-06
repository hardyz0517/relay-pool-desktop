import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";

const pageFormSource = await readFile("src/components/ui/PageForm.tsx", "utf8");

assert.match(
  pageFormSource,
  /<form className=\{cn\("flex h-full min-h-0 flex-1 flex-col"/,
  "PageForm should reserve a fixed footer row while its content fills the remaining height",
);
assert.match(
  pageFormSource,
  /<div className="min-h-0 flex-1 overflow-y-auto pb-\[var\(--shell-page-gap\)\] \[scrollbar-width:none\] \[&::-webkit-scrollbar\]:hidden">/,
  "PageForm content should scroll independently so long forms cannot overlap the footer",
);

const footerClassMatch = pageFormSource.match(
  /<div className="(?<className>[^"]+)">\s*\{footer\}/s,
);

assert.ok(
  footerClassMatch?.groups?.className,
  "PageForm should keep a centralized footer class contract",
);

const footerClassName = footerClassMatch.groups.className;

assert.match(
  footerClassName,
  /\bsticky\b/,
  "PageForm footer should remain sticky inside the page scroll container",
);
assert.match(
  footerClassName,
  /\bbottom-0\b/,
  "PageForm footer should anchor to the bottom of its own scroll container",
);
assert.match(
  footerClassName,
  /\bshrink-0\b/,
  "PageForm footer should keep its intrinsic height while the content row absorbs unused viewport space",
);
assert.doesNotMatch(
  footerClassName,
  /\bbottom-\[calc\(var\(--shell-page-gap\)\*-1\)\]/,
  "PageForm footer should not be shifted below the page scroll container",
);
assert.match(
  footerClassName,
  /(?:^|\s)-mx-\[var\(--shell-page-gap\)\](?:\s|$)/,
  "PageForm footer should continue spanning the page gutter horizontally",
);
assert.match(
  footerClassName,
  /(?:^|\s)-mb-\[var\(--shell-page-gap\)\](?:\s|$)/,
  "PageForm footer should continue consuming the page bottom gutter",
);

const collectTsxFiles = async (directory) => {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];

  for (const entry of entries) {
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...(await collectTsxFiles(entryPath)));
      continue;
    }

    if (entry.isFile() && entry.name.endsWith(".tsx")) {
      files.push(entryPath.replaceAll(path.sep, "/"));
    }
  }

  return files;
};

const featureFiles = await collectTsxFiles("src/features");
const pageFormConsumers = [];

for (const file of featureFiles) {
  const source = await readFile(file, "utf8");
  if (source.includes("<PageForm")) {
    pageFormConsumers.push(file);
    assert.ok(
      source.includes("PageForm"),
      `${file} should consume the shared PageForm component`,
    );
    assert.doesNotMatch(
      source,
      /\bbottom-\[calc\(var\(--shell-page-gap\)\*-1\)\]/,
      `${file} should not carry a page-specific negative sticky footer override`,
    );
  }
}

const expectedConsumers = [
  "src/features/channels/ChannelMonitorForm.tsx",
  "src/features/key-pool/AddKeyPage.tsx",
  "src/features/key-pool/EditKeyPage.tsx",
  "src/features/stations/AddProviderPage.tsx",
];

for (const expectedConsumer of expectedConsumers) {
  assert.ok(
    pageFormConsumers.includes(expectedConsumer),
    `${expectedConsumer} should keep using the shared PageForm footer behavior`,
  );
}

assert.ok(
  pageFormConsumers.length >= expectedConsumers.length,
  "PageForm sticky footer coverage should include all known form surfaces",
);

console.log("page form sticky footer contract ok");
