import { spawn } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

class Cdp {
  constructor(webSocketDebuggerUrl) {
    this.ws = new WebSocket(webSocketDebuggerUrl);
    this.nextId = 1;
    this.pending = new Map();
    this.ws.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      if (message.id && this.pending.has(message.id)) {
        const { resolve, reject } = this.pending.get(message.id);
        this.pending.delete(message.id);
        if (message.error) reject(new Error(JSON.stringify(message.error)));
        else resolve(message.result);
      }
    });
  }

  async open() {
    if (this.ws.readyState === WebSocket.OPEN) return;
    await new Promise((resolve, reject) => {
      this.ws.addEventListener("open", resolve, { once: true });
      this.ws.addEventListener("error", reject, { once: true });
    });
  }

  send(method, params = {}) {
    const id = this.nextId++;
    const promise = new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
    });
    this.ws.send(JSON.stringify({ id, method, params }));
    return promise;
  }

  close() {
    this.ws.close();
  }
}

const repo = process.cwd();
const profileName = process.env.RELAY_POOL_ROUTING_CDP_PROFILE ?? "task23-routing-workspace-cdp";
const profileRoot = join(repo, "output", "manual-routing-workspace", profileName);
const evidenceDir = join(profileRoot, "evidence");
const appData = join(profileRoot, "AppData", "Roaming");
const localAppData = join(profileRoot, "AppData", "Local");
const tempDir = join(profileRoot, "Temp");

const vitePort = parsePort(process.env.RELAY_POOL_ROUTING_CDP_VITE_PORT ?? "1431", "Vite");
const debugPort = parsePort(process.env.RELAY_POOL_ROUTING_CDP_DEBUG_PORT ?? "9236", "WebView2 debug");
const fixturePort = parsePort(process.env.RELAY_POOL_ROUTING_FIXTURE_PORT ?? "18181", "fixture");
const appIdentifier =
  process.env.RELAY_POOL_ROUTING_CDP_APP_IDENTIFIER ?? "dev.relaypool.desktop.routing-workspace-cdp";

for (const dir of [appData, localAppData, tempDir, evidenceDir]) {
  mkdirSync(dir, { recursive: true });
}

const configPath = join(profileRoot, "tauri-dev-overlay.json");
writeFileSync(
  configPath,
  JSON.stringify(
    {
      identifier: appIdentifier,
      build: {
        devUrl: `http://127.0.0.1:${vitePort}`,
        beforeDevCommand: `pnpm dev --port ${vitePort} --strictPort`,
      },
    },
    null,
    2,
  ),
);

const fixture = spawn(process.execPath, ["scripts/routing-workspace-fixture-server.mjs"], {
  cwd: repo,
  env: { ...process.env, RELAY_POOL_ROUTING_FIXTURE_PORT: String(fixturePort) },
  stdio: ["ignore", "pipe", "pipe"],
});
let fixtureLog = "";
for (const stream of [fixture.stdout, fixture.stderr]) {
  stream.setEncoding("utf8");
  stream.on("data", (chunk) => {
    fixtureLog = tail(fixtureLog + chunk, 5_000);
  });
}

