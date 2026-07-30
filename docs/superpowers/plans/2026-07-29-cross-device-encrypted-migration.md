# 跨设备加密迁移完整实施计划

状态：Reviewed Implementation Plan

日期：2026-07-29

规范来源：[`../../proposals/CROSS_DEVICE_ENCRYPTED_MIGRATION_SPEC.md`](../../proposals/CROSS_DEVICE_ENCRYPTED_MIGRATION_SPEC.md)

## 目标

为 Relay Pool Desktop 实现 Windows 首版 `.rpd-move` 跨设备加密搬家：源设备密钥不离机，迁移包使用 age passphrase 容器，便携 secret 使用一次性 transport key，目标库全部 secret 使用目标设备密钥重新加密；导出完整回读后原子发布，导入只构建新库并通过重启 journal 激活，任意失败或崩溃不破坏当前数据库。

## 架构结论

- `DeviceKeyStore` 只负责版本化设备密钥存取；`SecretService` 负责加解密；业务模块不再长期持有裸 `[u8; 32]`。
- `services/portable_migration` 负责格式、catalog、reader/writer、staging 和验证；不依赖 Tauri command 或 React。
- `application/data_migration` 负责 use case、幂等、operation/result registry 和维护状态编排。
- `persistence` 只提供一致性 snapshot、受信任新库写入、WAL freeze 和关闭原语。
- `services/data_store` 提供 Windows 文件身份与原子发布/替换 adapter，复用现有 `ReplaceFileW` 经验，不复制三套实现。
- 包内 SQLite 永远是不可信只读输入；不在其上运行 migration、DDL、trigger、view、extension 或任意动态 SQL。
- 正常 UI 只在 `normal/exporting/inspecting_import/preparing_import` 可用；`activation_pending/recovering` 在应用最外层进入维护或恢复界面。

## 非目标

- 不实现实时同步、云备份、行级 merge、选择性站点导入、无密码迁移或非 Windows key store。
- 不改变现有本机备份和同机 data-dir relocation 的产品语义。
- 不在本计划中删除旧格式 verified backup 或源 `.rpd-move`。
- 不提交真实 key、cookie、token、本地数据库、迁移包、日志、诊断或手工资格测试中的敏感记录。

## 实施前锁定

当前基线必须在 Task 0 重新确认：database generation `2`，writable schema、应用表数量和最新 migration 均以当时 registry 为准。状态监控与后续主干迁移合并后 `0009_provider_drafts.sql` 到 `0016_monitor_sub2api_latency_defaults.sql` 已被占用；安全基线 migration 应选下一个空闲编号（当前为 `0017_encrypted_secret_baseline.sql`），并同步更新本计划执行记录、fixture 和 compatibility 常量，禁止覆盖已有 migration。

当前 `src-tauri/src/lib.rs` 的启动顺序是 SecretManager 先于 installation lease；Task 2 必须先修。当前 composition 仍在多处复制 `[u8; 32]`；Task 3 必须逐步收口，不能在迁移功能旁继续保留第二条裸 key 路径。

## 锁定模块图

新增后端模块：

```text
src-tauri/src/application/data_migration/
  mod.rs                 # facade/use-case composition only
  export_service.rs
  import_service.rs
  registry.rs            # idempotency and bounded typed results
  errors.rs

src-tauri/src/services/portable_migration/
  mod.rs
  limits.rs
  format.rs
  age_envelope.rs
  catalog.rs
  schema_reader.rs
  target_writer.rs
  snapshot.rs
  transform.rs
  validate.rs
  staging.rs
  path_tokens.rs
  inspection_registry.rs
  activation_journal.rs
  recovery.rs
  fault.rs

src-tauri/src/services/secrets/
  material.rs
  device_key_store.rs
  rekey.rs
  baseline_conversion.rs

src-tauri/src/services/data_store/
  atomic_file.rs
  file_identity.rs

src-tauri/src/application/data_maintenance.rs
src-tauri/src/commands/data_migration.rs
src-tauri/src/ipc/dto/data_migration.rs
```

新增前端模块：

```text
src/features/settings/data-migration/
  DataMigrationSection.tsx
  ExportMigrationDialog.tsx
  ImportMigrationDialog.tsx
  ImportMigrationSummary.tsx
  MigrationMaintenanceScreen.tsx
  migrationViewModel.ts
  useDataMigrationController.ts

src/lib/api/dataMigration.ts
src/lib/types/dataMigration.ts
```

每个文件 SHOULD 控制在约 350 行以内。超出时按“格式/IO/策略/状态机”职责拆分，禁止形成同时持有 Tauri、SQLite、keyring、age 和 UI DTO 的单体 service。

## 全局执行规则

- 每个 task 先写 RED test，再写最小实现，再跑列出的 focused checks；不得一次实现后补测试。
- 每次开始前运行 `git status --short`，保留所有无关改动。只使用 task 中列出的 `git add -- <paths>`；不使用 `git add .` 或 `git add -A`。
- 下列 `git commit` 是建议提交边界，不代表本计划编写阶段执行提交。
- 任何失败输出、fixture 和截图不得包含真实凭据。secret fixture 只使用固定 canary。
- 结构版本、错误码、进度码、table policy 和 generated binding 发生变化时，Rust、TypeScript、fixture、前端文案映射和 contract gate 必须同一提交更新。
- 全部文件 IO、hash、SQLite、KDF 和 age 工作在 Rust 后台线程；React 主线程只处理 DTO 与状态。

## Task 0：冻结基线、ADR、依赖准入和 feature gate

**Files:**

- Create: `docs/superpowers/specs/2026-07-29-portable-migration-crypto-format-adr.md`
- Create: `scripts/portable-migration-baseline.test.mjs`
- Modify: `scripts/run-contract-tests.mjs`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Create: `src-tauri/tests/age_envelope_spike.rs`
- Modify: `docs/superpowers/audits/architecture-scale-dependency-lifecycle.json`
- Modify only if required by advisory result: `docs/superpowers/audits/dependency-advisory-exceptions.json`

- [ ] **Step 1: 写 baseline RED gate**

脚本必须从 migration registry 实际建库并断言 generation、writable schema、29 张当前表和每张表名；断言 `CROSS_DEVICE_ENCRYPTED_MIGRATION_SPEC.md` 的表矩阵覆盖全部表，并允许且仅允许额外的前置表 `app_secret_bindings`。脚本还检查 `SECURITY_EXPORT_IMPORT.md` 当前仍禁止 portable secret migration，避免实现计划被误读为已批准功能。

```powershell
node scripts/portable-migration-baseline.test.mjs
```

Expected RED：基线审计脚本和 age spike test 尚不存在。

- [ ] **Step 2: 完成 age spike 和 ADR**

在独立 test module 验证候选 `age` 精确版本能够：passphrase round trip、读取 header 后在执行 KDF 前拒绝超限 scrypt work factor、流式消费到认证 EOF、错误密码/截断统一分类。若 crate API 无法在 KDF 前限额，停止实现并更换版本/adapter；不得绕过规范。

ADR 锁定 `.rpd-move` v1 framing、标准 age binary、RFC4648 Base64、SemVer/UUID/RFC3339 规范形式、KDF 上限来源、24 个月 reader 支持责任和依赖回滚版本。`age`、用于进程内幂等摘要的 `hmac` 和仅用于负 trait 编译断言的 dev dependency `static_assertions`，都必须把 Step 2 通过的 resolved version 以 Cargo 精确等号约束写入，禁止 wildcard、范围或只依赖 lockfile 的松散声明。

- [ ] **Step 3: 登记依赖并保持功能关闭**

在 dependency lifecycle ledger 为 `age`、`hmac` 增加 resolved version、官方来源、owner、review date 和安全决策。若 advisory exception 必须新增，记录 advisory、影响分析、补偿控制、到期日和 owner；不能仅为让 gate 变绿而忽略。

