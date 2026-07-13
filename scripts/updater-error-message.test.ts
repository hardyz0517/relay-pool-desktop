import assert from "node:assert/strict";
import test from "node:test";

const modulePath = new URL("../src/lib/api/updaterErrors.ts", import.meta.url);

test("network update check errors are shown as actionable Chinese text", async () => {
  const { normalizeUpdaterError } = await import(modulePath.href);

  assert.equal(
    normalizeUpdaterError(new Error("error sending request for url (https://github.com/hardyz0517/relay-pool-desktop/releases/latest/download/latest.json)")),
    "\u65e0\u6cd5\u8fde\u63a5 GitHub \u66f4\u65b0\u6e90\uff0c\u8bf7\u68c0\u67e5\u7f51\u7edc\u6216 Windows \u7cfb\u7edf\u4ee3\u7406\u540e\u91cd\u8bd5\u3002",
  );
});

test("missing latest.json update asset is shown as not published yet", async () => {
  const { normalizeUpdaterError } = await import(modulePath.href);

  assert.equal(
    normalizeUpdaterError("server returned 404 for latest.json"),
    "\u68c0\u67e5\u66f4\u65b0\u5931\u8d25\uff1a\u5f53\u524d\u8fd8\u6ca1\u6709\u53d1\u5e03\u53ef\u7528\u7684\u66f4\u65b0\u6587\u4ef6\u3002",
  );
});

test("newer manifest without a native resource has actionable Chinese text", async () => {
  const { normalizeUpdaterError } = await import(modulePath.href);

  assert.equal(
    normalizeUpdaterError({
      code: "manifest-newer-native-unavailable",
      publishedVersion: "0.2.3",
    }),
    "\u53d1\u73b0\u65b0\u7248\u672c 0.2.3\uff0c\u4f46\u66f4\u65b0\u5668\u65e0\u6cd5\u51c6\u5907\u4e0b\u8f7d\uff1b\u8bf7\u68c0\u67e5\u7f51\u7edc\u6216 Windows \u7cfb\u7edf\u4ee3\u7406\u540e\u91cd\u8bd5\u3002",
  );
});
