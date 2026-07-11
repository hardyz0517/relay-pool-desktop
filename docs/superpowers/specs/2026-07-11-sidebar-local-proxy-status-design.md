# Sidebar Local Proxy Status Design

## Goal

Make the bottom-left sidebar indicator reflect whether the local proxy is running.

## Root Cause

`DashboardPage` updates its local `ProxyStatus` after start and stop actions, but
`AppShell` renders a fixed orange dot and fixed stopped label. The two components
do not share a proxy-status source.

## Approach

`AppShell` will call the existing `getProxyStatus()` API when mounted and every
two seconds. The shell owns only the display state and does not start,
stop, or otherwise control the local proxy.

The indicator maps `ProxyStatus.running` as follows:

| running | Dot color | Accessible label |
| --- | --- | --- |
| `true` | green | `本地代理运行中` |
| `false` | amber | `本地代理未启动` |

Failures to refresh retain the most recent displayed state so transient reads do
not make the sidebar misleadingly flip to stopped.

## Scope

- Update `src/components/shell/AppShell.tsx`.
- Add one focused source-level regression script for the status fetch, refresh,
  color, and accessible labels.
- Run that script and the TypeScript/Vite build.

## Non-goals

- No change to the proxy runtime, Tauri commands, or start/stop behavior.
- No change to dashboard or settings proxy controls.
- No new global store or event protocol.
