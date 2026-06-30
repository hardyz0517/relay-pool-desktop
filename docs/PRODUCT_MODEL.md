# Product Model

Relay Pool Desktop uses a deliberately split product model so collection, routing, pricing, and health surfaces do not blur into one another.

## Core Concepts

- `Station / 中转站` = station account asset.
- `Station Key / 中转站 Key` = the routeable API key.
- `Key Pool / Key 池` = the global routing pool view.
- `Collector / 信息采集` = works around `Station`.
- `Router / 路由` = works around `Station Key`.
- `Channel Status / 渠道状态` = works around `Key / Channel`.
- `Pricing Rule / 价格规则` = normalized pricing data for station / group / model.
- `Balance Snapshot / 余额快照` = normalized balance or quota state with explicit units.
- `Request Cost / 请求成本` = per-request usage and estimated cost metadata.
- `Secret / 凭据密文` = encrypted sensitive data referenced by business objects.

## Station

`Station` means one relay website plus one user login account.

It owns:

- name
- base_url
- station_type
- login account
- login password state
- login status
- balance source
- group source
- multiplier source
- model pricing source
- collector snapshots
- the station keys under it

`Station` is an account asset, not the final request-routing object.

## Station Key

`Station Key` means one usable API key under a station account.

It owns:

- key name
- masked key
- parent station
- enabled state
- priority
- group / tier
- status
- latency
- success rate
- recent error
- model scope
- future routing policy input

`Station Key` is the object that participates in forwarding, routing, fallback, and health checks.

## Key Pool

`Key Pool` is the unified management view for all `station_keys`.

It owns:

- flat display of all keys
- filter by station
- search
- drag sorting
- enable / disable
- priority
- fallback order
- health status
- future router integration

## Pricing Rule

`Pricing Rule` is normalized price data derived from a station snapshot, manual entry, or collector source.

It owns:

- station
- group / tier
- model
- input price
- output price
- currency
- unit
- source
- confidence
- enabled state
- collected time

## Balance Snapshot

`Balance Snapshot` is normalized balance or quota state with explicit units.

It owns:

- station
- optional station key
- scope
- value
- currency or credit unit
- low balance threshold
- state
- source
- confidence
- collected time

## Request Cost

`Request Cost` is per-request usage and estimated cost metadata.

It owns:

- token counts
- estimated input cost
- estimated output cost
- estimated total cost
- cost currency
- pricing rule source
- cost status

## Secret

`Secret` is encrypted sensitive data owned by a Station, Station Key, collector, proxy runtime, or settings surface.

It owns:

- encrypted value
- masked value
- owner id
- kind
- encryption version
- migration status

Business objects reference secrets through `SecretRef` and never expose full values in list APIs.

## Collector

`Collector` works around `Station`.

It owns:

- login test
- balance collection
- group collection
- multiplier collection
- model collection
- key metadata collection
- collector snapshot persistence

## Router

`Router` works around `Station Key`.

It owns:

- model-aware key selection
- protocol-aware key selection
- key model allowlist / blocklist matching
- model alias mapping
- cooldown and health-aware filtering
- route explanation
- fallback
- failure switching
- usage stats
- request log writing
- cheap-first cost-aware sorting

## Channel Status

`Channel Status` will gradually move toward `Key / Channel` level instead of only station level.

It owns:

- latency
- success rate
- error rate
- consecutive failures
- cooldown state
- recent status bars
- recent error
- parent station
- parent key

## Phase Boundaries

- P4.1 closes the product model and Key Pool MVP.
- P4 continues improving login-state information collection.
- P5 builds the local OpenAI-compatible proxy and basic priority fallback.
- P6 adds model-aware, protocol-aware, health-aware Station Key routing with aliases, key capability scope, cooldown, route simulation, and route explanations.
- P7 adds price normalization, balance avoidance, request cost tracking, and cheap-first routing.
- P8 adds security and credential governance: encrypted local secrets, plaintext migration, UI masking, log/snapshot redaction, import/export boundaries, and local proxy exposure review.