- [ ] **Step 4: 验证并提交边界**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml age_envelope_spike -- --nocapture
pnpm.cmd run architecture:dependencies
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check-advisories.ps1
node scripts/portable-migration-baseline.test.mjs
git add -- docs/superpowers/specs/2026-07-29-portable-migration-crypto-format-adr.md scripts/portable-migration-baseline.test.mjs scripts/run-contract-tests.mjs src-tauri/tests/age_envelope_spike.rs src-tauri/Cargo.toml src-tauri/Cargo.lock docs/superpowers/audits/architecture-scale-dependency-lifecycle.json docs/superpowers/audits/dependency-advisory-exceptions.json
git commit -m "docs: freeze portable migration crypto contract"
```

## Task 1：抽取可复用的 Windows 原子文件与文件身份原语

**Files:**

- Create: `src-tauri/src/services/data_store/atomic_file.rs`
- Create: `src-tauri/src/services/data_store/file_identity.rs`
- Modify: `src-tauri/src/services/data_store/mod.rs`
- Modify: `src-tauri/src/services/data_store/config.rs`
- Modify: `src-tauri/src/persistence/upgrade_recovery_executor.rs`
- Test: inline unit/fault tests in the new modules

- [ ] **Step 1: 先写 Windows/portable adapter contract tests**

覆盖 create-new publish、replace-existing preserving old file、选择时 absent 但发布前出现文件、选择时 existing 但 file ID 被替换、journal write/readback、database replace with rollback path、replace 调用返回未知后的文件身份判定、父目录/leaf 验证、不同卷拒绝、junction/reparse escape、大小写/8.3 alias 和打开句柄后的文件替换。测试接口为：

```rust
trait AtomicFilePublishPort { fn publish(&self, prepared: &File, target: &ApprovedLeaf) -> Result<PublishEvidence, AtomicFileError>; }
trait AtomicJournalPort { fn publish_and_readback(&self, bytes: &[u8], target: &ApprovedLeaf) -> Result<Vec<u8>, AtomicFileError>; }
trait AtomicDatabaseReplacePort { fn replace(&self, active: &ApprovedFile, staged: &ApprovedFile, rollback: &ApprovedLeaf) -> Result<ReplaceEvidence, AtomicFileError>; }
```

`FileIdentity` 至少含 volume serial、file ID、length、SHA-256；路径只用于受控 UI，不作为身份。

- [ ] **Step 2: 实现单一 Windows adapter**

复用 `config.rs` 已验证的 `ReplaceFileW` 路径，统一 `sync_all`、CreateNew 临时文件、replace/create 两种发布语义和回读验证。非 Windows 只为单元测试提供同目录 adapter，产品 capability 仍为 Windows only。禁止三处保留私有 `replace_existing_file`。

- [ ] **Step 3: 回归已有 config/upgrade**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml services::data_store::atomic_file -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml services::data_store::config -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml persistence::upgrade -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
git add -- src-tauri/src/services/data_store/atomic_file.rs src-tauri/src/services/data_store/file_identity.rs src-tauri/src/services/data_store/mod.rs src-tauri/src/services/data_store/config.rs src-tauri/src/persistence/upgrade_recovery_executor.rs
git commit -m "refactor: centralize durable file replacement"
```

## Task 2：修复 installation lease 与系统凭据错误分类

**Files:**

- Create: `src-tauri/src/services/secrets/device_key_store.rs`
- Create: `src-tauri/src/services/secrets/device_key_journal.rs`
- Modify: `src-tauri/src/services/secrets/keychain.rs`
- Modify: `src-tauri/src/services/secrets/mod.rs`
- Modify: `src-tauri/src/services/data_store/types.rs`
- Modify: `src-tauri/src/services/data_store/decision.rs`
- Modify: `src-tauri/src/services/data_store/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/app_composition.rs`
- Create: `scripts/device-key-startup-boundary.test.mjs`
- Modify: `scripts/run-contract-tests.mjs`

- [ ] **Step 1: 写错误矩阵和启动顺序 RED tests**

用 fake credential backend 覆盖 `NotFound/Unavailable/PermissionDenied/Corrupt/Unsupported/Internal`：全新安装仅 `NotFound` 可调用 create；已有 DB/backup/upgrade journal/import journal 时 `NotFound` 也必须进入 recovery 且 create count 为零。create 后 readback 不一致必须失败且不 commit active pointer。Node boundary test 断言 `lib.rs` 顺序为 resolve config -> acquire `InstallationLease` -> inspect data-store recovery facts -> context-aware key load/create -> data store，并禁止 setup 直接调用旧 `load_or_create_data_key`。

- [ ] **Step 2: 实现版本化 key entry**

实现 legacy `local-data-key-v1` reader、新的 `device-data-key:<key-id>` entry 和版本化 active pointer。应用预生成 ID，`create_pending(id)` 只创建未激活 entry 且拒绝覆盖；`commit_active` 只切 pointer。使用 `subtle` 常量时间比较 readback。新增固定 config-dir bootstrap journal，phase 为 `planned/key_created/database_validated/active_committed/completed`，只含 key ID、candidate identity 和时间，不含 key material。first-run 测试固定 `journal planned -> pending key -> DB/Local Key validate -> active pointer -> config/marker -> journal cleanup`，并覆盖每个边界失败后的 startup recovery。所有 keyring 调用继续通过 `BlockingExecutor`，公开错误只保留稳定类别。

- [ ] **Step 3: 调整 setup 所有分支的 lease 生命周期**

包括 ready、first-run、upgrade/recovery 和错误 UI 分支；lease 由 `DataStoreRuntimeOwner` 持有到 runtime 关闭。只有已证明的 first-run 可把 key `NotFound` 转成 create；其他 key error/missing 分支不得注册代理、collector/monitor runner 或 writable persistence。

- [ ] **Step 4: 验证与提交**

```powershell
node scripts/device-key-startup-boundary.test.mjs
cargo test --manifest-path src-tauri/Cargo.toml services::secrets::device_key_store -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml services::secrets::device_key_journal -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml installation_lease -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
git add -- src-tauri/src/services/secrets/device_key_store.rs src-tauri/src/services/secrets/device_key_journal.rs src-tauri/src/services/secrets/keychain.rs src-tauri/src/services/secrets/mod.rs src-tauri/src/services/data_store/types.rs src-tauri/src/services/data_store/decision.rs src-tauri/src/services/data_store/mod.rs src-tauri/src/lib.rs src-tauri/src/app_composition.rs scripts/device-key-startup-boundary.test.mjs scripts/run-contract-tests.mjs
git commit -m "fix: acquire installation lease before device key"
```

## Task 3：收口 secret material、key ID 与业务访问边界

**Files:**

- Create: `src-tauri/src/services/secrets/material.rs`
- Modify: `src-tauri/src/services/secrets/mod.rs`
- Modify: `src-tauri/src/services/secrets/vault.rs`
- Modify: `src-tauri/src/services/secrets/crypto.rs`
- Modify: `src-tauri/src/application/credentials.rs`
- Modify: `src-tauri/src/application/app_services.rs`
- Modify: `src-tauri/src/app_composition.rs`
- Modify: `src-tauri/src/runtime_composition.rs`
- Modify: `src-tauri/src/application/command_facades/local_proxy.rs`
- Modify: `src-tauri/src/application/command_facades/provider_drafts.rs`
- Modify: `src-tauri/src/application/command_facades/remote_keys.rs`
- Modify: `src-tauri/src/application/command_facades/station_collection.rs`
- Modify: `src-tauri/src/commands/data_store_startup.rs`
- Modify: `src-tauri/src/services/collectors/mod.rs`
- Modify: `src-tauri/src/services/data_store/generation_upgrade.rs`
- Modify: `src-tauri/src/services/proxy/routing_repository.rs`
- Modify: `src-tauri/src/services/proxy/startup.rs`
- Modify: `src-tauri/src/services/proxy/startup_auto_start.rs`
- Modify: `src-tauri/src/services/remote_keys.rs`
- Modify: `src-tauri/src/services/station_collectors.rs`

- [ ] **Step 1: 添加 compile-time 与 behavior tests**

用 `static_assertions` 或 compile-fail test 证明 `SecretKeyMaterial` 不实现 `Copy/Clone/Serialize`，`Debug` 只输出 redacted 类型名；drop zeroizes。测试 `SecretKeyResolver::with_key(id, |key| ...)` 的借用不逃逸，未知 key ID 和 encryption version fail closed。

- [ ] **Step 2: 建立窄接口**

`SecretKeyMaterial(Zeroizing<[u8;32]>)` 只在 secrets module 内暴露字节。`SecretService` 提供 encrypt/decrypt/validate/rekey 行级操作；业务 service 持有 `Arc<dyn CredentialVault>` 或更窄端口，不持有 key。不要把 `SecretManager` 扩成包含迁移、数据库和 UI 的 god object。

- [ ] **Step 3: 逐个迁移 composition 调用者**

先改 credential service，再改 collectors、remote keys、proxy startup 和 command facade；每个调用者删除 copied `[u8;32]` 后运行 focused test。测试 helper 可通过 in-memory resolver 注入固定 key，但 production constructor 不再接收裸 key。

- [ ] **Step 4: 验证无裸 key 生产路径**

```powershell
rg -n 'data_key\(|\*.*data_key|data_key: \[u8; 32\]' src-tauri/src
cargo test --manifest-path src-tauri/Cargo.toml services::secrets -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml application::credentials -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
git add -- src-tauri/src/services/secrets/material.rs src-tauri/src/services/secrets/mod.rs src-tauri/src/services/secrets/vault.rs src-tauri/src/services/secrets/crypto.rs src-tauri/src/application/credentials.rs src-tauri/src/application/app_services.rs src-tauri/src/app_composition.rs src-tauri/src/runtime_composition.rs src-tauri/src/application/command_facades/local_proxy.rs src-tauri/src/application/command_facades/provider_drafts.rs src-tauri/src/application/command_facades/remote_keys.rs src-tauri/src/application/command_facades/station_collection.rs src-tauri/src/commands/data_store_startup.rs src-tauri/src/services/collectors/mod.rs src-tauri/src/services/data_store/generation_upgrade.rs src-tauri/src/services/proxy/routing_repository.rs src-tauri/src/services/proxy/startup.rs src-tauri/src/services/proxy/startup_auto_start.rs src-tauri/src/services/remote_keys.rs src-tauri/src/services/station_collectors.rs
git commit -m "refactor: keep device keys behind secret services"
```