const tauri = spawn("cmd.exe", ["/d", "/s", "/c", `pnpm.cmd tauri dev --config ${configPath}`], {
  cwd: repo,
  env: {
    ...process.env,
    APPDATA: appData,
    LOCALAPPDATA: localAppData,
    TEMP: tempDir,
    TMP: tempDir,
    RELAY_POOL_DEV_AUTO_START_PROXY: "0",
    RELAY_POOL_START_PROXY_ON_LAUNCH: "0",
    RUSTUP_TOOLCHAIN: process.env.RUSTUP_TOOLCHAIN ?? "1.95.0",
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${debugPort}`,
  },
  stdio: ["ignore", "pipe", "pipe"],
});
let tauriLog = "";
for (const stream of [tauri.stdout, tauri.stderr]) {
  stream.setEncoding("utf8");
  stream.on("data", (chunk) => {
    tauriLog = tail(tauriLog + chunk, 30_000);
  });
}

try {
  await waitJson(`http://127.0.0.1:${fixturePort}/v1/models`, 10_000, "fixture /v1/models");
  const target = await waitForCdpTarget();
  const cdp = new Cdp(target.webSocketDebuggerUrl);
  await cdp.open();
  await cdp.send("Runtime.enable");
  await cdp.send("Page.enable");
  await delay(6_000);

  const setup = await evaluate(cdp, setupSyntheticProfileExpression(fixturePort));
  await sendSyntheticProxyRequest(setup.proxyPort, setup.localKey);
  await delay(1_000);

  const backend = await evaluate(cdp, inspectBackendExpression());
  const routePrep = await evaluate(cdp, openRoutingTraceExpression());
  const windowInfo = await cdp.send("Browser.getWindowForTarget", {});
  const captures = [];
  for (const [width, height] of [
    [1280, 800],
    [1024, 768],
    [980, 640],
  ]) {
    captures.push(await captureRoutingWorkspace(cdp, windowInfo.windowId, width, height));
  }

  const requestLog = await evaluate(cdp, openRequestLogExpression());
  const requestLogScreenshot = join(evidenceDir, "request-log-opened-1024x768.png");
  const requestLogShot = await cdp.send("Page.captureScreenshot", {
    format: "png",
    captureBeyondViewport: false,
  });
  writeFileSync(requestLogScreenshot, Buffer.from(requestLogShot.data, "base64"));

  await evaluate(cdp, stopProxyExpression());
  cdp.close();

  const evidence = {
    profileRoot,
    appIdentifier,
    configPath,
    fixtureBaseUrl: `http://127.0.0.1:${fixturePort}/v1`,
    stationId: setup.stationId,
    stationKeyId: setup.stationKeyId,
    backend,
    routePrep,
    captures,
    requestLog: {
      ...requestLog,
      screenshot: requestLogScreenshot,
    },
  };
  const evidencePath = join(evidenceDir, "routing-workspace-tauri-cdp-evidence.json");
  writeFileSync(evidencePath, JSON.stringify(evidence, null, 2));
  console.log("routing workspace Tauri CDP verification ok");
  console.log(JSON.stringify({ evidencePath, captures, requestLogScreenshot }, null, 2));
} finally {
  await cleanup();
}

function setupSyntheticProfileExpression(port) {
  return `(async () => {
  const coreUrl = performance.getEntriesByType('resource').map((entry) => entry.name).find((name) => name.includes('@tauri-apps_api_core'));
  const { invoke } = await import(coreUrl);
  const station = await invoke('create_station', { input: {
    name: 'Synthetic Routing Fixture Station With Quite Long Local Name For Layout Verification',
    stationType: 'openai-compatible',
    websiteUrl: 'http://127.0.0.1:${port}',
    apiBaseUrl: 'http://127.0.0.1:${port}/v1',
    apiKey: 'fixture-local-key',
    collectorProxyMode: 'direct',
    collectorProxyUrl: null,
    enabled: true,
    creditPerCny: 1,
    lowBalanceThresholdCny: null,
    collectionIntervalMinutes: 60,
    note: 'Task23 synthetic fixture only'
  }});
  const keyResult = await invoke('save_station_key_with_defaults', { input: {
    mode: 'create',
    stationId: station.id,
    name: 'Synthetic Routing Fixture Key With Long Name For Layout',
    apiKey: 'fixture-local-key',
    enabled: true,
    schedulable: true,
    priority: 0,
    tierLabel: null,
    balanceScope: null,
    status: 'healthy',
    note: 'Task23 synthetic fixture only',
    groupSelection: { kind: 'clear' },
    capabilities: null
  }});
  const stationKeyId = keyResult.stationKey.id;
  await invoke('update_station_key_capabilities', { input: {
    stationKeyId,
    supportsChatCompletions: true,
    supportsResponses: true,
    supportsEmbeddings: true,
    supportsStream: true,
    supportsTools: false,
    supportsVision: false,
    supportsReasoning: false,
    modelAllowlist: ['routing-fixture-chat', 'routing-fixture-embedding'],
    modelBlocklist: [],
    preferredModels: ['routing-fixture-chat'],
    onlyUseAsBackup: false,
    routingTags: ['task23-fixture']
  }});
  const localKey = await invoke('get_local_access_key', { input: {} });
  const proxy = await invoke('start_local_proxy', { input: {} });
  return { stationId: station.id, stationKeyId, localKey, proxyPort: proxy.port };
})()`;
}

function inspectBackendExpression() {
  return `(async () => {
  const coreUrl = performance.getEntriesByType('resource').map((entry) => entry.name).find((name) => name.includes('@tauri-apps_api_core'));
  const { invoke } = await import(coreUrl);
  const snapshot = await invoke('load_routing_workspace_snapshot', { input: { limit: 50, cursor: null } });
  const overlay = await invoke('load_routing_runtime_overlay', { input: {} });
  const logs = await invoke('list_request_logs', { input: {} });
  const recent = await invoke('list_recent_route_decisions', { input: { limit: 8, cursor: null } });
  const trace = logs[0]?.id ? await invoke('get_request_decision_trace', { input: { requestLogId: logs[0].id } }) : null;
  const simulation = await invoke('simulate_route', { input: {
    endpoint: 'chat_completions',
    model: 'routing-fixture-chat',
    stream: false,
    usesTools: false,
    usesVision: false,
    usesReasoning: false,
    policy: null,
    maxRateMultiplier: null,
    routingGroupFilter: null,
    sessionHash: null,
    previousResponseId: null
  }});
  return {
    snapshotCandidateCount: snapshot.candidates?.length ?? 0,
    snapshotStatus: snapshot.status,
    firstCandidate: snapshot.candidates?.[0] ?? null,
    overlayVersion: overlay.overlayVersion,
    requestLog: logs[0] ?? null,
    recentDecisionCount: recent.decisions?.length ?? 0,
    traceStatus: trace?.status ?? null,
    timelineKinds: trace?.timeline?.map((entry) => entry.kind) ?? [],
    simulationSelectedStationKeyId: simulation.selectedStationKeyId,
    simulationCapacityMode: simulation.capacityMode
  };
})()`;
}

function openRoutingTraceExpression() {
  return `(async () => {
  const controls = [...document.querySelectorAll('button,a')];
  controls[3]?.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, view: window }));
  await new Promise((resolve) => setTimeout(resolve, 3000));
  const decisionButton = [...document.querySelectorAll('button')].find((element) => (element.textContent || '').includes('routing-fixture-chat'));
  if (decisionButton) {
    decisionButton.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, view: window }));
    await new Promise((resolve) => setTimeout(resolve, 2000));
  }
  return {
    routeClicked: controls[3]?.getAttribute('aria-label'),
    decisionClicked: Boolean(decisionButton),
    body: document.body.innerText.slice(0, 2200)
  };
})()`;
}

function openRequestLogExpression() {
  return `(async () => {
  const button = [...document.querySelectorAll('button')].find((element) => (element.textContent || '').includes('查看使用记录'));
  const buttonText = (button?.textContent || '').trim();
  if (button) {
    button.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, view: window }));
    await new Promise((resolve) => setTimeout(resolve, 2200));
  }
  return {
    buttonText,
    opened: document.body.innerText.startsWith('使用记录'),
    body: document.body.innerText.slice(0, 2200)
  };
})()`;
}

function stopProxyExpression() {
  return `(async () => {
  const coreUrl = performance.getEntriesByType('resource').map((entry) => entry.name).find((name) => name.includes('@tauri-apps_api_core'));
  const { invoke } = await import(coreUrl);
  return await invoke('stop_local_proxy', { input: {} });
})()`;
}

async function sendSyntheticProxyRequest(port, localKey) {
  await json(`http://127.0.0.1:${port}/v1/chat/completions`, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      authorization: `Bearer ${localKey}`,
    },
    body: JSON.stringify({
      model: "routing-fixture-chat",
      messages: [{ role: "user", content: "task23 synthetic ping" }],
      stream: false,
    }),
  });
}

