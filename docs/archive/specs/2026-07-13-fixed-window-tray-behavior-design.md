# Fixed Window and Tray Behavior Design

## Goal

Make the main window lifecycle predictable and non-configurable:

- Minimizing keeps the application on the Windows taskbar.
- Closing the main window hides it to the system tray without stopping the process.
- The tray Show action restores and focuses the main window.
- The tray Quit action exits the process.

## Root Cause

The current Tauri event handler already separates close-to-tray and minimize-to-tray behavior, but the persisted and fallback setting defaults to `minimize-to-tray`. Existing installations can therefore continue hiding the window after a minimize event.

The setting is broader than the desired product behavior. Changing only its default would not fix existing installations because their persisted value would remain unchanged.

## Design

The main window event handler will use a fixed policy:

- `CloseRequested` prevents the native close and hides the main window.
- Minimize and resize events receive no custom handling, so Windows retains the minimized window on the taskbar.
- Events from non-main windows remain unaffected.

The settings UI and the TypeScript/Rust settings contracts will no longer expose tray behavior as a user preference. Existing `tray_behavior` rows may remain in local databases as ignored compatibility data. No destructive database migration is required.

The existing tray menu and tray icon restoration behavior remain unchanged.

## Compatibility

Removing the active setting ensures existing installations adopt the fixed policy regardless of their persisted `tray_behavior` value. Leaving the old database row in place avoids an unnecessary schema or data migration and is harmless because runtime code no longer reads it.

Frontend mock and fallback settings will remove the obsolete field so browser-mode development matches the desktop contract.

## Testing

Rust unit tests will define the lifecycle policy before implementation and cover both required outcomes:

- A close request maps to hiding the main window.
- A minimize or resize event does not map to hiding the main window.

Contract and fixture updates will remove `trayBehavior` from frontend and backend settings data. Verification will include the focused Rust tests, the available Cargo check, and the TypeScript/Vite build.

## Out of Scope

- Changing tray icon appearance or tray menu labels.
- Adding a dark theme or other settings UI work.
- Deleting historical `tray_behavior` rows from user databases.
- Changing the lifecycle of auxiliary windows.