Expected `rg`：只允许 secrets module 内部、测试 fixture 和明确审阅的 legacy conversion adapter 命中。

## Task 4：实现通用 SecretRekeyService

**Files:**

- Create: `src-tauri/src/services/secrets/rekey.rs`
- Modify: `src-tauri/src/services/secrets/mod.rs`
- Modify: `src-tauri/src/models/secrets.rs`
- Create: `src-tauri/tests/secret_rekey.rs`

- [ ] **Step 1: 写三密钥与失败矩阵并确认 RED**

覆盖 source -> target 正常、逐行新 nonce、错误 AAD、错误 key、nonce 长度、unknown encryption version、中间第 N 行失败、destination 已存在、取消、输入只读、输出不可激活。断言无全量 plaintext vector，错误/Debug 不含 canary。先运行 focused test，Expected RED：`SecretRekeyService` 和 policy/report 类型尚不存在。

- [ ] **Step 2: 实现流式 rekey**

每行 plaintext 使用 `Zeroizing<Vec<u8>>`，AAD 由 canonical `(scope, owner_id, kind, version)` builder 产生。输出只写 CreateNew 文件/受控 writer；report 仅含 from/to key ID、成功行数和稳定 code。policy 明确 include/drop/reset，不允许调用者传任意闭包决定未知 secret。

- [ ] **Step 3: 验证**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml secret_rekey -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml services::secrets -- --nocapture
git add -- src-tauri/src/services/secrets/rekey.rs src-tauri/src/services/secrets/mod.rs src-tauri/src/models/secrets.rs src-tauri/tests/secret_rekey.rs
git commit -m "feat: add reusable secret rekey service"
```

## Task 5：交付可恢复的 encrypted-secret 安全基线转换

**Files:**

- Create after preflight confirms free: `src-tauri/src/persistence/migrations/0017_encrypted_secret_baseline.sql`
- Create: `src-tauri/src/services/secrets/baseline_conversion.rs`
- Modify: `src-tauri/src/persistence/migrations.rs`
- Modify: `src-tauri/src/persistence/runtime.rs`
- Modify: `src-tauri/src/persistence/upgrade_journal.rs`
- Modify: `src-tauri/src/persistence/upgrade_recovery_plan.rs`
- Modify: `src-tauri/src/persistence/upgrade_recovery_executor.rs`
- Modify: `src-tauri/src/services/data_store/generation_upgrade.rs`
- Modify: `src-tauri/src/services/data_store/backup.rs`
- Modify: `src-tauri/src/services/secrets/validation.rs`
- Modify: `src-tauri/src/application/settings.rs`
- Modify: `src-tauri/src/models/settings.rs`
- Modify: `src-tauri/src/persistence/stores/settings_store.rs`
- Modify: `src-tauri/src/persistence/stores/credential_store.rs`
- Modify: `src-tauri/src/lib.rs`
- Create: `scripts/encrypted-secret-baseline.test.mjs`
- Modify: `scripts/run-contract-tests.mjs`
- Modify: `scripts/settings-local-access-key.test.mjs`

- [ ] **Step 1: 锁定 conversion 状态机 RED tests**

测试当前 pre-baseline schema（状态监控合并后为 v10）有/无 secrets、有 legacy Local Key、`stations.api_key`/`station_keys.api_key`/`station_credentials.login_password` legacy 明文、明文与现有 secret 冲突、错误密文、错误 key、backup 失败、每个 journal 原子边界崩溃、target validation 失败、active replacement 未知、旧 binary compatibility。断言 active pre-baseline schema 从不被 sqlx migrator 原地推进到 encrypted baseline schema。

- [ ] **Step 2: 添加结构 migration，但禁止直接激活半成品**

下一个空闲 migration（状态监控合并后当前应为 `0011`）只提供新库构建所需结构：可回填的 `key_id/encryption_version` 中间形态与 `app_secret_bindings`。schema compatibility 仍保持 pre-baseline，直到 application conversion 重建 `secrets` 为最终 `NOT NULL`、验证全部密文、加密 Local Key、迁移并清空三个 legacy credential 列及 `settings.local_key`、完成重建/VACUUM 后才提交新 profile。明文与已有 secret 冲突时 fail closed；fresh database 也走同一 finalizer。

- [ ] **Step 3: 扩展现有 upgrade journal/recovery 框架**

不要把 generation 1 -> 2 的 phase 强塞进安全转换。抽取共享 atomic journal/evidence，新增闭合 conversion kind 和 phase；source 只读，verified backup 带“旧安全格式、仅本机恢复”元数据，target 在新文件构建。失败保留 source、backup、journal 和可恢复 candidate。

- [ ] **Step 4: 改 startup gate**

setup 在 persistence runtime 之前检测 schema/profile：pre-baseline schema 进入 baseline converter；encrypted baseline 完整 profile 才 writable；半转换或 journal 矛盾进入 recovery UI。旧 binary 看到 encrypted baseline schema 必须 compatibility fail closed。

- [ ] **Step 5: 验证明文清理与恢复矩阵**

```powershell
node scripts/encrypted-secret-baseline.test.mjs
node scripts/settings-local-access-key.test.mjs
cargo test --manifest-path src-tauri/Cargo.toml baseline_conversion -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml persistence::upgrade -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml services::data_store -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
git add -- src-tauri/src/persistence/migrations/0017_encrypted_secret_baseline.sql src-tauri/src/services/secrets/baseline_conversion.rs src-tauri/src/persistence/migrations.rs src-tauri/src/persistence/runtime.rs src-tauri/src/persistence/upgrade_journal.rs src-tauri/src/persistence/upgrade_recovery_plan.rs src-tauri/src/persistence/upgrade_recovery_executor.rs src-tauri/src/services/data_store/generation_upgrade.rs src-tauri/src/services/data_store/backup.rs src-tauri/src/services/secrets/validation.rs src-tauri/src/application/settings.rs src-tauri/src/models/settings.rs src-tauri/src/persistence/stores/settings_store.rs src-tauri/src/persistence/stores/credential_store.rs src-tauri/src/lib.rs scripts/encrypted-secret-baseline.test.mjs scripts/settings-local-access-key.test.mjs scripts/run-contract-tests.mjs
git commit -m "feat: convert databases to encrypted secret baseline"
```

## Task 6：实现 DataMaintenanceCoordinator 与两层 mutation admission

**Files:**

- Create: `src-tauri/src/application/data_maintenance.rs`
- Modify: `src-tauri/src/application/mod.rs`
- Modify: `src-tauri/src/background_tasks/operation.rs`
- Modify: `src-tauri/src/persistence/runtime.rs`
- Modify: `src-tauri/src/persistence/runtime_lifecycle.rs`
- Modify: `src-tauri/src/persistence/write_coordinator.rs`
- Modify: `src-tauri/src/ipc/registry.rs`
- Modify: `src-tauri/src/services/proxy/runtime.rs`
- Modify: `src-tauri/src/services/station_collectors.rs`
- Modify: `src-tauri/src/services/channel_monitors/mod.rs`
- Modify: `src-tauri/src/services/channel_monitors/probe.rs`

- [ ] **Step 1: 写纯状态机和并发 RED tests**

覆盖所有合法/非法转换、互斥 operation、export/inspection 不阻塞业务写、prepare freeze 前失败回 normal、freeze 后所有 mutation/runner/proxy/updater 拒绝、已有任务必须 cancel+join、write checkout drain timeout。用 barrier 测试 command admission 与 persistence admission 同时生效。

- [ ] **Step 2: 建立 coordinator lease**

用 RAII lease 管理 `exporting/inspecting_import/preparing_import`，用不可逆 commit barrier 进入 `activation_pending`。禁止多个 `AtomicBool` 分散判断。generated command registry 的 mutation metadata 统一进入 admission middleware；读取命令仍可服务维护页所需状态。

- [ ] **Step 3: 增加 persistence 防御层与 freeze primitive**

`PersistenceRuntime::freeze_for_activation` 固定执行：block new writes -> drain active sessions -> checkpoint/truncate WAL -> close pool -> verify no non-empty sidecar。超时或无法证明关闭返回稳定错误；关闭后同进程不得 reopen。

- [ ] **Step 4: 验证**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml data_maintenance -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml persistence::runtime_lifecycle -- --nocapture
pnpm.cmd run architecture:commands
cargo check --manifest-path src-tauri/Cargo.toml
git add -- src-tauri/src/application/data_maintenance.rs src-tauri/src/application/mod.rs src-tauri/src/background_tasks/operation.rs src-tauri/src/persistence/runtime.rs src-tauri/src/persistence/runtime_lifecycle.rs src-tauri/src/persistence/write_coordinator.rs src-tauri/src/ipc/registry.rs src-tauri/src/services/proxy/runtime.rs src-tauri/src/services/station_collectors.rs src-tauri/src/services/channel_monitors/mod.rs src-tauri/src/services/channel_monitors/probe.rs src-tauri/src/lib.rs
git commit -m "feat: coordinate exclusive data maintenance"
```

