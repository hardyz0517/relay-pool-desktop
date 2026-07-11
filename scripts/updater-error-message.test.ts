import assert from "node:assert/strict";
import test from "node:test";

const modulePath = new URL("../src/lib/api/updaterErrors.ts", import.meta.url);

test("network update check errors are shown as actionable Chinese text", async () => {
  const { normalizeUpdaterError } = await import(modulePath.href);

  assert.equal(
    normalizeUpdaterError(new Error("error sending request for url (https://github.com/hardyz0517/relay-pool-desktop/releases/latest/download/latest.json)")),
    "\u68c0\u67e5\u66f4\u65b0\u672a\u5b8c\u6210\uff1a\u65e0\u6cd5\u8bfb\u53d6 GitHub \u66f4\u65b0\u6e90\uff1b\u5982\u679c\u66f4\u65b0\u65e5\u5fd7\u4e0e\u5f53\u524d\u7248\u672c\u4e00\u81f4\uff0c\u8bf4\u660e\u5df2\u662f\u6700\u65b0\u7248\u672c\u3002",
  );
});

test("missing latest.json update asset is shown as not published yet", async () => {
  const { normalizeUpdaterError } = await import(modulePath.href);

  assert.equal(
    normalizeUpdaterError("server returned 404 for latest.json"),
    "\u68c0\u67e5\u66f4\u65b0\u5931\u8d25\uff1a\u5f53\u524d\u8fd8\u6ca1\u6709\u53d1\u5e03\u53ef\u7528\u7684\u66f4\u65b0\u6587\u4ef6\u3002",
  );
});
