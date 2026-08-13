# Debug-Only Local Proxy Auto-Start Design

## Goal

Allow development sessions to start the existing local proxy without relying on the desktop UI. This is intended for automated diagnosis when the WebView is unavailable or blank.

## Scope

- Add the environment variable `RELAY_POOL_DEV_AUTO_START_PROXY`.
- Treat the normalized value `1` as enabled.
- Compile and evaluate the hook only in debug builds.
- Reuse the persisted local proxy port, the real application database, the initialized data key, and `ProxyRuntimeState::start`.
- Keep current startup behavior unchanged when the variable is absent or has another value.

The design does not add a network management endpoint, bypass local bearer authentication, expose secrets, or change release behavior.

## Architecture

The Tauri setup path already creates and manages `AppDatabase`, `SecretManager`, and `ProxyRuntimeState`. After those states are initialized, a small debug-only startup helper receives clones of the same state values used by the existing `start_local_proxy` command.

The helper performs the same preparation as the command:

1. Read `local_proxy_port` from persisted settings.
2. Run the existing plaintext-secret migration with the initialized data key.
3. Build `ProxyStartConfig` from the real database, data key, and port.
4. Call `ProxyRuntimeState::start` on the Tauri async runtime.

There is one proxy runtime instance. The development hook and UI command both use it, so existing idempotency and lifecycle locking remain authoritative.

## Activation And Safety

- The hook is guarded with `#[cfg(debug_assertions)]`.
- Only the exact normalized value `1` enables it.
- Release builds contain no environment-variable read or automatic start call.
- The proxy continues to bind through the existing v2 server implementation, which is loopback-only.
- Local bearer authentication remains mandatory.
- Errors must not include API keys, cookies, bearer tokens, or decrypted credential values.

## Failure Behavior

Automatic proxy startup is auxiliary to desktop startup. If settings, secret migration, binding, or proxy initialization fails, the application window must still start. The failure is logged as a concise, redacted development diagnostic. The existing proxy status snapshot remains the source of truth for the UI.

## Verification

Focused tests will prove:

- absent, empty, and non-`1` values do not enable auto-start;
- `1` enables auto-start in debug/test code;
- the startup helper uses the persisted port and existing runtime path;
- startup failure is contained and does not fail application setup;
- existing manual start and proxy runtime tests remain green.

Live verification will launch:

```powershell
$env:RELAY_POOL_DEV_AUTO_START_PROXY='1'
$env:CARGO_TARGET_DIR='D:\Dev\Projects\relay-pool-desktop\output\local-routing-v2-target'
pnpm tauri:dev
```

Success requires the desktop process, `127.0.0.1:1430`, and `127.0.0.1:8787` to appear without UI interaction. A real authenticated streaming `/v1/responses` request must then complete or produce an accurately recorded upstream failure.