## Task 7：实现 v1 limits、framing 与严格 Manifest

**Files:**

- Create: `src-tauri/src/services/portable_migration/mod.rs`
- Create: `src-tauri/src/services/portable_migration/limits.rs`
- Create: `src-tauri/src/services/portable_migration/format.rs`
- Modify: `src-tauri/src/services/mod.rs`
- Create: `src-tauri/tests/fixtures/portable-migration/v1/manifest-valid.json`
- Create: malformed manifest/framing fixtures under the same directory

- [ ] **Step 1: 写 parser RED matrix**

覆盖 magic、big-endian length、checked arithmetic、截断、尾随字节、重复 JSON key、未知顶层字段、feature-name、SemVer、UUIDv7、UTC RFC3339、Base64 padding、category 集合、recordCounts exact key set、JSON depth、每项和总量上限，以及 export 2h/inspection 30m/prepare 2h/drain 30s 的 operation limits。对每个数值边界测试 `limit-1/limit/limit+1`。

- [ ] **Step 2: 实现唯一 `PortableMigrationLimitsV1`**

export、import、capability 和 fixture builder 全部引用同一 struct/constants。Manifest 使用专用 duplicate-key rejecting deserializer 和 `deny_unknown_fields`；extensions 只保留受限 `serde_json::Value`，不得映射回顶层。

- [ ] **Step 3: 实现流式 framing reader/writer**

transport key 直接进入 `SecretKeyMaterial` 等价的 zeroizing 32-byte wrapper；不经过 JSON/Base64/String。reader 在 age adapter 外只处理 `Read`，writer 只处理 `Write`，便于 fuzz 和 fixture 测试。

- [ ] **Step 4: 验证**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml portable_migration::format -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml portable_migration::limits -- --nocapture
git add -- src-tauri/src/services/portable_migration/mod.rs src-tauri/src/services/portable_migration/limits.rs src-tauri/src/services/portable_migration/format.rs src-tauri/src/services/mod.rs src-tauri/tests/fixtures/portable-migration/v1
git commit -m "feat: define strict portable migration format"
```

## Task 8：实现 exhaustive MigrationDataCatalog 与 occupancy

**Files:**

- Create: `src-tauri/src/services/portable_migration/catalog.rs`
- Create: `src-tauri/src/services/portable_migration/transform.rs`
- Modify: `src-tauri/src/services/portable_migration/mod.rs`
- Modify: `src-tauri/src/persistence/migrations.rs`
- Create: `scripts/portable-migration-catalog.test.mjs`
- Modify: `scripts/run-contract-tests.mjs`

- [ ] **Step 1: 生成当前 schema 并写全覆盖 RED test**

从实际 `sqlite_schema` 读取 29 张当前表，加上 Task 5 的 `app_secret_bindings` 后应为 30 张。比较 catalog，不用手写“期望缺失为空”的假测试。枚举 table policy、敏感列策略、setting key allowlist、`(scope,kind)` allowlist、占用查询和复制依赖阶段；新增表/列/setting/secret kind 未声明时测试失败。

- [ ] **Step 2: 实现 spec 11.5 的精确矩阵**

`provider_drafts`/preview 排除但计入 occupancy；`collector_model_facts` reset；`group_rate_records` optional history；Local Key、token/session/cookie、device path/runtime state reset/exclude。未知 secret、孤立引用、未知 setting 一律 policy violation，不能静默跳过。

- [ ] **Step 3: 实现纯 transform 与 canary scanner**

transform 输入/输出为版本化 row model，不接受自由 SQL。JSON 递归脱敏器有 depth/bytes 限制并复用现有 redaction 规则。canary scanner 同时扫描普通字段、JSON、SQLite pages/freelist 重建后的最终字节。

- [ ] **Step 4: 验证**

```powershell
node scripts/portable-migration-catalog.test.mjs
cargo test --manifest-path src-tauri/Cargo.toml portable_migration::catalog -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml portable_migration::transform -- --nocapture
git add -- src-tauri/src/services/portable_migration/catalog.rs src-tauri/src/services/portable_migration/transform.rs src-tauri/src/services/portable_migration/mod.rs src-tauri/src/persistence/migrations.rs scripts/portable-migration-catalog.test.mjs scripts/run-contract-tests.mjs
git commit -m "feat: classify every portable migration table"
```

## Task 9：实现版本化 PortableSchemaReader 与受信任 TargetWriter

**Files:**

- Create: `src-tauri/src/services/portable_migration/schema_reader.rs`
- Create: `src-tauri/src/services/portable_migration/target_writer.rs`
- Create: `src-tauri/src/services/portable_migration/validate.rs`
- Modify: `src-tauri/src/services/portable_migration/mod.rs`
- Modify: `src-tauri/src/persistence/mod.rs`
- Modify: `src-tauri/src/persistence/runtime.rs`
- Create: schema fingerprint fixtures under `src-tauri/tests/fixtures/portable-migration/v1/`

- [ ] **Step 1: 写不可信 SQLite RED suite**

覆盖 unknown table/column/index/FK、trigger、view、virtual table、malformed DDL、extension、超大字段、超多行、恶意 JSON、VDBE operation limit、foreign key break 和 schema 自报版本欺骗。证明 reader 不运行 package migrations/DDL/ATTACH，不读取 `sqlite_schema.sql` 后执行。

- [ ] **Step 2: 实现 compatibility registry**

`PortableMigrationCompatibilityRegistry` 以精确 `(formatVersion, portableSchemaProfile, databaseGeneration, schemaVersion range, exportPolicyVersion, encryptionVersion, requiredFeatures)` 选择 reader；当前 v1/profile reader 使用固定参数化 SELECT 与结构指纹。无精确 reader 返回对应 importer/policy/encryption/feature/schema unsupported code。

- [ ] **Step 3: 实现 staged domain record API**

reader 按 catalog stage 流式产出 bounded row；writer 先跑当前受信任 migrations，再按 spec 13.5 的 8 个依赖阶段写入单一事务。Station/Key secret ref 先置空后回填，允许 deferred FK，禁止 `foreign_keys=OFF`。internal/default/built-in 行由当前 binary 创建，不信任包内版本行。

- [ ] **Step 4: 完成 target 关闭态验证**

writer commit 后执行 quick/foreign-key/schema/secret/canary/transport-residue 检查，随后 `VACUUM INTO` 新文件，切到无 WAL sidecar关闭态并再次验证。

- [ ] **Step 5: 验证**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml portable_migration::schema_reader -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml portable_migration::target_writer -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml portable_migration::validate -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
git add -- src-tauri/src/services/portable_migration/schema_reader.rs src-tauri/src/services/portable_migration/target_writer.rs src-tauri/src/services/portable_migration/validate.rs src-tauri/src/services/portable_migration/mod.rs src-tauri/src/persistence/mod.rs src-tauri/src/persistence/runtime.rs src-tauri/tests/fixtures/portable-migration/v1
git commit -m "feat: rebuild imports through trusted schema writers"
```

## Task 10：实现一致性导出 snapshot、策略转换与 transport rekey

**Files:**

- Create: `src-tauri/src/services/portable_migration/snapshot.rs`
- Modify: `src-tauri/src/services/portable_migration/transform.rs`
- Modify: `src-tauri/src/services/secrets/rekey.rs`
- Modify: `src-tauri/src/persistence/backup.rs`
- Create: `src-tauri/src/application/data_migration/export_service.rs`
- Create: `src-tauri/src/application/data_migration/errors.rs`
- Create: `src-tauri/src/application/data_migration/mod.rs`
- Modify: `src-tauri/src/application/mod.rs`

- [ ] **Step 1: 写 snapshot 并发与导出策略 RED tests**

在 Online Backup 期间并发写入，证明 snapshot 是一致性边界且后续写不进入包；直接 `fs::copy` WAL 数据库的 guard test 必须失败。覆盖历史 on/off、所有 table policy、未知 setting/secret fail closed、source secret N/2 解密失败、空间公式 overflow 和取消。

- [ ] **Step 2: 实现 export pipeline 到 portable SQLite B**

