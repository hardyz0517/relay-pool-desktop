import assert from "node:assert/strict";
import test from "node:test";

const modulePath = new URL("../src/lib/api/updaterCheckCoordinator.ts", import.meta.url);

test("passes the detected proxy to the authoritative native check", async () => {
  const { coordinateUpdateCheck } = await import(modulePath.href);
  let receivedProxy: string | null = null;

  const result = await coordinateUpdateCheck({
    currentVersion: "0.2.2",
    proxyUrl: "http://127.0.0.1:7890",
    checkNative: async (proxyUrl) => {
      receivedProxy = proxyUrl;
      return null;
    },
    inspectPublished: async () => {
      throw new Error("fallback must not run");
    },
  });

  assert.equal(receivedProxy, "http://127.0.0.1:7890");
  assert.deepEqual(result, { kind: "current", currentVersion: "0.2.2" });
});

test("returns the native resource when an installable update is available", async () => {
  const { coordinateUpdateCheck } = await import(modulePath.href);
  const update = {
    currentVersion: "0.2.2",
    version: "0.2.3",
    body: "Fixes",
  };

  const result = await coordinateUpdateCheck({
    currentVersion: "0.2.2",
    proxyUrl: null,
    checkNative: async () => update,
    inspectPublished: async () => {
      throw new Error("fallback must not run");
    },
  });

  assert.equal(result.kind, "available");
  assert.equal(result.kind === "available" && result.update, update);
});

test("uses a same-or-older fallback only to prove the app is current", async () => {
  const { coordinateUpdateCheck } = await import(modulePath.href);

  const result = await coordinateUpdateCheck({
    currentVersion: "0.2.2",
    proxyUrl: null,
    checkNative: async () => {
      throw new Error("native network failure");
    },
    inspectPublished: async () => ({
      relation: "current_or_older" as const,
      version: "0.2.2",
      notes: null,
    }),
  });

  assert.deepEqual(result, { kind: "current", currentVersion: "0.2.2" });
});

test("never turns a manifest-only newer version into an installable update", async () => {
  const {
    coordinateUpdateCheck,
    ManifestNewerButNativeUnavailableError,
  } = await import(modulePath.href);
  const nativeError = new Error("native network failure");

  await assert.rejects(
    coordinateUpdateCheck({
      currentVersion: "0.2.2",
      proxyUrl: null,
      checkNative: async () => {
        throw nativeError;
      },
      inspectPublished: async () => ({
        relation: "newer" as const,
        version: "0.2.3",
        notes: "Fixes",
      }),
    }),
    (error) =>
      error instanceof ManifestNewerButNativeUnavailableError &&
      error.publishedVersion === "0.2.3" &&
      error.nativeError === nativeError,
  );
});

test("preserves the native failure when fallback inspection also fails", async () => {
  const { coordinateUpdateCheck } = await import(modulePath.href);
  const nativeError = new Error("native network failure");

  await assert.rejects(
    coordinateUpdateCheck({
      currentVersion: "0.2.2",
      proxyUrl: null,
      checkNative: async () => {
        throw nativeError;
      },
      inspectPublished: async () => {
        throw new Error("fallback network failure");
      },
    }),
    (error) => error === nativeError,
  );
});