async function captureRoutingWorkspace(cdp, windowId, width, height) {
  await cdp.send("Browser.setWindowBounds", {
    windowId,
    bounds: { width, height, windowState: "normal" },
  });
  await delay(1_400);
  const metrics = await evaluate(
    cdp,
    `(() => {
      const body = document.body.innerText;
      return {
        innerWidth,
        innerHeight,
        scrollWidth: document.documentElement.scrollWidth,
        viewportOverflowX: document.documentElement.scrollWidth > window.innerWidth + 4,
        routingVisible: body.includes('Synthetic Routing Fixture'),
        recentDecisionVisible: body.includes('routing-fixture-chat'),
        timelineVisible: body.includes('Legacy routing summary') && body.includes('Cost aggregate'),
        requestLogButtonVisible: body.includes('查看使用记录'),
        body: body.slice(0, 2200)
      };
    })()`,
  );
  const screenshot = await cdp.send("Page.captureScreenshot", {
    format: "png",
    captureBeyondViewport: false,
  });
  const file = join(evidenceDir, `routing-workspace-${width}x${height}.png`);
  writeFileSync(file, Buffer.from(screenshot.data, "base64"));
  return { width, height, file, metrics };
}

async function evaluate(cdp, expression) {
  const result = await cdp.send("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  if (result.exceptionDetails) {
    throw new Error(JSON.stringify(result.exceptionDetails));
  }
  return result.result.value;
}