顺序固定为 preflight -> maintenance lease -> source secret 全验证 -> Online Backup A -> transform working copy -> source-to-transport 逐行 rekey -> VACUUM/rebuild B -> full validation。Transport key 每次 CSPRNG 生成，ID 为 `transport:<uuidv7>`；任意失败删除未发布工件并 zeroize。

- [ ] **Step 3: 证明导出期间业务仍可写**

只在 Online Backup 建立快照时使用现有 persistence 协调；之后释放数据库工作许可。UI 结果记录 snapshot timestamp，不停止代理。

- [ ] **Step 4: 验证**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml portable_export -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml persistence::backup -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
git add -- src-tauri/src/services/portable_migration/snapshot.rs src-tauri/src/services/portable_migration/transform.rs src-tauri/src/services/secrets/rekey.rs src-tauri/src/persistence/backup.rs src-tauri/src/application/data_migration/export_service.rs src-tauri/src/application/data_migration/errors.rs src-tauri/src/application/data_migration/mod.rs src-tauri/src/application/mod.rs
git commit -m "feat: build portable migration snapshots"
```

## Task 11：实现 age envelope、完整回读自检与原子导出发布

**Files:**

- Create: `src-tauri/src/services/portable_migration/age_envelope.rs`
- Create: `src-tauri/src/services/portable_migration/staging.rs`
- Modify: `src-tauri/src/services/portable_migration/format.rs`
- Modify: `src-tauri/src/application/data_migration/export_service.rs`
- Modify: `src-tauri/src/services/data_store/atomic_file.rs`

- [ ] **Step 1: 写 end-to-end export RED tests**

覆盖 passphrase scalar/UTF-8 边界、错误确认、age 写中断、flush/sync/publish 失败、已有目标未批准覆盖、目标被替换、package self-test 失败、cancel、partial cleanup failure。成功必须重新打开刚写出的 encrypted partial，解密到独立 scratch，消费认证 EOF，并重做 manifest/hash/SQLite/secret 检查。

- [ ] **Step 2: 实现 age adapter**

adapter 只接受 zeroizing passphrase 和 bounded `Read/Write`；KDF work factor 在昂贵计算前检查。所有 authentication/password/truncation 映射同一 public code，内部不得记录 header/password。

- [ ] **Step 3: 实现同目录原子发布**

通过 Task 1 port 发布 `.<leaf>.<exportId>.partial`。create-new 与用户已批准 replace 是两条显式路径；任何不支持可证明原子 replace 的卷失败并保留旧文件。只有 publish evidence 回读确认后 operation completed。

- [ ] **Step 4: 验证**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml age_envelope -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml portable_export_package -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml atomic_file -- --nocapture
git add -- src-tauri/src/services/portable_migration/age_envelope.rs src-tauri/src/services/portable_migration/staging.rs src-tauri/src/services/portable_migration/format.rs src-tauri/src/application/data_migration/export_service.rs src-tauri/src/services/data_store/atomic_file.rs
git commit -m "feat: publish self-verified encrypted migration packages"
```

## Task 12：实现 path token、inspection 和 typed result registries

**Files:**

- Create: `src-tauri/src/services/portable_migration/path_tokens.rs`
- Create: `src-tauri/src/services/portable_migration/inspection_registry.rs`
- Create: `src-tauri/src/application/data_migration/registry.rs`
- Modify: `src-tauri/src/background_tasks/operation.rs`
- Modify: `src-tauri/src/application/data_migration/mod.rs`

- [ ] **Step 1: 写 registry RED tests**

使用 paused time 覆盖 10 分钟 token/inspection、30 分钟 terminal result、每类 64 项容量、一次消费、跨 operation 类型、进程 nonce、GC zeroize、same idempotency+same digest、same key+different digest、operation completed 但 result missing。并发两个相同 prepare 必须只有一个 owner；成功消费必须移动出一个不可 Clone 的 `ImportPreparationLease`，而不是复制或提前清零 transport key。

- [ ] **Step 2: 实现句柄绑定 token**

import token 持有禁止共享删除的只读文件句柄和稳定身份；export token 持有规范父目录句柄、批准 leaf，以及选择时 absent 或已批准 existing file identity。消费从持有句柄执行，不按字符串路径 reopen；发布前目标状态/身份变化返回 `selected_file_changed`。已消费/过期/类型不匹配分别映射规范 code。

- [ ] **Step 3: 实现无密码摘要的幂等表**

启动时生成 process-local HMAC key；摘要包含 command kind、句柄身份、options/mode/confirmation 语义和 HMAC(passphrase)，不存原文。先保留 idempotency binding，再消费 token/inspection。结果 registry 只存 allowlist DTO。

- [ ] **Step 4: 迁移进度使用 typed projection**

保留通用 `OperationRegistry`，但 portable facade 只接受闭合 progress enum；三类 start 显式使用 limits 中的 2h/30m/2h deadline，不能继承全局 30s。`get_portable_migration_operation` 校验 owner 并返回 code/counters/terminal，不暴露通用自由文本 message。paused-time test 证明同一阶段更新需同时满足距上次至少 250 ms 且变化至少 1%，phase/terminal 可立即发送，KDF 只发开始/结束且不伪造百分比。

- [ ] **Step 5: 验证**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml portable_migration::path_tokens -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml portable_migration::inspection_registry -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml data_migration::registry -- --nocapture
git add -- src-tauri/src/services/portable_migration/path_tokens.rs src-tauri/src/services/portable_migration/inspection_registry.rs src-tauri/src/application/data_migration/registry.rs src-tauri/src/background_tasks/operation.rs src-tauri/src/application/data_migration/mod.rs
git commit -m "feat: bound portable migration handles and results"
```

## Task 13：实现防御式导入 inspection

**Files:**

- Create: `src-tauri/src/application/data_migration/import_service.rs`
- Modify: `src-tauri/src/services/portable_migration/age_envelope.rs`
- Modify: `src-tauri/src/services/portable_migration/schema_reader.rs`
- Modify: `src-tauri/src/services/portable_migration/validate.rs`
- Modify: `src-tauri/src/services/portable_migration/staging.rs`
- Modify: `src-tauri/src/services/portable_migration/inspection_registry.rs`

- [ ] **Step 1: 写恶意 package RED matrix**

覆盖 2.25 GiB 文件上限、恶意 KDF、错误 password/tag、truncation、Manifest/SQLite/hash 修改、integer overflow、non-SQLite、畸形 SQLite、schema object 攻击、资源上限、TOCTOU 内容变化、连续 5 次失败退避。所有 case 断言 active DB/hash 不变且无 transport key 落盘。

- [ ] **Step 2: 实现 staged decrypt**

从 token 句柄读 age；Manifest 留在 bounded memory；SQLite 只写受控 ACL 的 `portable.sqlite3.partial`，认证 EOF 后 rename。验证顺序严格遵循 spec 13.4，任何前置失败不打开 writable DB。

- [ ] **Step 3: 注册 inspection**

只在全部验证和 transport secret 逐条解密成功后注册；entry 持有 zeroizing transport key、input identity、staging identity/hash、reader ID 和非敏感 summary。消费时所有权移动给 prepare lease，prepare 结束后统一 zeroize/cleanup。UI 只看到 app version/time/categories/counts/history flag。

- [ ] **Step 4: 验证**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml portable_import_inspection -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml malicious_portable_package -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
git add -- src-tauri/src/application/data_migration/import_service.rs src-tauri/src/services/portable_migration/age_envelope.rs src-tauri/src/services/portable_migration/schema_reader.rs src-tauri/src/services/portable_migration/validate.rs src-tauri/src/services/portable_migration/staging.rs src-tauri/src/services/portable_migration/inspection_registry.rs
git commit -m "feat: inspect migration packages defensively"
```

## Task 14：实现目标库重建、target rekey 与导入模式校验

**Files:**

- Modify: `src-tauri/src/application/data_migration/import_service.rs`
- Modify: `src-tauri/src/services/portable_migration/target_writer.rs`
- Modify: `src-tauri/src/services/portable_migration/catalog.rs`
- Modify: `src-tauri/src/services/portable_migration/staging.rs`
- Modify: `src-tauri/src/services/secrets/rekey.rs`
- Modify: `src-tauri/src/application/settings.rs`

- [ ] **Step 1: 写 restore/replace 与三密钥 RED tests**

`restoreIntoEmpty` 必须对每张用户表、未知 setting、draft 和非设备 secret 做 occupancy；不能只看 stations。`replaceCurrent` 要求精确确认文本。三把 key 证明 source/transport/target 只能解各自阶段，目标无 source/transport key ID/ciphertext canary，Local Key 新生成且 start-on-launch false。

- [ ] **Step 2: 在 active 同卷构建最终 target**

inspection 消费后按 reader/writer 依赖阶段复制；逐行 transport -> target，不形成明文集合。AppData scratch、active 卷 staging 和 backup 卷分别做 checked free-space preflight；active 同卷 staging 无法建立时失败，不退化跨卷 copy。

