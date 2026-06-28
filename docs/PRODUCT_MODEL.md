# Product Model

Relay Pool Desktop uses a deliberately split product model so collection, routing, and health surfaces do not blur into one another.

## Core Concepts

- `Station / 中转站` = station account asset.
- `Station Key / 中转站 Key` = the routeable API key.
- `Key Pool / Key 池` = the global routing pool view.
- `Collector / 信息采集` = works around `Station`.
- `Router / 路由` = works around `Station Key`.
- `Channel Status / 渠道状态` = works around `Key / Channel`.

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
- P7 / P8 can connect price normalization, balance avoidance, cost calculation, and richer routing policies.
