# 变更中心告警升级基线

日期：2026-08-08

## 当前事实

- `change_events.status` 同时承载 unread/read/dismissed/resolved，无法区分问题生命周期和用户关注状态。
- 生产路径仍由 `ChangeService`、`ChangeStore` 和旧 IPC 命令写入/读取 `change_events`。
- Change Center 以固定窗口查询后在前端筛选和聚合，无法对全量当前问题提供可靠计数。
- 现有恢复逻辑只覆盖少量采集失败路径；group、balance、pricing、key health、station health 和 routing 没有统一恢复 owner。
- 仓库已有用户改动位于 `src/components/ui/SelectControl.tsx`、`src/features/changes/ChangeCenterPage.tsx` 和 `src/features/key-pool/KeyPoolPage.tsx`，升级不得覆盖。

## 升级基线约束

- 新 occurrence 只追加，incident 由当前权威事实投影，attention 不得改变 incident 生命周期。
- 旧表只允许由 durable backfill 读取，禁止新 producer 双写或运行时 fallback。
- 0029 migration 只建立结构和 postcondition，不在 migration 内复制历史。
- 未完成 history backfill、current-fact rebuild、schema15 recovery 和备份验证前，不得切换生产 writer，也不得删除 `change_events`。

## 已完成证据

- 领域模型覆盖 occurrence、incident、attention、policy 和 delivery。
- 0029 schema migration 与持久化结构测试通过。
- 历史复制器具备 cursor、幂等 key 和中断重跑测试。
- ingress、condition key、projector 和 delivery deadline 已有单元测试。

## 尚未完成

- producer cutover、incident SQL projector、policy/settings IPC、delivery worker、Change Center UI、桌面通知和 legacy table removal。
