import assert from "node:assert/strict";
import test from "node:test";

const modulePath = new URL("../src/lib/query/queryErrorNotificationCycle.ts", import.meta.url);

test("a continuously failing query notifies only once", async () => {
  const { createQueryErrorNotificationCycle } = await import(modulePath.href);
  const cycle = createQueryErrorNotificationCycle();

  assert.equal(cycle.shouldNotify("request-logs"), true);
  assert.equal(cycle.shouldNotify("request-logs"), false);
  assert.equal(cycle.shouldNotify("request-logs"), false);
});

test("a successful refresh enables notification for the next failure", async () => {
  const { createQueryErrorNotificationCycle } = await import(modulePath.href);
  const cycle = createQueryErrorNotificationCycle();

  assert.equal(cycle.shouldNotify("request-logs"), true);
  cycle.reset("request-logs");
  assert.equal(cycle.shouldNotify("request-logs"), true);
});