async function waitForCdpTarget() {
  const deadline = Date.now() + 120_000;
  while (Date.now() < deadline) {
    if (tauri.exitCode !== null) {
      throw new Error(`tauri dev exited early with ${tauri.exitCode}\n${tauriLog}`);
    }
    try {
      const targets = await json(`http://127.0.0.1:${debugPort}/json/list`);
      const target = targets?.find((candidate) => candidate.webSocketDebuggerUrl);
      if (target) return target;
    } catch {
      // Keep waiting; the WebView is not ready yet.
    }
    await delay(500);
  }
  throw new Error(`CDP target not available\n${tauriLog}`);
}

async function waitJson(url, timeoutMs, label) {
  const deadline = Date.now() + timeoutMs;
  let lastError = null;
  while (Date.now() < deadline) {
    try {
      return await json(url);
    } catch (error) {
      lastError = error;
      await delay(250);
    }
  }
  throw new Error(`timeout waiting for ${label}: ${lastError}\nfixture log:\n${fixtureLog}`);
}

async function json(url, options) {
  const response = await fetch(url, options);
  const text = await response.text();
  let body = null;
  try {
    body = text ? JSON.parse(text) : null;
  } catch {
    body = text;
  }
  if (!response.ok) {
    throw new Error(`${response.status} ${JSON.stringify(body)}`);
  }
  return body;
}

async function cleanup() {
  for (const child of [tauri, fixture]) {
    if (child.exitCode === null) {
      await new Promise((resolve) => {
        const killer = spawn("taskkill.exe", ["/pid", String(child.pid), "/t", "/f"], {
          stdio: "ignore",
        });
        killer.on("exit", resolve);
        killer.on("error", resolve);
        setTimeout(resolve, 3_000);
      });
    }
  }
}

function parsePort(value, label) {
  const parsed = Number.parseInt(value, 10);
  if (!Number.isInteger(parsed) || parsed < 1024 || parsed > 65535) {
    throw new Error(`${label} port must be an integer in 1024..65535`);
  }
  return parsed;
}

function tail(value, maxLength) {
  return value.length > maxLength ? value.slice(-maxLength) : value;
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
