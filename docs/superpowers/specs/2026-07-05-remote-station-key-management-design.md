# Remote Station Key Management Design

## 1. Background

Relay Pool Desktop already separates `Station` from `Station Key`: a Station is the relay-site account, while Station Key is the object used by local routing, fallback, health checks, and cost decisions. The existing data model already has key-level group and rate fields, including `groupName`, `groupIdHash`, `groupBindingId`, `rateMultiplier`, `rateSource`, and `rateCollectedAt`.

This design adds remote key management for Sub2API and NewAPI stations without changing that product model. The feature lets Relay Pool Desktop discover which keys exist on the relay site, identify which remote key matches a locally saved Station Key, sync the key-level group/rate facts, and create a new remote key from a selected group without making the user open the relay website.

## 2. Approved Product Decisions

- First station support: design for both Sub2API and NewAPI.
- Adapter behavior: each station type reports its supported remote-key capabilities; unsupported operations degrade cleanly instead of making the whole station unusable.
- Scan behavior: scanning remote keys only displays and identifies remote keys by default. It does not automatically create local Station Keys.
- Create behavior: when the user explicitly creates a remote key inside Relay Pool Desktop, the created key is saved locally as an enabled Station Key.
- UI direction: keep the local desktop-tool style. Do not make a separate SaaS-style admin center. Put compact key rows directly under the supplier add/edit form, similar to the existing model-mapping row editor pattern.

## 3. Goals

- Show the keys the current user has created on a relay site, including group, multiplier, masked key, display name, and remote metadata when available.
- Identify which remote key corresponds to a locally saved Station Key.
- Sync group and multiplier facts from remote key discovery to the matched local Station Key.
- Let the user choose a group and create a remote key from the supplier row or supplier edit UI.
- Let the supplier add/edit UI display and edit the Station Keys under that supplier directly.
- Preserve manual local-key management for stations that cannot expose remote key APIs.

## 4. Non-Goals

- Do not auto-enable scanned remote keys that the user did not explicitly create in the app.
- Do not delete remote keys in this phase.
- Do not implement full remote/local reconciliation as a new top-level synchronization center.
- Do not bypass CAPTCHA, Turnstile, 2FA, or site-specific anti-abuse flows.
- Do not expose full API keys in logs, screenshots, collector snapshots, or routine UI after the creation moment.

## 5. Architecture

### 5.1 Adapter Capability Contract

Add a remote-key capability layer beside the existing collector adapter flow. Each station adapter exposes a small capability summary:

- `canListRemoteKeys`
- `canCreateRemoteKey`
- `canReadGroups`
- `requiresManualSession`
- `unsupportedReason`

The frontend uses this summary to enable or disable `Get all keys` and `Create remote key` actions.

### 5.2 Remote Key Types

Use a normalized remote-key structure before touching local Station Keys:

- `stationId`
- `remoteKeyIdHash`
- `remoteKeyName`
- `apiKeyMasked`
- `apiKeyFingerprint`
- `groupIdHash`
- `groupName`
- `tierLabel`
- `rateMultiplier`
- `rateSource`
- `createdAt`
- `lastUsedAt`
- `rawSource`
- `matchStatus`
- `matchedStationKeyId`
- `matchConfidence`

`apiKeyFingerprint` is derived from a full key only when the remote API returns it. If the remote API only returns a masked key, matching falls back to visible prefix/suffix, group, name, and known local key metadata.

### 5.3 Matching Rules

Remote-to-local matching runs in this order:

1. Full key fingerprint match when both sides can compute one.
2. Stable remote key id hash match when the station exposes a stable key id and the local key was previously bound to it.
3. Masked prefix/suffix match plus station id.
4. Masked match plus group/name hints.

Only high-confidence matches update local Station Key facts automatically. Low-confidence matches are shown as possible matches and require user confirmation before binding.

### 5.4 Fact Synchronization

When a remote key confidently matches a local Station Key, update:

- `groupBindingId` when a matching Station Group Binding exists.
- `groupIdHash`
- `groupName`
- `tierLabel`
- `rateMultiplier`
- `rateSource`
- `rateCollectedAt`

The sync must not replace the stored API key unless the user explicitly edits the key field or creates a new remote key through the app.

## 6. Station Type Behavior

### 6.1 Sub2API

Sub2API should support the full first version when the station session has enough permission:

- Read groups from `GET /api/v1/groups/available`.
- Read rates from `GET /api/v1/groups/rates`.
- Scan keys from the site key-list endpoint when available, starting with the documented `GET /api/v1/keys?page=1&page_size=100` style path.
- Create remote keys through the site key-create endpoint when supported by the station.

