# NewAPI 严格采集可靠性设计

## 目标

Relay Pool 的 NewAPI 采集只保存能够由 QuantumNous/new-api 上游 controller、model 和写入链证明语义的数据。字段缺失、接口截断或历史覆盖不完整时保存 `None`，不得用零、默认倍率或计费额度伪装为真实值。

## 权威来源

- `/api/user/self`：`quota` 是账号剩余额度，`used_quota` 是累计计费额度，`request_count` 是累计账号请求数。
- `/api/data/self`：`count`、`quota`、`token_used` 分别是请求数、计费额度和真实 token 的所选窗口聚合。
- `/api/log/self/stat`：`quota` 是所选窗口计费额度；`rpm` 和 `tpm` 是最近 60 秒速率。
- `/api/log/self`：日志中的 `prompt_tokens` 和 `completion_tokens` 是逐条真实 token，但用户日志计数最多为 10000，只有完整分页后才能作为精确窗口事实。
- `/api/user/self/groups`：`ratio` 是当前用户使用目标分组的有效倍率。
- `/api/user/models`：返回当前用户可用模型并集，不代表具体分组能力或上游实时健康。

## 采集规则

1. `used_quota` 永远不得写入 token 字段；`quota_display_type=TOKENS` 仅影响展示。
2. 今日 token 取 `/api/data/self.token_used`；dashboard 不可用时，仅在日志窗口完整且日志确实提供 token 字段时使用日志合计。
3. 总 token 只有在 dashboard 累计原始 `quota` 与 `/api/user/self.used_quota` 精确相等、累计 `count` 同时与 `/api/user/self.request_count` 精确相等时写入；日志回退也必须完整分页且请求数与账号累计请求数一致。否则为未知。
4. 累计请求数优先且实际由 `/api/user/self.request_count` 提供。日志窗口截断时不得写入日志 `total`。
5. `/api/log/self/stat` 缺少或非法 `quota` 时消费为未知，不得生成 `0`。
6. `quota_per_unit` 缺失或无效时，不进行余额或消费金额换算。原始额度不映射到现有货币字段；不依赖换算的 dashboard `token_used/count/quota` 仍可用于直接事实和历史覆盖验证。
7. 标准 NewAPI 不提供基础消费字段；仅接受响应中明确存在的兼容扩展字段，不从其他字段推导。
8. 分组 `ratio` 只写 `effective_rate_multiplier`；默认倍率和用户覆盖倍率保持未知。
9. SQLite 和前端继续保留空值；未知 token 显示 `-`，未知请求数显示“未采集”。

## 修改边界

- `src-tauri/src/services/collectors/adapters/newapi/mod.rs`：严格化来源选择、截断保护和统计合并。
- `src-tauri/src/services/collectors/adapters/newapi/parsers.rs`：可选额度换算、严格字段映射和分组倍率语义。
- 如现有前端空值回归测试不足，仅补充相应脚本测试；不改页面布局。

## 验证

- 每项行为先添加失败回归，再做最小实现。
- 运行 NewAPI parser 和 balance 定向测试。
- 运行 NewAPI adapter 全部测试、`cargo fmt --check`、`cargo check -p relay-pool-desktop`。
- 若改动前端，额外运行相应 Node 脚本及 `pnpm build`。

## 最终源码复核补充

- `/api/user/self` 当前标准响应不包含注册时间；累计 dashboard 必须按月向前搜索。最近窗口为空只能表示该窗口无记录，不能提前终止历史搜索，也不能据此生成全历史零值。
- 跨 dashboard 窗口累计时，任一窗口缺少某项标准指标，该累计指标整体保持未知；不得只合计其他窗口后冒充全量。
- `/api/user/models` 的标准 `data` 是字符串数组；对象形式的 `id/name/model` 不作为兼容别名。
- `/api/token/` 的标准 `data` 是 `PageInfo`，必须包含一致的 `page/page_size/total/items`；Token 只读取上游结构定义的 `id/name/key/group/created_time/accessed_time`。
- `/api/status` 必须通过标准 `success/data` envelope；失败或缺失 envelope 不回退解析原始对象。所有用于换算的浮点值必须为有限数。
