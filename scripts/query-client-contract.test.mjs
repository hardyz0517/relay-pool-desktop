import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const text = async (path) => readFile(new URL(`../${path}`, import.meta.url), "utf8");

async function assertStaticContract() {
  const packageJson = JSON.parse(await text("package.json"));
  assert.equal(
    packageJson.dependencies?.["@tanstack/react-query"],
    "^5.90.3",
    "package.json must depend on @tanstack/react-query ^5.90.3",
  );

  const main = await text("src/main.tsx");
  assert.match(
    main,
    /<QueryClientProvider\s+client=\{queryClient\}>/,
    "src/main.tsx must wrap the app with QueryClientProvider client={queryClient}",
  );
  assert.match(
    main,
    /<QueryErrorNotifier\s*\/>/,
    "src/main.tsx must render QueryErrorNotifier inside ToastProvider",
  );

  const queryClientSource = await text("src/lib/query/queryClient.ts");
  assert.match(queryClientSource, /new\s+QueryClient/, "queryClient.ts must create a QueryClient");
  assert.match(
    queryClientSource,
    /refetchOnWindowFocus:\s*true/,
    "queryClient.ts must enable window-focus refetching",
  );
  assert.match(
    queryClientSource,
    /refetchIntervalInBackground:\s*false/,
    "queryClient.ts must keep interval refetches out of the background",
  );

  const notifierSource = await text("src/lib/query/QueryErrorNotifier.tsx");
  assert.match(
    notifierSource,
    /createQueryErrorNotificationCycle/,
    "QueryErrorNotifier must suppress repeated notifications during one continuous query failure",
  );
  assert.match(
    notifierSource,
    /event\.action\.type === "success"/,
    "QueryErrorNotifier must reset the notification cycle after a successful refresh",
  );
  assert.match(
    notifierSource,
    /toast\.error\(\s*["']数据刷新失败["']/,
    "QueryErrorNotifier must show the generic Chinese refresh failure title",
  );
  assert.doesNotMatch(
    notifierSource,
    /queryKey/,
    "QueryErrorNotifier must not expose or reference queryKey values",
  );

  const queryKeysSource = await text("src/lib/query/queryKeys.ts");
  for (const key of [
    "settings",
    "proxyStatus",
    "requestLogs",
    "stations",
    "stationAssets",
    "keyPool",
    "balanceSnapshots",
    "changeEvents",
    "localRoutingWorkspace",
    "pricing",
    "channelStatus",
  ]) {
    assert.match(queryKeysSource, new RegExp(`\\b${key}\\b`), `queryKeys.ts must define ${key}`);
  }
  assert.match(
    queryKeysSource,
    /\bstationAsset\s*[:=]\s*\(?\s*stationId\b/,
    "queryKeys.ts must define stationAsset(stationId)",
  );
}

async function assertQueryClientBehavior() {
  const { QueryClient } = await import("@tanstack/react-query");

  {
    const client = new QueryClient({
      defaultOptions: {
        queries: {
          retry: false,
        },
      },
    });
    let calls = 0;
    const queryFn = async () => {
      calls += 1;
      await new Promise((resolve) => setTimeout(resolve, 10));
      return { ok: true };
    };

    const [first, second] = await Promise.all([
      client.fetchQuery({ queryKey: ["contract", "dedupe"], queryFn }),
      client.fetchQuery({ queryKey: ["contract", "dedupe"], queryFn }),
    ]);

    assert.equal(calls, 1, "same-key in-flight fetchQuery calls must dedupe");
    assert.deepEqual(first, { ok: true });
    assert.deepEqual(second, { ok: true });
    client.clear();
  }

  {
    const client = new QueryClient({
      defaultOptions: {
        queries: {
          retry: false,
          staleTime: 0,
        },
      },
    });
    const key = ["contract", "last-good"];
    const lastGood = { balance: "12.34" };

    await client.fetchQuery({
      queryKey: key,
      queryFn: async () => lastGood,
    });

    await assert.rejects(
      client.fetchQuery({
        queryKey: key,
        queryFn: async () => {
          throw new Error("backend unavailable");
        },
      }),
      /backend unavailable/,
      "failed refetch must reject",
    );
    assert.deepEqual(
      client.getQueryData(key),
      lastGood,
      "failed refetch must preserve the last-good cached data",
    );
    client.clear();
  }
}

await assertStaticContract();
await assertQueryClientBehavior();
console.log("query client contract passed");