- [ ] **Step 3: 完整 target self-validation**

关闭 writer、VACUUM INTO `target.sqlite3`、验证无 sidecar、quick/FK/schema/catalog/secrets/canary/record count；记录 target file identity/hash。失败时 current active 和 target device active key pointer 均不变。

- [ ] **Step 4: 验证**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml portable_import_target -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml portable_import_three_keys -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml migration_occupancy -- --nocapture
git add -- src-tauri/src/application/data_migration/import_service.rs src-tauri/src/services/portable_migration/target_writer.rs src-tauri/src/services/portable_migration/catalog.rs src-tauri/src/services/portable_migration/staging.rs src-tauri/src/services/secrets/rekey.rs src-tauri/src/application/settings.rs
git commit -m "feat: rebuild portable imports with target keys"
```

## Task 15：实现 verified backup、WAL freeze 与 prepared journal 提交

**Files:**

- Create: `src-tauri/src/services/portable_migration/activation_journal.rs`
- Create: `src-tauri/src/services/portable_migration/fault.rs`
- Modify: `src-tauri/src/application/data_migration/import_service.rs`
- Modify: `src-tauri/src/application/data_maintenance.rs`
- Modify: `src-tauri/src/services/data_store/backup.rs`
- Modify: `src-tauri/src/persistence/runtime.rs`
- Modify: `src-tauri/src/services/data_store/atomic_file.rs`

- [ ] **Step 1: 写 prepare commit-order RED tests**

逐边界注入失败：target validated、backup create/validate、mutation admission、runner cancel/join、proxy drain、write drain、WAL checkpoint、pool close、sidecar check、active identity/hash、journal write/sync/replace/readback、coordinator transition。每个 case 断言 active main/WAL 状态、target/backup 保留策略、当前进程是否拒写、下次启动决策。

- [ ] **Step 2: 实现闭合 journal schema**

顶层 `deny_unknown_fields`、重复 key 拒绝、64 KiB、固定 config dir、ACL、UUIDv7/timestamp/hash/file ID/path identity 验证。phase 只允许 spec 的图，并验证 `observedRollbackFileId` 等 phase-specific null/required shape；payload 保存 prepare 时的 target key ID 以及 active/staged/backup 的 file ID、length、SHA-256。canonical checksum 不替代候选文件 SHA-256；transport/password/key material 不进入 journal。

- [ ] **Step 3: 按唯一顺序提交 prepared**

target validate -> verified backup -> block admission -> cancel/join runners -> proxy drain -> write drain -> checkpoint/truncate -> close all SQLite -> verify/delete empty sidecars -> hash stable active -> publish/readback `prepared` -> coordinator `activation_pending` -> typed result `restartRequired=true`。第一个不可逆点调用 operation commit barrier。

任何 freeze 后、journal 确认前失败保持拒写并要求 restart，不尝试 hot reopen。两种 import mode 都必须有 verified backup；backup UI 元数据不含 secret。

- [ ] **Step 4: 验证**

```powershell
cargo test --manifest-path src-tauri/Cargo.toml activation_prepare -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml wal_freeze -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml services::data_store::backup -- --nocapture
git add -- src-tauri/src/services/portable_migration/activation_journal.rs src-tauri/src/services/portable_migration/fault.rs src-tauri/src/application/data_migration/import_service.rs src-tauri/src/application/data_maintenance.rs src-tauri/src/services/data_store/backup.rs src-tauri/src/persistence/runtime.rs src-tauri/src/services/data_store/atomic_file.rs
git commit -m "feat: prepare imports behind a durable restart journal"
```

## Task 16：实现启动前激活、确定性恢复和 rollback

**Files:**

- Create: `src-tauri/src/services/portable_migration/recovery.rs`
- Modify: `src-tauri/src/services/portable_migration/activation_journal.rs`
- Modify: `src-tauri/src/services/data_store/atomic_file.rs`
- Modify: `src-tauri/src/services/data_store/decision.rs`
- Modify: `src-tauri/src/services/data_store/types.rs`
- Modify: `src-tauri/src/services/data_store/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/services/secrets/validation.rs`
- Create: `scripts/portable-migration-startup-boundary.test.mjs`
- Modify: `scripts/run-contract-tests.mjs`

- [ ] **Step 1: 写 phase x file-state 故障矩阵**

至少覆盖每个 phase 与以下实际状态：active=old/staged=new；active=new/rollback=old；active new invalid/rollback old；missing/duplicate/hash mismatch/file ID mismatch；journal malformed/version unknown/path escape；ReplaceFileW 成功但返回未知；rollback replace 中断。期望只能是 old active、new active、validated rollback 或 `manual_recovery_required`，禁止自动创建新库。

- [ ] **Step 2: 把 recovery 放到任何 persistence/proxy 之前**

启动顺序：config dir -> installation lease -> strict journal read -> observe actual artifacts -> pure recovery plan -> 按 journal `targetDeviceKeyId` 预加载目标 key -> atomic execute -> 使用同一 key 验证新 active -> persistence composition。journal 有效或损坏时都不得先打开业务 runtime，且不得静默改用另一个 active key ID。

- [ ] **Step 3: 实现 phase 收敛与 receipt**

实际文件证据可补写落后的 phase，但每次都 publish/readback。新库数据库验证失败且 rollback 身份匹配时，用同一 atomic replace port 回滚并验证旧库；无唯一证据进入 recovery UI。`activated_validated` 后非数据库 composition 失败不回滚，下一次重试；runtime Ready 和 receipt durable 后才 completed/cleanup。

- [ ] **Step 4: 接入现有 DataStore recovery UI state**

新增 portable recovery reason，但不允许现有“选择任意候选/创建新库”命令处理 active import journal。只提供受控重试、打开备份目录和导出脱敏诊断；manual state 保存所有候选。

- [ ] **Step 5: 验证**

```powershell
node scripts/portable-migration-startup-boundary.test.mjs
cargo test --manifest-path src-tauri/Cargo.toml portable_migration::recovery -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml services::data_store::decision -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml startup_activation -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
git add -- src-tauri/src/services/portable_migration/recovery.rs src-tauri/src/services/portable_migration/activation_journal.rs src-tauri/src/services/data_store/atomic_file.rs src-tauri/src/services/data_store/decision.rs src-tauri/src/services/data_store/types.rs src-tauri/src/services/data_store/mod.rs src-tauri/src/lib.rs src-tauri/src/services/secrets/validation.rs scripts/portable-migration-startup-boundary.test.mjs scripts/run-contract-tests.mjs
git commit -m "feat: recover portable import activation deterministically"
```

## Task 17：发布 application facade、IPC、权限与 generated bindings

**Files:**

- Modify: `src-tauri/src/application/data_migration/mod.rs`
- Modify: `src-tauri/src/application/data_migration/registry.rs`
- Create: `src-tauri/src/commands/data_migration.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Create: `src-tauri/src/ipc/dto/data_migration.rs`
- Create: `src-tauri/src/ipc/dto/data_migration.typescript.txt`
- Modify: `src-tauri/src/ipc/dto/mod.rs`
- Modify: `src-tauri/src/ipc/dto/fixtures/pilot-serialization.json`
- Modify: `src-tauri/src/ipc/registry.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/permissions/main-window.toml`
- Generated: `src-tauri/generated/command-registry.json`
- Generated: `src/lib/bridge/generated.ts`
- Generated: `src/lib/bridge/contract.ts`
- Modify: generated Tauri schemas only when the repository generator changes them

- [ ] **Step 1: 写 DTO/command RED contract**

覆盖 spec 16 的 11 个命令、deny unknown fields、所有字符串/数组/数字上限、UUIDv7 idempotency/resource ID、正整数字符串 operation ID、`替换当前数据` UTF-8 精确确认、closed mode/resource/progress/error/recovery codes、password fixture canary 和 capability policy-disabled state。commands 只 parse DTO、调用 facade、映射 error；禁止 SQL/keyring/age/file IO。

- [ ] **Step 2: 实现 capability 与 command facade**

security policy 未批准时 capability `enabled=false` 且所有 start 后端拒绝。path chooser 使用 native dialog 后立即生成 token。start 返回 operation/resource kind/id；get result 使用 resource ID；cancel 复用现有 command；migration operation 使用 typed projection。

- [ ] **Step 3: 注册并最小授权**

只将 11 个 migration commands 加入 generated registry/main-window permission；不授予 capture window。mutation metadata 标注 export inspection read-like maintenance、prepare destructive maintenance，确保 Task 6 admission 生效。

- [ ] **Step 4: 生成并验证 bindings**

