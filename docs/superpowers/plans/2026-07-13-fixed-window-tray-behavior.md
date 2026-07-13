# Fixed Window and Tray Behavior Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make minimizing stay on the Windows taskbar and make closing the main window always hide to the system tray.

**Architecture:** Replace the persisted tray preference with a fixed Rust lifecycle policy used by the Tauri window event handler. Remove the obsolete setting from Rust and TypeScript contracts while leaving historical database rows untouched and unread.

**Tech Stack:** Tauri 2, Rust, React, TypeScript, Vite

---

## File Structure

- `src-tauri/src/lib.rs`: Own the fixed main-window lifecycle policy and Tauri event handling.
- `src-tauri/src/models/settings.rs`: Remove tray behavior from serialized settings contracts and test old-client-compatible deserialization.
- `src-tauri/src/services/database.rs`: Stop seeding, updating, and reading the obsolete setting.
- `src-tauri/src/services/proxy/runtime.rs`: Remove the obsolete field from its settings test fixture.
- `src/lib/types/settings.ts`: Remove the frontend tray behavior type, fields, mapper entry, and labels.
- `src/lib/api/settings.ts`: Remove the browser fallback value.
- `src/features/settings/SettingsPage.tsx`: Remove tray behavior from local fallback and form state mapping.
- `src/lib/mock/settings.ts`: Remove the obsolete mock setting.

### Task 1: Fix the main-window lifecycle policy

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write the failing regression test**

Replace the current mapping tests with a test showing that persisted legacy values must no longer control window behavior:

```rust
#[test]
fn persisted_tray_settings_do_not_override_fixed_window_policy() {
    for value in ["minimize-to-tray", "close-to-tray", "disabled", "unexpected"] {
        let behavior = TrayBehavior::from_setting(value);
        assert!(behavior.hides_on_close());
        assert!(!behavior.hides_on_minimize());
    }
}
```

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml persisted_tray_settings_do_not_override_fixed_window_policy
```

Expected: FAIL because `minimize-to-tray` currently returns `MinimizeToTray`, so `hides_on_close()` is false.

- [ ] **Step 3: Implement the minimal behavior change**

Make every legacy setting map to close-to-tray behavior:

```rust
fn from_setting(_value: &str) -> Self {
    Self::CloseToTray
}
```

- [ ] **Step 4: Run the focused test and verify GREEN**

Run the same `cargo test` command. Expected: PASS.

- [ ] **Step 5: Refactor to a fixed policy**

Replace the configurable enum and database lookup with a fixed policy:

```rust
#[derive(Debug, Clone, Copy)]
struct WindowLifecyclePolicy;

impl WindowLifecyclePolicy {
    fn hides_on_close(self) -> bool {
        true
    }

    fn hides_on_minimize(self) -> bool {
        false
    }
}
```

In `on_window_event`, use `let behavior = WindowLifecyclePolicy;`. Keep the close branch that calls `api.prevent_close()` and `window.hide()`. The resize branch remains guarded by `hides_on_minimize()`, which is fixed to false, so Windows handles minimize normally. Replace the regression test with:

```rust
#[test]
fn fixed_window_policy_hides_only_on_close() {
    let behavior = WindowLifecyclePolicy;
    assert!(behavior.hides_on_close());
    assert!(!behavior.hides_on_minimize());
}
```

- [ ] **Step 6: Re-run the Rust unit tests**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml fixed_window_policy_hides_only_on_close
```

Expected: PASS.

- [ ] **Step 7: Commit the lifecycle fix**

```powershell
git add -- src-tauri/src/lib.rs
git commit -m "fix: keep minimized window on taskbar"
```

### Task 2: Remove the obsolete Rust settings contract

**Files:**
- Modify: `src-tauri/src/models/settings.rs`
- Modify: `src-tauri/src/services/database.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Test: `src-tauri/src/models/settings.rs`

- [ ] **Step 1: Write the failing contract test**

Delete `"trayBehavior": "minimize-to-tray"` from the JSON fixture in `update_settings_input_allows_missing_scheduler_fields`. Keep the existing `.expect(...)` and assertions.

- [ ] **Step 2: Run the focused test and verify RED**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml update_settings_input_allows_missing_scheduler_fields
```