If the key-list or key-create endpoint differs by deployment, the adapter reports partial support and preserves group-rate collection.

### 6.2 NewAPI

NewAPI support is capability-based:

- Use existing user/group collection when available.
- Enable remote key scanning or creation only when the concrete NewAPI deployment exposes a compatible token/key endpoint.
- If a deployment lacks a compatible key endpoint, show that remote key management is unsupported while keeping manual local Station Key editing available.

## 7. UI Design

### 7.1 Supplier Add/Edit Page

Add a `密钥` section below the existing connection and optional fields.

Header actions:

- `获取所有 Key`
- `新建远端 Key`
- `添加密钥`

Local key rows:

- Name
- API key input
- Group
- Rate multiplier
- Enabled switch
- Delete icon

For existing keys, the API key input placeholder says the old key is retained when the field is left empty. Adding a key appends a new editable row immediately, matching the compact row-editor feel of the model-mapping screenshot.

Remote discovery rows are displayed separately from local editable rows. A discovered remote key can show:

- `已匹配` with the local key name.
- `可能匹配` with a confirmation action.
- `未绑定` with actions to bind or save as a local key.

Scanning does not silently add these rows to the local editable key list.

### 7.2 Supplier List Row Action

Add a compact key action on the supplier list row, using an icon button or short `Key` action in the existing row controls. When clicked:

1. Load available groups for that station.
2. Open a small create dialog.
3. Let the user choose group and enter key name.
4. Create the key remotely.
5. Save the returned full key locally as an enabled Station Key.
6. Refresh the station key list and remote discovery state.

The action is disabled with a clear reason when the adapter reports `canCreateRemoteKey = false`.

## 8. Error Handling

- Missing login/session: prompt the user to test login or provide tokens. Do not clear prior successful scan data.
- Manual-session requirement: mark the operation as requiring manual login state and keep the station otherwise usable.
- Unsupported remote key scan/create: disable the action and keep manual local key editing available.
- Remote scan failure: keep the previous scan result visible with the last successful timestamp and current error.
- Remote creation succeeds but local save fails: show the newly created full key once in a protected result panel and report the local save failure.
- Low-confidence match: do not sync group/rate facts until the user confirms the match.
- Secret leakage: redact `authorization`, `cookie`, `set-cookie`, full API keys, access tokens, refresh tokens, and raw create responses in logs and snapshots.

## 9. Persistence

The local database should persist discovered remote-key facts separately from local Station Keys so scans can be displayed without importing keys. A minimal table can store:

- station id
- remote key id hash
- masked key
- key fingerprint when available
- group id hash
- group name
- rate multiplier
- match status
- matched station key id
- confidence
- source
- collected timestamp

Local Station Keys remain the routing source of truth. Remote discovery rows are facts and suggestions until the user binds, imports, or creates through the app.

## 10. Verification Plan

Automated checks:

- TypeScript check for new UI types, API wrappers, and supplier form state.
- Cargo check for Tauri commands, adapter trait changes, database migrations, and service wiring.
- Unit tests for remote/local key matching, masked-key fallback, low-confidence non-sync behavior, and create-then-save enabled behavior.

Manual smoke:

- Add supplier with multiple local key rows.
- Edit supplier and leave an existing key field empty to retain the old secret.
- Scan remote keys and confirm they display without auto-import.
- Confirm a high-confidence remote/local match syncs group and multiplier.
- Confirm low-confidence matches require manual binding.
- Create a remote key from a selected group and verify it appears locally as enabled.
- Verify unsupported NewAPI key-management deployments still allow manual key editing.

## 11. Implementation Order

1. Add remote-key normalized types and capability types.
2. Add Tauri commands for capability query, remote key scan, remote key creation, and optional match confirmation.
3. Add database persistence for remote discovery facts and match state.
4. Implement Sub2API adapter support first, with capability fallback for endpoint differences.
5. Implement NewAPI capability detection and any compatible endpoint support without assuming all NewAPI deployments expose key management.
6. Add frontend API wrappers and memory fallbacks for browser-only smoke.
7. Add supplier add/edit key row editor.
8. Add remote discovery list and supplier-row create-key action.
9. Add tests and smoke verification.

## 12. Risks

- NewAPI deployments may not share a stable key-management API. The capability contract prevents this from blocking the feature.
- Some sites may return only masked keys. The matching flow must distinguish high-confidence from possible matches.
- Remote key creation returns the full key only once. The UI must handle local-save failure without losing that value.
- Existing workspace changes around station detail and key pool may overlap with this work. Implementation should stage exact paths and avoid broad refactors.