```powershell
pnpm.cmd run generate:bindings
pnpm.cmd run architecture:commands
pnpm.cmd run architecture:security
cargo test --manifest-path src-tauri/Cargo.toml ipc::dto::data_migration -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
git add -- src-tauri/src/application/data_migration/mod.rs src-tauri/src/application/data_migration/registry.rs src-tauri/src/commands/data_migration.rs src-tauri/src/commands/mod.rs src-tauri/src/ipc/dto/data_migration.rs src-tauri/src/ipc/dto/data_migration.typescript.txt src-tauri/src/ipc/dto/mod.rs src-tauri/src/ipc/dto/fixtures/pilot-serialization.json src-tauri/src/ipc/registry.rs src-tauri/src/lib.rs src-tauri/permissions/main-window.toml src-tauri/generated/command-registry.json src/lib/bridge/generated.ts src/lib/bridge/contract.ts src-tauri/gen/schemas/acl-manifests.json src-tauri/gen/schemas/desktop-schema.json src-tauri/gen/schemas/windows-schema.json
git commit -m "feat: expose portable migration commands safely"
```

生成后先运行 `git diff --name-only`；若某个列出的 generated schema 未变化，不要强行 stage。

## Task 18：实现设置页导出/导入向导与全局维护态

**Files:**

- Create: `src/lib/types/dataMigration.ts`
- Create: `src/lib/api/dataMigration.ts`
- Create: `src/lib/api/dataMigration.test.ts`
- Modify: `src/lib/bridge/BackendClient.ts`
- Modify: `src/lib/bridge/DesktopBackend.ts`
- Modify: `src/lib/bridge/DemoBackend.ts`
- Create: `src/features/settings/data-migration/migrationViewModel.ts`
- Create: `src/features/settings/data-migration/useDataMigrationController.ts`
- Create: `src/features/settings/data-migration/DataMigrationSection.tsx`
- Create: `src/features/settings/data-migration/ExportMigrationDialog.tsx`
- Create: `src/features/settings/data-migration/ImportMigrationDialog.tsx`
- Create: `src/features/settings/data-migration/ImportMigrationSummary.tsx`
- Create: `src/features/settings/data-migration/MigrationMaintenanceScreen.tsx`
- Create: `src/features/settings/data-migration/migrationViewModel.test.ts`
- Create: `src/features/settings/data-migration/DataMigrationSection.test.tsx`
- Create: `src/features/settings/data-migration/MigrationMaintenanceScreen.test.tsx`
- Modify: `src/features/settings/SettingsPage.tsx`
- Modify: `src/features/data-recovery/DataStoreBootstrap.tsx`

- [ ] **Step 1: 写 view-model 与泄漏 RED tests**

覆盖 capability disabled、export 5 steps、import 8 steps、history default false、12 Unicode scalar/1024 UTF-8 byte password、confirmation、replace confirmation、indeterminate KDF、typed progress mapping、page refresh operation restore、inspection expiry、result unknown、restart failure、activated/rolled-back/manual recovery。前端计数必须使用 code point 语义（如 `Array.from`）和 `TextEncoder` 字节数，不能使用 UTF-16 `.length` 代替。断言 passphrase 不进入 query cache/localStorage/toast/error/detail/analytics/screenshot fixture。

- [ ] **Step 2: 实现紧凑本地工具 UI**

在“设置 -> 数据与备份”保留本机备份、同机目录、跨设备搬家三块，不创建网站式页面。使用现有 Dialog/Button/Input 和 lucide icon；密码显示使用 eye icon+tooltip；历史使用 checkbox；模式用 segmented/radio；替换确认用明确文本。所有后端 code 在前端静态 exhaustive switch 映射中文。

- [ ] **Step 3: 实现维护态 outer gate**

`DataStoreBootstrap` 在 App/business queries 之前读取 recovery/maintenance state。`activationPending` 只显示重启，`rolledBack/activated` 显示 receipt 摘要，manual recovery 使用现有恢复骨架。进入 pending 后卸载业务 App，不能靠禁用几个按钮。

- [ ] **Step 4: Demo 与 restart 语义**

DemoBackend 返回明确 unsupported，不模拟迁移成功。prepare 成功调用 `@tauri-apps/plugin-process` relaunch；失败显示“已准备，请手动重启”，不恢复旧 UI。

- [ ] **Step 5: 验证**

```powershell
pnpm.cmd exec vitest run src/features/settings/data-migration src/lib/api/dataMigration.test.ts src/features/data-recovery
pnpm.cmd run test
pnpm.cmd run build
git add -- src/lib/types/dataMigration.ts src/lib/api/dataMigration.ts src/lib/api/dataMigration.test.ts src/lib/bridge/BackendClient.ts src/lib/bridge/DesktopBackend.ts src/lib/bridge/DemoBackend.ts src/features/settings/data-migration/migrationViewModel.ts src/features/settings/data-migration/useDataMigrationController.ts src/features/settings/data-migration/DataMigrationSection.tsx src/features/settings/data-migration/ExportMigrationDialog.tsx src/features/settings/data-migration/ImportMigrationDialog.tsx src/features/settings/data-migration/ImportMigrationSummary.tsx src/features/settings/data-migration/MigrationMaintenanceScreen.tsx src/features/settings/data-migration/migrationViewModel.test.ts src/features/settings/data-migration/DataMigrationSection.test.tsx src/features/settings/data-migration/MigrationMaintenanceScreen.test.tsx src/features/settings/SettingsPage.tsx src/features/data-recovery/DataStoreBootstrap.tsx
git commit -m "feat: add cross-device migration workflows"
```

## Task 19：完成 fixtures、fault injection、恶意输入与清理门禁

**Files:**

- Create: `src-tauri/tests/portable_migration_e2e.rs`
- Create: `src-tauri/tests/portable_migration_faults.rs`
- Create: `src-tauri/tests/portable_migration_malicious.rs`
- Add: encrypted non-secret fixtures under `src-tauri/tests/fixtures/portable-migration/`
- Create: `scripts/portable-migration-fixture-matrix.test.mjs`
- Create: `scripts/portable-migration-redaction.test.mjs`
- Create: `scripts/portable-migration-boundary.test.mjs`
- Create: `scripts/run-portable-migration-performance.ps1`
- Modify: `scripts/run-contract-tests.mjs`
- Modify: `scripts/local-data-artifact-ignore.test.mjs`
- Modify: `.gitignore`

- [ ] **Step 1: 建立 fixture manifest**

每个 fixture 登记 format/profile/schema/features、SHA-256、expected summary/error。至少有当前 valid、支持的旧 reader valid、wrong password、truncated、unknown required feature、too-new schema、malformed SQLite、trigger/view、FK broken、resource overflow。fixture secret 只用 `RPD_TEST_*` canary。

- [ ] **Step 2: 跑三密钥 E2E**

导出 -> 完整 package -> inspection -> target prepare -> simulated restart activation；验证源/transport/目标 key 隔离、history on/off、Local Key reset、session exclusion、quick/FK、recordCounts、source key bytes 不在包和临时目录。

- [ ] **Step 3: 跑完整故障注入矩阵**

覆盖 spec 22.3 的每个边界和 Task 15/16 的原子 edge。每个 case 断言四种允许终局之一，无部分数据库、无 journal 模糊状态、代理未在 recovery 启动。

- [ ] **Step 4: 清理/路径/脱敏检查**

只删除验证过的单 operation directory；journal 引用工件不删；24h orphan partial 可清；junction/reparse 越界拒绝。`.gitignore` 明确覆盖 `.rpd-move`、migration staging、activation journal test output、SQLite/backup/log；artifact gate 注入真实形态文件名证明不会提交。boundary script 禁止 React/command 直接使用 age、SQLite、keyring、shell 或外部进程，禁止 portable reader 执行 migration/DDL/ATTACH。

- [ ] **Step 5: 执行性能资格**