Expected: FAIL because `UpdateSettingsInput.tray_behavior` is still required and deserialization reports a missing field.

- [ ] **Step 3: Remove tray behavior from the Rust contract**

Delete these fields:

```rust
pub tray_behavior: String,
```

Remove the field from both `AppSettings` and `UpdateSettingsInput`. Then remove:

- `("tray_behavior", input.tray_behavior)` from `update_settings`.
- `("tray_behavior", "minimize-to-tray")` from `seed_default_settings`.
- `tray_behavior: read_setting(connection, "tray_behavior")?` from `load_settings`.
- `tray_behavior` entries from Rust `UpdateSettingsInput` fixtures in `database.rs` and `proxy/runtime.rs`.

Do not add a deletion migration. Existing rows remain ignored.

- [ ] **Step 4: Run Rust settings and library tests**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml update_settings_input_allows_missing_scheduler_fields
cargo test --manifest-path src-tauri/Cargo.toml fixed_window_policy_hides_only_on_close
```

Expected: both commands PASS.

- [ ] **Step 5: Commit the Rust contract cleanup**

```powershell
git add -- src-tauri/src/models/settings.rs src-tauri/src/services/database.rs src-tauri/src/services/proxy/runtime.rs
git commit -m "refactor: remove tray behavior setting"
```

### Task 3: Remove the obsolete frontend settings contract

**Files:**
- Modify: `src/lib/types/settings.ts`
- Modify: `src/lib/api/settings.ts`
- Modify: `src/features/settings/SettingsPage.tsx`
- Modify: `src/lib/mock/settings.ts`

- [ ] **Step 1: Remove tray behavior types and mappings**

From `src/lib/types/settings.ts`, remove:

```typescript
export type TrayBehavior = "minimize-to-tray" | "close-to-tray" | "disabled";
```

Also remove `trayBehavior` from `AppSettings`, `UpdateSettingsInput`, and `appSettingsToUpdateInput`, and delete `trayBehaviorLabels`.

- [ ] **Step 2: Remove frontend fallback and form fields**

Remove `trayBehavior` from:

- `memorySettings` in `src/lib/api/settings.ts`.
- `SettingsFormState`, `fallbackSettings`, `settingsToForm`, and `formToInput` in `src/features/settings/SettingsPage.tsx`.
- `MockSettings` and `mockSettings` in `src/lib/mock/settings.ts`.

Remove the now-unused `TrayBehavior` import from `SettingsPage.tsx`.

- [ ] **Step 3: Confirm no active tray setting references remain**

Run:

```powershell
rg -n "minimize-to-tray|close-to-tray|tray_behavior|trayBehavior|TrayBehavior" src src-tauri/src
```

Expected: no matches. Historical documentation and local database rows are intentionally outside this check.

- [ ] **Step 4: Run full verification**

Run:

```powershell
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: TypeScript/Vite build succeeds, all Rust tests pass, and Cargo check succeeds without new warnings caused by this change.

- [ ] **Step 5: Commit the frontend cleanup**

```powershell
git add -- src/lib/types/settings.ts src/lib/api/settings.ts src/features/settings/SettingsPage.tsx src/lib/mock/settings.ts
git commit -m "refactor: remove tray behavior frontend contract"
```

### Task 4: Review final scope

**Files:**
- Review: all files committed in Tasks 1-3

- [ ] **Step 1: Confirm only task files changed**

Run:

```powershell
git status --short
git log -4 --oneline
```

Expected: pre-existing user changes may remain unstaged, while the tray behavior implementation files are committed. No unrelated files are included in the new commits.

- [ ] **Step 2: Manually verify the desktop behavior when an interactive desktop session is available**

Run:

```powershell
pnpm tauri:dev
```

Verify that minimize leaves the window on the taskbar, close hides it, tray Show restores it, and tray Quit exits. If interactive verification cannot be completed in the current environment, report that limitation explicitly.