脚本生成无真实 secret 的 1 GiB 数据集，记录 export/import 峰值 RSS、耗时、progress event 数和临时空间；在 4C/8 GiB/SSD 参考环境断言 RSS SHOULD 小于 512 MiB、buffer 不超过 1 MiB、secret 总量增长不导致线性 plaintext memory。未达 SHOULD 必须在 ADR 记录原因、风险和修复日期，不能静默通过。

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/run-portable-migration-performance.ps1
```

- [ ] **Step 6: 验证**

```powershell
node scripts/portable-migration-fixture-matrix.test.mjs
node scripts/portable-migration-redaction.test.mjs
node scripts/portable-migration-boundary.test.mjs
node scripts/local-data-artifact-ignore.test.mjs
cargo test --manifest-path src-tauri/Cargo.toml --test portable_migration_e2e -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test portable_migration_faults -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml --test portable_migration_malicious -- --nocapture
git add -- src-tauri/tests/portable_migration_e2e.rs src-tauri/tests/portable_migration_faults.rs src-tauri/tests/portable_migration_malicious.rs src-tauri/tests/fixtures/portable-migration scripts/portable-migration-fixture-matrix.test.mjs scripts/portable-migration-redaction.test.mjs scripts/portable-migration-boundary.test.mjs scripts/run-portable-migration-performance.ps1 scripts/run-contract-tests.mjs scripts/local-data-artifact-ignore.test.mjs .gitignore
git commit -m "test: qualify portable migration failure boundaries"
```

## Task 20：更新安全政策、用户文档和发布资格

**Files:**

- Modify: `docs/SECURITY_EXPORT_IMPORT.md`
- Modify: `docs/README.md`
- Modify: `docs/PROJECT_PLAN.md`
- Modify: `README.md`
- Create: `docs/release/PORTABLE_MIGRATION_SMOKE_CHECKLIST.md`
- Modify: `docs/superpowers/specs/2026-07-29-portable-migration-crypto-format-adr.md`
- Modify: `docs/superpowers/audits/architecture-scale-dependency-lifecycle.json`
- Modify: `src-tauri/src/application/data_migration/mod.rs`
- Modify: `src-tauri/src/ipc/dto/data_migration.rs`
- Modify: `scripts/verify.ps1`
- Modify: `scripts/release-verification-entrypoint.test.mjs`

- [ ] **Step 1: 安全政策正式评审后才开启 capability**

文档明确区分默认导出、本机备份、同机 relocation 和显式密码保护跨机迁移；说明迁移密码丢失不可恢复、旧本机 backup 仍依赖旧设备 key、旧安全格式 backup 可能含 Local Key 明文、SSD 不承诺物理擦除、JS 字符串无法保证清零。只有评审结论记录后，将后端 feature gate 改为 enabled，并更新 capability DTO fixture；若评审未批准，本 Task 保持 disabled，功能不得发布。

- [ ] **Step 2: 执行两机资格矩阵**

使用 disposable Windows 10/11 VM 或独立用户 profile，覆盖默认/自定义 data dir、非 ASCII/长路径、U 盘/LAN/云盘完整落盘文件、真实 Station Key 请求、网页登录重新授权、CCSwitch 更新新 Local Key，以及 prepare/replace/rollback 各阶段强杀进程。记录版本、fixture ID、结果和脱敏截图；不把真实 package/key 入库。

- [ ] **Step 3: 同一 revision 跑完整自动门禁**

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --lib
pnpm.cmd run generate:bindings
pnpm.cmd run architecture:commands
pnpm.cmd run architecture:security
pnpm.cmd run architecture:dependencies
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check-advisories.ps1
pnpm.cmd run test
pnpm.cmd run build
pnpm.cmd run verify:full
pnpm.cmd run verify:release
pnpm.cmd run tauri:build -- --target x86_64-pc-windows-msvc
```

任何 timeout、ignored fixture、`no tests found`、手工步骤未记录或 advisory gate 缺工具都不算通过。

- [ ] **Step 4: 最终 artifact/canary 审计**

```powershell
git status --short
git diff --check
rg -n "RPD_TEST_|sk-|Bearer |refresh_token|access_token|cookie" docs src src-tauri scripts -g '!src-tauri/tests/fixtures/portable-migration/**'
```

逐项人工确认命中均为字段名、redaction 规则或固定测试 canary，不是真实 secret。

- [ ] **Step 5: 提交文档与 release gate**

```powershell
git add -- docs/SECURITY_EXPORT_IMPORT.md docs/README.md docs/PROJECT_PLAN.md README.md docs/release/PORTABLE_MIGRATION_SMOKE_CHECKLIST.md docs/superpowers/specs/2026-07-29-portable-migration-crypto-format-adr.md docs/superpowers/audits/architecture-scale-dependency-lifecycle.json src-tauri/src/application/data_migration/mod.rs src-tauri/src/ipc/dto/data_migration.rs scripts/verify.ps1 scripts/release-verification-entrypoint.test.mjs
git commit -m "docs: approve portable migration release policy"
```

## 完成门禁

功能只有在以下证据同时存在时才能标记完成：

- security baseline 已升级且 active SQLite 无 Local Key 明文；旧 backup 风险已如实展示。
- current format/profile fixture 已冻结；兼容 registry 和 24 个月 reader owner 已登记。
- 三密钥 E2E、全表 catalog、malicious input、fault matrix、WAL replacement、IPC/UI 泄漏测试全部通过。
- 导出文件经过加密后回读自检和原子 publish；导入只经 trusted target + verified backup + journal 激活。
- startup 在 key 创建、baseline conversion、import recovery 前都持有 installation lease。
- `activation_pending/recovering` 不注册代理/runners，不允许任何 persistence mutation。
- Windows 10/11 手工跨机和真实客户端检查完成，结果无敏感数据。
- 上述 full/release/build 命令在同一 revision 通过。

## 需求追踪矩阵

| Spec 责任 | 实施 Task | 自动证据 | 发布证据 |
| --- | --- | --- | --- |
| 1-7 规范边界、目标/非目标、威胁模型 | 0, 17, 20 | baseline/capability/boundary tests | ADR、安全政策、用户说明 |
| 8.1-8.2 key store 错误与 lease 顺序 | 2 | device key 错误矩阵、startup boundary | 首次启动并发检查 |
| 8.3-8.6 key ID、Local Key、安全基线 | 3-5 | baseline conversion/fault/canary | 旧 backup 风险文案 |
| 8.5/9.1 secret 流式换钥与 zeroize | 3-4 | rekey 三密钥/compile-time tests | 依赖/内存剩余风险说明 |
| 9.2-10 age、framing、Manifest、limits | 0, 7, 11 | format/KDF/self-test fixtures | ADR 与 dependency review |
| 11 catalog、历史、occupancy | 8 | live schema exhaustive gate | schema 变更 release gate |
| 12 导出 snapshot 到原子发布 | 10-11 | concurrent snapshot、readback、publish faults | 真实 1 GiB 性能检查 |
| 13.2-13.4 不可信包 inspection | 12-13 | malicious package/TOCTOU/resource tests | 云盘/U盘导入 |
| 13.5 trusted target 与 target key | 9, 14 | writer dependency/FK/three-key tests | 目标机真实 Key 请求 |
| 13.6 backup | 15 | 两模式 backup failure tests | backup 可见且未静默删除 |
| 13.7 activation/recovery/WAL | 1, 15-16 | phase x file-state fault matrix | 强杀/断电资格测试 |
| 14 maintenance/cancel | 6, 12, 15 | admission/drain/commit barrier tests | pending UI 无业务写 |
| 15 模块边界 | 1-18 | architecture commands/security gates | code review file-size check |
| 16 IPC/idempotency/path token | 12, 17 | generated fixture/token/registry tests | main-window 最小权限 |
| 17 UI | 18 | view-model/component/no-leak tests | Windows 桌面人工流程 |
| 18 error/recovery codes | 13, 16-18 | exhaustive Rust/TS mapping | 脱敏错误截图 |
| 19 staging/cleanup/path | 1, 11-13, 19 | reparse/TTL/orphan cleanup tests | artifact ignore audit |
| 20 compatibility | 0, 9, 19-20 | old/new fixture registry | 24 个月 owner/release note |
| 21 observability | 12, 17, 19 | allowlist DTO/redaction tests | 本地诊断审计 |
| 22 全测试矩阵 | 2-19 | unit/integration/fault/malicious/UI | 跨机 qualification |
| 23 性能 | 7, 10-14, 20 | bounded buffers/RSS benchmark | 4C/8GiB/SSD 记录 |
| 24 分阶段发布 | 0-20 | task dependency gates | A-E 阶段门槛逐项签署 |
| 25 完成标准 | 20 | full/release/build 同 revision | smoke checklist 签署 |
| 26 后续扩展不变量 | 0, 7, 9, 20 | compatibility/architecture tests | ADR 保留 device-key/AEAD/staging/recovery 边界 |

## 计划复核清单

- [x] 所有 spec `MUST/MUST NOT` 均能映射到上表 task 和测试，无“实现时再决定”的安全边界。
- [x] 所有新增命令、DTO、progress/error/recovery code 在 Rust、TypeScript、fixture、权限和 UI 中有同一 owner。
- [x] 当前 29 表 + `app_secret_bindings` 全部有 policy、field rule、occupancy 和 copy stage。
- [x] 无 task 要求在不可信 package DB 上执行 migration/DDL/ATTACH。
- [x] 无 task 在 runtime 打开时替换 SQLite；WAL sidecar 在 main file replacement 前已证明清空。
- [x] 无 task 将 password、transport key 或 device key 写入 Manifest、journal、result、progress、日志或磁盘。
- [x] 所有不可逆文件动作都由原子 port 和 fault evidence 覆盖。
- [x] security policy 更新前，UI 与后端 capability 双重禁用。
- [x] 所有 commit 示例使用精确路径；无 `git add .` / `git add -A`。
- [x] 最终验证同时包含 Cargo、bindings、architecture、advisory、Vitest、Vite、full/release 和 Tauri build。
