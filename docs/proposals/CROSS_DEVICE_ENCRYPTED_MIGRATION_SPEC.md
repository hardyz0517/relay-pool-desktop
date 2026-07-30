# Relay Pool Desktop 跨设备加密迁移实现规格

状态：Reviewed Draft

最后审查：2026-07-29

目标版本：待排期

适用范围：Windows 桌面版、Persistence V2 及后续兼容版本

关联文档：

- [`../PROJECT_PLAN.md`](../PROJECT_PLAN.md)
- [`../SECURITY_EXPORT_IMPORT.md`](../SECURITY_EXPORT_IMPORT.md)

历史背景（不作为当前实现依据）：

- [`../archive/early-phase-plans/PHASE_8_SECURITY_CREDENTIAL_GOVERNANCE_PLAN.md`](../archive/early-phase-plans/PHASE_8_SECURITY_CREDENTIAL_GOVERNANCE_PLAN.md)

## 1. 规范约定

本文使用以下约束级别：

- `MUST`：实现、测试和发布必须满足。
- `MUST NOT`：明确禁止。
- `SHOULD`：默认应满足；偏离时必须在 ADR 中记录理由和风险。
- `MAY`：允许实现，但不构成首版交付要求。

本文定义的是产品级“跨设备搬家”，不是简单的文件复制。迁移结果必须能在另一台 Windows 电脑、另一个 Windows 用户和不同的系统凭据数据密钥下恢复。

当前 `SECURITY_EXPORT_IMPORT.md` 仍禁止默认导出携带 secret 或密文。本规格定义的是独立、显式确认、密码保护的“跨设备迁移”能力，不改变“默认导出”的含义。功能入口在安全政策完成评审和更新前 MUST 保持禁用；实现规格不能自行覆盖当前安全政策。

## 2. 背景与问题

当前敏感凭据由 AES-256-GCM 加密后保存在 SQLite，数据密钥保存在 Windows Credential Manager。现有 SQLite 备份可以恢复业务数据，但其中的凭据密文仍依赖源电脑的系统凭据项，因此不具备跨设备可移植性。

同时存在以下前置缺口：

1. 系统凭据读取的任意错误目前都可能被当作“凭据不存在”，进而创建新数据密钥。
2. 本地代理 `local_key` 仍存储在 `settings.value` 明文列中。
3. 密文记录没有完整的 `key_id` 与 `encryption_version`，不利于轮换和诊断。
4. 现有备份与数据目录搬迁没有表达“跨设备重新封装凭据”的语义。

跨设备迁移功能不得绕过这些缺口直接交付。

## 3. 目标

本功能必须达到以下结果：

1. 用户可从源电脑导出一个带密码保护的 `.rpd-move` 文件。
2. 迁移包可在目标电脑导入，不依赖源电脑 Windows Credential Manager。
3. 源电脑的设备数据密钥永不写入迁移包。
4. 目标电脑导入后，所有长期凭据使用目标电脑的数据密钥重新加密。
5. 导入失败、取消、进程崩溃、断电或磁盘空间不足时，不破坏目标电脑当前数据。
6. 导出与导入支持明确的格式版本、数据库版本和兼容性检查。
7. 新增业务表或敏感字段时，编译期或测试期必须迫使维护者声明其迁移策略。
8. UI、日志、操作进度和诊断文件不得暴露密码、密钥、Cookie、Token 或凭据明文。

## 4. 非目标

首版明确不实现：

- 多设备实时同步。
- 云端备份。
- 两个已有数据库的自动合并。
- 只导入单个站点或单把 Key。
- 无密码的便携凭据导出。
- 导出 Windows Credential Manager 中的设备数据密钥。
- 在两台电脑之间保留相同的本地代理访问 Key。
- 将迁移包作为长期增量备份格式。
- 跨平台系统凭据实现；格式需保持可扩展，但首版只发布 Windows。

## 5. 用户场景与产品边界

### 5.1 本机备份

现有“本机备份”继续保留：

- 用途：本机误操作恢复、升级前备份。
- 内容：一致性 SQLite 备份，包括使用当前设备密钥加密的凭据密文。
- 可移植性：不保证跨设备恢复。
- UI 文案必须明确显示“依赖本机 Windows 凭据”。

### 5.2 同机数据目录搬迁

现有数据目录 relocation 继续负责同一设备内的目录切换：

- 不更换数据密钥。
- 不生成迁移密码。
- 继续使用已有 relocation intent 和重启恢复机制。

### 5.3 跨设备搬家

新增“跨设备搬家”：

- 导出时要求用户设置迁移密码。
- 源数据中的可迁移凭据改用一次性传输数据密钥加密。
- 整个迁移载荷再使用标准密码加密容器保护。
- 导入后凭据改用目标设备数据密钥加密。
- 首版只支持导入到空数据集或替换当前数据集。

## 6. 威胁模型

### 6.1 需要防护

实现必须考虑：

- 迁移文件丢失或被第三方复制。
- 迁移文件被修改、截断、拼接或替换。
- 用户选择了伪造、畸形或超大迁移包。
- 错误密码的离线猜测。
- 导出或导入中途崩溃、断电或被强制结束。
- 磁盘空间不足和目标路径不可写。
- 源数据库在导出过程中仍有后台采集、代理日志或配置写入。
- 目标电脑已有可用数据，导入失败造成覆盖或部分更新。
- 日志、错误文本、操作进度或临时文件泄漏敏感信息。
- 新增数据表后未更新导出策略，造成漏数据或意外导出。

### 6.2 不承诺防护

以下情况不属于本功能的完整防护范围，但文档必须明确：

- 源电脑或目标电脑已被同权限恶意程序完全控制。
- 用户输入迁移密码时被键盘记录。
- 应用运行时被调试器或内存读取工具提取明文。
- 用户选择弱密码后遭受离线字典攻击。
- 操作系统、SQLite 或密码学依赖本身存在未修复漏洞。

## 7. 总体架构

迁移过程采用三层密钥边界：

1. `Source Device Data Key`：源电脑设备数据密钥，只用于读取源凭据，永不导出。
2. `Transport Data Key`：每次导出随机生成的一次性 256 位密钥，用于迁移 SQLite 内敏感字段的 AES-256-GCM 加密。
3. `Target Device Data Key`：目标电脑设备数据密钥，导入时用于重新加密所有长期凭据。

迁移密码只用于标准外层容器，不直接作为 SQLite 字段加密密钥。

```text
source SQLite
  -> consistent snapshot
  -> apply export policy
  -> decrypt each secret with source device key
  -> encrypt each included secret with transport data key
  -> rebuild compact portable SQLite
  -> write manifest + raw transport data key + portable SQLite into age stream
  -> .rpd-move

.rpd-move
  -> authenticate and decrypt age stream with passphrase
  -> validate manifest and portable SQLite in staging
  -> decrypt each secret with transport data key
  -> encrypt each secret with target device key
  -> rebuild compact target SQLite
  -> validate
  -> restart-bound journaled activation
```

## 8. 必须先完成的安全前置

跨设备导出入口在以下事项全部完成前必须保持不可用。

### 8.1 系统凭据错误分类

`DeviceKeyStore` 必须返回封闭错误类型：

```rust
enum DataKeyLoadError {
    NotFound,
    Unavailable,
    PermissionDenied,
    Corrupt,
    Unsupported,
    Internal,
}

enum DataKeyStoreError {
    AlreadyExists,
    Unavailable,
    PermissionDenied,
    VerificationFailed,
    Internal,
}
```

约束：

- `NotFound` 只是进入创建流程的必要条件，不是充分条件。只有启动决策已证明是全新安装、没有 active/候选数据库和待恢复 journal，或正在执行显式 key rotation 的 pending-key 步骤时才允许创建。
- 已有数据库、backup、upgrade/import journal 或 secret key ID 需要该设备 key 时，即使 credential store 返回 `NotFound` 也必须进入恢复并 fail closed；不得生成替代 key、不得改写 active pointer。
- `Unavailable`、`PermissionDenied`、`Corrupt`、`Unsupported` 和 `Internal` 必须 fail closed。
- 读取失败不得调用 `set_password`，不得生成替代密钥，不得启动代理或后台采集。
- `create_pending` 遇到同 ID entry 必须返回 `AlreadyExists`，不能覆盖；`commit_active` 写入后必须回读 active pointer 并验证精确 ID，失败返回 `VerificationFailed`。
- 错误信息只能包含错误类别和可执行建议，不得包含凭据内容。
- 测试必须覆盖每一种系统凭据错误，证明不会覆盖已有密钥。

设备密钥接口必须以 key ID 寻址，不能继续假定进程中永远只有一把隐式密钥：

```rust
trait DeviceKeyStore {
    fn active_key_id(&self) -> Result<DeviceKeyId, DataKeyLoadError>;
    fn load(&self, id: &DeviceKeyId) -> Result<SecretKeyMaterial, DataKeyLoadError>;
    fn create_pending(&self, id: &DeviceKeyId) -> Result<SecretKeyMaterial, DataKeyStoreError>;
    fn commit_active(&self, id: &DeviceKeyId) -> Result<(), DataKeyStoreError>;
}
```

- `SecretKeyMaterial` 必须由 `Zeroizing<[u8; 32]>` 持有。
- `SecretKeyMaterial`、迁移密码和 transport key MUST NOT 实现 `Copy`、`Clone`、`Serialize` 或泄漏内容的 `Debug`。
- 业务模块不得获得可长期保存的裸 `[u8; 32]`；解密、换钥和验证通过受限闭包或 secret service 完成。
- 删除旧设备密钥不属于普通轮换事务；只有数据库已验证使用新 key ID、verified backup 保留策略已满足且用户明确确认后才 MAY 删除。
- Windows Credential Manager entry 的命名规则和 active key pointer 必须版本化并有 legacy `local-data-key-v1` 兼容适配器。

### 8.2 单实例与密钥创建顺序

- 安装级 lease 必须在首次创建系统数据密钥之前获得。
- 当前启动顺序需要调整为：解析配置目录 -> 获取 installation lease -> 只读检查 data-store/journal 并判定是否 first-run -> 按判定加载或创建数据密钥 -> 执行恢复或打开数据存储。只读检查不得运行 migration 或创建数据库。
- 两个进程并发首次启动时只能有一个进程创建密钥。
- 创建后必须立即回读并以常量时间比较确认写入值一致。
- first-run 不得直接覆盖 active pointer：应用先生成 key ID 并在默认配置目录耐久化不含 key material 的 bootstrap journal，再按该 ID 执行 `create_pending` -> 用 pending key ID 初始化并验证新数据库及 Local Key -> `commit_active` -> 提交 data-dir config/installation marker -> 清理 journal。任一步失败不得启动业务 runtime；已创建的 pending key 或数据库候选必须由 journal 关联并在下次启动幂等恢复，不能静默另建第二把 key/第二个空库。`create_pending(id)` 必须拒绝覆盖同名 entry；恢复器只能 load 已存在的该 ID 或完成尚未执行的创建步骤。

### 8.3 密钥标识与密文版本

`secrets` 表必须增加：

```sql
key_id TEXT NOT NULL,
encryption_version INTEGER NOT NULL,
```

约束：

- `key_id` 标识解密所需数据密钥，不包含密钥本身。
- `encryption_version = 1` 固定表示 AES-256-GCM、32 字节密钥、12 字节随机 nonce 和 AAD v1。
- 未知 `encryption_version` 必须拒绝解密，不得猜测兼容。
- 新写入不得继续产生无 `key_id` 的密文。
- 旧行升级必须是可恢复、可重复执行的数据迁移。
- SQLite 不能直接给已有行增加无默认值的 `NOT NULL` 列；升级必须先建立允许回填的中间 schema，逐条用当前设备密钥验证旧密文后写入 `key_id` 和版本，再通过新表重建收紧 `NOT NULL` 约束。
- 该升级属于需要设备密钥的应用级数据迁移，不得只放入无密钥上下文的静态 SQL migration。
- 回填完成前不得提高 schema compatibility 版本；失败时保留原库和可恢复 journal。

### 8.4 本地代理访问 Key 加密

- `settings.value` 不得继续保存 `local_key` 明文。
- 新增通用应用密钥绑定，例如 `app_secret_bindings(name, secret_id)`。
- 绑定名固定为 `local_proxy_access_key`。
- 数据升级必须先加密并验证，再清除旧 `settings.local_key`。
- 清除旧值后必须通过重建或 `VACUUM` 确保发布版数据库文件不保留该明文的可搜索副本。
- 应用必须在本地 Key 数据迁移完成前拒绝启动代理。
- 跨设备导出默认不包含该 Key，目标电脑导入时生成新值。

### 8.5 数据密钥轮换能力

必须先实现可复用的 `SecretRekeyService`：

```rust
trait SecretRekeyService {
    fn rekey_database(
        source: &Path,
        destination: &Path,
        from: &dyn SecretKeyResolver,
        to: &dyn SecretKeyResolver,
        policy: &SecretMigrationPolicy,
    ) -> Result<RekeyReport, RekeyError>;
}
```

要求：

- 逐行处理凭据，不建立全量明文列表。
- 明文缓冲使用 `Zeroizing<Vec<u8>>`。
- 每条凭据使用新随机 nonce。
- AAD 必须由 `scope:owner_id:kind` 和固定版本规则重新计算。
- 任意一条解密失败时整个输出不可激活。
- 输入数据库永远只读；输出必须写入新文件。
- 服务不得依赖 Tauri command 或 React DTO，可被密钥轮换、导出和导入共同复用。
- 换钥输出必须记录 from/to key ID 和行数，但不得记录密钥材料、密文、nonce 或 masked value。
- 设备密钥轮换遵循 pending key -> 新数据库验证 -> active pointer commit -> 延迟退休旧 key 的顺序；不得先覆盖 active credential entry。

### 8.6 安全基线数据库转换

`key_id` / `encryption_version` 收紧、Local Key 加密和所有 legacy 凭据明文残留清理必须作为一次可恢复的数据库转换交付，不得拆成多个普通启动写入：

- 实现开始时从 migration registry 选择下一个未占用 schema version，不在本文硬编码迁移文件编号。
- 转换复用现有 verified backup、generation upgrade journal、候选验证和 recovery 入口；若现有框架不能表达密钥参与的数据转换，先扩展框架再迁移。
- 源数据库只读，目标数据库用当前受信任 schema 新建并导入；成功验证前不替换 active。
- `stations.api_key`、`station_keys.api_key`、`station_credentials.login_password` 中的有效 legacy 值必须先进入对应 secret 并验证引用，再清空；冲突的明文与 secret 不得猜测覆盖，必须进入恢复/修复。
- compatibility 版本只在目标数据库完整写入、全部 secret 可解密、Local Key 新绑定可用且上述 legacy 明文字段全部为空后提高。
- 旧 binary 必须依据 schema compatibility fail closed，不得以 legacy 写路径向新库写入缺少 key ID 的 secret。
- 转换前 verified backup 可能包含旧明文 Local Key 或 legacy API Key/登录密码。它必须以受控 ACL 保留并在 UI 标记为“旧安全格式、仅本机恢复”；不得把“active 数据库已无明文”误表述为历史备份也已清除。
- 安全政策明确保留前，应用不得静默删除旧格式 verified backup；后续清理必须是独立、显式确认功能。

## 9. 密码学规范

### 9.1 数据库字段加密

首版继续使用：

- 算法：AES-256-GCM。
- 数据密钥：CSPRNG 生成的 32 字节。
- nonce：每条写入由 OS CSPRNG 生成的 12 字节，不得复用。
- AAD v1：UTF-8 编码的 `scope:owner_id:kind`。
- 应用拥有的密钥材料和明文缓冲必须在生命周期结束时以 zeroize 清零。

不得将迁移密码直接做 SHA-256 后作为 AES 密钥。

### 9.2 外层迁移包加密

`.rpd-move` v1 必须使用 Rust `age` 实现的 passphrase 加密模式：

- 使用标准 age 二进制格式。
- 使用 age 规范定义的 scrypt passphrase recipient。
- 内容加密和完整性认证由 age 格式提供。
- 依赖必须锁定精确可审计版本，并纳入依赖安全扫描。
- 不得自行实现 age、scrypt、ChaCha20-Poly1305 或流分块协议。

依赖准入门槛：

- 必须证明所选 `age` 版本对恶意 KDF work factor 有明确上限，或允许调用方在执行 KDF 前施加上限。
- 如果无法施加上限，该版本不得用于发布，避免伪造文件触发不受控 CPU 或内存消耗。
- 错误密码、认证失败和截断统一返回 `package_authentication_failed`，不暴露可用于探测内部结构的细节。

### 9.3 密码要求

- UI 最少接受 12 个 Unicode scalar value，UTF-8 编码最大 1024 字节。
- UI 必须要求输入两次并在前端和后端分别验证一致性。
- 后端是最终校验边界，不能依赖前端验证。
- 不设置武断的复杂度组合规则；应显示长度和弱密码提示。
- 密码按用户输入的 UTF-8 字节原样使用，不做 trim、大小写转换或 Unicode normalization；前后端必须使用同一计数和比较规则。
- 前后空白 MAY 有效，但 UI 必须在导出确认前明确警告，避免不可见字符导致无法恢复。
- 密码不得写入应用状态持久化、日志、崩溃报告或剪贴板。
- Tauri command 参数不可避免地会短暂持有字符串；命令层必须立即转换为 `Zeroizing<String>`，不得派生 `Debug` 或记录参数。
- 导出完成或失败后必须清空表单字段。
- JavaScript 字符串和第三方密码学库内部缓冲无法承诺物理清零；实现必须最小化它们的生命周期、禁止持久化/缓存，并在安全说明中如实记录该剩余风险。

## 10. `.rpd-move` v1 文件格式

文件外层是标准 age 加密流。age 解密后的明文载荷使用固定顺序，不使用 ZIP、TAR 或任意路径归档，以消除路径穿越、重复文件名和解压炸弹风险。

### 10.1 解密后载荷布局

```text
offset  size       field
0       8          magic = "RPDMOVE1"
8       4          manifest_length, unsigned big-endian u32
12      N          manifest JSON, UTF-8
12+N    32         raw transport data key
44+N    8          sqlite_length, unsigned big-endian u64
52+N    M          portable SQLite bytes
52+N+M  32         SHA-256(portable SQLite bytes)
```

约束：

- 不允许尾随字节。
- `manifest_length` 首版最大 256 KiB。
- `sqlite_length` 首版默认最大 2 GiB；可由内部配置降低，但不得由迁移包提高。
- 解密字节总数必须受 `manifest_length + 32 + sqlite_length + 固定开销` 限制。
- SHA-256 只用于传输诊断和确定性校验，不代替 age 完整性认证。
- 必须消费 age 流到 EOF 并验证最终认证状态后，才可信任载荷。
- 32 字节 transport key 必须直接读入专用 `Zeroizing<[u8; 32]>`，不得经过 JSON、Base64、普通 `String`、`Debug` 或通用 Map。

### 10.2 Manifest

Manifest 使用 UTF-8 JSON，字段使用 camelCase。v1 顶层 schema 是封闭的：未知顶层字段不得忽略，只能放入受限的 `extensions`。以下字段为 v1 必需：

```json
{
  "format": "relay-pool-portable-migration",
  "formatVersion": 1,
  "exportId": "UUIDv7",
  "createdAt": "RFC3339 UTC",
  "sourceAppVersion": "0.0.0",
  "sourcePlatform": "windows",
  "databaseGeneration": 2,
  "databaseSchemaVersion": 10,
  "portableSchemaProfile": "encrypted-secrets-v1",
  "minimumImporterVersion": "0.0.0",
  "transportKeyId": "transport:<UUIDv7>",
  "encryptionVersion": 1,
  "exportPolicyVersion": 1,
  "requiredFeatures": [],
  "extensions": {},
  "includedCategories": [],
  "excludedCategories": [],
  "recordCounts": {},
  "sqliteSizeBytes": 0,
  "sqliteSha256": "base64-32-bytes"
}
```

约束：

- Manifest 位于 age 加密边界内，不在文件外暴露站点数量、版本或导出时间。
- transport key 不属于 Manifest，只允许存在于 age 加密载荷的固定 32 字节字段和进程零化内存中。
- `transportKeyId` 必须与便携 SQLite 中每条 `secrets.key_id` 一致。
- `recordCounts` 仅用于导入预览和验证，不作为数据库真实性来源。
- Manifest 顶层 schema 必须封闭并拒绝重复 JSON key、未知顶层字段、错误类型、未知 format、超长字符串和不合法版本。
- 可选扩展只能放入 `extensions`；影响安全、解密、数据语义或兼容性的扩展名必须同时列入 `requiredFeatures`。
- Importer 遇到未知 `requiredFeatures` 必须拒绝；未知但非 required 的 extension MAY 忽略。
- `extensions` 总序列化大小不得超过 64 KiB，单个扩展不得覆盖顶层字段语义。
- `exportId` 和 `transportKeyId` 中的 UUIDv7 必须使用小写、带连字符的规范文本；`createdAt` 必须是带 `Z` 的 UTC RFC3339，禁止本地时区和模糊精度。
- `sourceAppVersion` 与 `minimumImporterVersion` 必须是规范 SemVer，不接受前导 `v`、范围表达式或任意自由文本。
- `sqliteSizeBytes` 必须等于实际 SQLite 字节数；`sqliteSha256` 使用 RFC 4648 标准 Base64（带 padding）编码恰好 32 字节摘要。
- `encryptionVersion` 必须等于便携库中全部已包含 secret 的 `encryption_version`；v1 不允许一个包混用多个 secret 加密版本。
- `includedCategories` 与 `excludedCategories` 使用闭合代码：`core_data`、`history`、`session_credentials`、`local_proxy_access_key`、`device_runtime_state`、`provider_drafts`。`core_data` 必须 included；`history` 恰好位于 included 或 excluded 之一；其余四项必须 excluded。数组不得重复、不得同时出现同一 code。
- `recordCounts` 的 key 必须恰好是所选 `PortableSchemaReader` 结构指纹中声明的全部应用表名（包括计数为零的 Reset / Exclude 表），value 是不超过 catalog 上限的非负整数；不得以类别名、未知表名或省略零值代替表级计数。
- `requiredFeatures` 与 `extensions` 的 key 使用同一 feature-name 语法：ASCII 小写字母开头，后续只允许小写字母、数字、`.`、`-`，最长 128 bytes；两者各最多 64 项，且 `requiredFeatures` 不得重复。
- `sqliteSha256` 和尾部 SHA-256 必须一致并匹配实际数据库。
- 示例中的 `databaseSchemaVersion` 仅表示整数类型。v1 导出必须要求数据库已经完成安全前置升级，并匹配 binary 常量 `PORTABLE_MIGRATION_MIN_SCHEMA_PROFILE = "encrypted-secrets-v1"`；不得以当前 pre-baseline schema v9 直接导出。

### 10.3 v1 资源上限

所有上限必须集中在 `PortableMigrationLimitsV1`，由导出、导入、capability DTO 和测试 fixture 共用；禁止各模块复制数字。

| 项目 | v1 上限 |
| --- | --- |
| age 加密文件 | 2.25 GiB |
| portable SQLite | 2 GiB |
| Manifest | 256 KiB |
| `extensions` | 64 KiB |
| passphrase UTF-8 | 1024 bytes |
| `requiredFeatures` | 64 项，每项 128 bytes |
| `recordCounts` | 128 项 |
| 单个普通 TEXT/BLOB 字段 | 1 MiB |
| 单个显式允许的大型 redacted JSON 字段 | 8 MiB |
| JSON 嵌套深度 | 64 |
| 单表行数 | 5,000,000 |
| 全部用户表总行数 | 10,000,000 |
| export operation deadline | 2 小时 |
| inspection operation deadline | 30 分钟 |
| prepare operation deadline | 2 小时 |
| 后台任务/代理/write drain 单项 deadline | 30 秒 |

- 导出源超过任一上限时必须在生成包前失败，不能生成当前 importer 无法接受的 v1 包。
- 每个字段只能使用 catalog 声明的普通或大型限制；未知 BLOB/TEXT 一律按普通限制。
- 乘法、加法和长度换算使用 checked arithmetic；溢出等同超限。
- 上限只能由新 format version 或明确向后兼容的 importer capability 提升，不能由 Manifest 请求提升。
- KDF work factor 上限由通过依赖准入的 age adapter 常量定义，并加入同一 capability；恶意文件不能提高该上限。
- portable operation 必须显式使用上表 deadline，不得继承当前全局 30 秒默认值；deadline 到期按协作取消处理，进入 commit barrier 后按 `result_unknown` / journal 真相处理，不能强行删除可能已提交的工件。

## 11. 数据分类与迁移策略

必须新增集中式 `MigrationDataCatalog`。它是所有表和敏感字段的唯一迁移策略来源，禁止在导出函数中散落表名判断。

每个表必须声明一种策略：

```rust
enum TableMigrationPolicy {
    Include,
    IncludeWithTransform,
    OptionalHistory,
    Reset,
    Exclude,
    InternalRebuild,
}
```

每个敏感字段必须声明：

```rust
enum SensitiveFieldPolicy {
    EncryptedSecretReference,
    RedactedMetadata,
    ResetOnExport,
    ForbiddenPlaintext,
}
```

### 11.1 默认包含

- `stations`
- `station_keys`
- Station Key 的 API Key 凭据
- 用户明确保存的站点登录密码
- 通用登录资料及其用户明确保存的密码
- `station_key_capabilities`
- `model_aliases`
- `pricing_rules`
- `model_base_prices`
- `station_group_bindings`
- 路由、采集频率、监控模板和用户设置
- `remote_station_keys` 中不含完整远端 Key 的脱敏元数据

### 11.2 默认重置或排除

- `access_token`、`refresh_token`、Cookie 和登录 Session：删除对应 secret，清空引用并将状态重置为需要重新授权。
- 本地代理访问 Key：不导出，目标机生成新值。
- `local_proxy_start_on_launch`：重置为 `false`。
- 活跃后台任务、操作注册表和运行中状态：不导出。
- `collector_task_state`：重置 next run 和运行中状态，导入后由调度器重建。
- `collector_model_facts`：默认重置并由目标机重新采集，避免保留指向已排除 collector run 的非空外键。
- `provider_drafts` 和 `provider_draft_previews`：首版排除；这些表是临时工作区且 JSON 可能含未提交敏感输入。
- 数据目录、待搬迁目录和安装 lease：不进入迁移 SQLite 或 Manifest。
- 更新器状态、窗口状态和设备路径：不导出。

### 11.3 可选历史数据

导出 UI 可提供一个“包含历史记录”复选框，默认关闭。该类别包括：

- `request_logs` 与 `request_attempts`
- `collector_runs` 与 `collector_snapshots`
- `group_rate_records`
- `channel_monitor_runs`
- `balance_snapshots`
- `change_events`

即使用户选择包含，也必须经过现有集中脱敏器重新处理。Prompt、Response、Authorization、Cookie、Token 和完整 Key 永远不得进入历史导出。

### 11.4 Schema 覆盖门禁

必须提供自动测试：

1. 从已应用迁移后的 `sqlite_schema` 枚举全部用户表。
2. 与 `MigrationDataCatalog` 声明比较。
3. 任意新表缺少策略时测试失败。
4. 扫描已知敏感列名和 JSON 字段，缺少字段策略时测试失败。
5. 对默认导出执行 canary 扫描，迁移包解密后的 SQLite 不得包含排除项明文。

### 11.5 当前 Schema v9 表策略基线

以下矩阵必须作为首版 `MigrationDataCatalog` 的行为基线。表名变化时必须同时更新 schema migration、catalog 和兼容 fixture。

| 表 | 策略 | 必需转换 |
| --- | --- | --- |
| `persistence_schema_compatibility` | `InternalRebuild` | 由目标 binary 的正式 migration 维护，不信任包内自报版本 |
| `persistence_runtime_health` | `InternalRebuild` | 在目标库重建为干净、非运行中状态 |
| `settings` | `IncludeWithTransform` | 只包含设置 key allowlist；重置 Local Key、启动状态、路径和设备相关值 |
| `secrets` | `IncludeWithTransform` | 依据 secret kind 包含、排除并换成 transport key |
| `stations` | `IncludeWithTransform` | 保留资产配置；明文兼容 API Key 列必须为空；健康与临时登录状态重置 |
| `station_keys` | `IncludeWithTransform` | 保留 Key 元数据和 secret 引用；明文兼容 API Key 列必须为空；临时健康状态重置 |
| `station_endpoint_health` | `Reset` | 为现有 Station 重建 unchecked 基线 |
| `station_key_health` | `Reset` | 为现有 Station Key 重建 unchecked 基线 |
| `station_credentials` | `IncludeWithTransform` | 保留账号名和用户选择保存的密码；清空 session、token、cookie 及其状态 |
| `remote_station_keys` | `IncludeWithTransform` | 仅保留脱敏元数据和绑定；禁止完整远端 Key |
| `station_key_capabilities` | `Include` | 完整保留用户能力和模型范围配置 |
| `model_aliases` | `Include` | 完整保留 |
| `pricing_rules` | `Include` | 完整保留 |
| `balance_snapshots` | `OptionalHistory` | 仅在包含历史时保留 |
| `request_logs` | `OptionalHistory` | 再次脱敏错误、经济上下文和候选详情 |
| `request_attempts` | `OptionalHistory` | 仅随父 request log 保留；再次脱敏 detail |
| `collector_runs` | `OptionalHistory` | 运行中记录转换为中断终态；错误文本再次脱敏 |
| `collector_snapshots` | `OptionalHistory` | JSON 再次递归脱敏并施加大小上限 |
| `station_group_bindings` | `IncludeWithTransform` | 保留配置事实，清理 run 引用和临时检查状态 |
| `group_rate_records` | `OptionalHistory` | 仅在包含历史时保留已脱敏的倍率历史 |
| `collector_model_facts` | `Reset` | last_seen_run_id 为非空外键；默认由目标机重新采集 |
| `collector_task_state` | `Reset` | 由目标调度器重建 |
| `change_events` | `OptionalHistory` | JSON 与 message 再次脱敏；清理被排除的日志引用 |
| `model_base_prices` | `Include` | 完整保留 |
| `channel_monitor_request_templates` | `IncludeWithTransform` | 请求体通过禁止敏感字段校验后保留 |
| `channel_monitors` | `IncludeWithTransform` | 保留配置，清空 last/next run 和临时错误状态 |
| `channel_monitor_runs` | `OptionalHistory` | 错误文本再次脱敏 |
| `provider_drafts` | `Exclude` | 首版不导出未提交工作区 |
| `provider_draft_previews` | `Exclude` | 随 draft 排除 |
| `app_secret_bindings`（前置新增） | `IncludeWithTransform` | 不包含 Local Key 绑定；其他绑定必须逐项声明策略 |

`settings` 必须采用 key allowlist，不能使用“复制所有未知设置”的策略。当前已知设置至少分为：

- 可携带：路由策略、倍率限制、余额阈值、采集/价格刷新周期、超时、并发、监控和用户 UI 偏好。
- 携带但转换：`common_login_profiles_json`，只保留 profile 元数据和有效 secret 引用。
- 重置：`local_key`、`local_proxy_start_on_launch`、数据目录、待搬迁目录、运行时和设备路径。
- 未知 setting key：导出必须失败并要求 catalog 更新，不能静默包含或丢弃。

`secrets` 必须采用 `(scope, kind)` allowlist：

| scope | kind | 策略 |
| --- | --- | --- |
| `station_key` | `api_key` | 必须包含并换成 transport key |
| `station_credentials` | `login_password` | 仅当用户启用了保存密码时包含并换钥，否则删除引用和 secret |
| `station_credentials` | `access_token` | 删除并重置授权状态 |
| `station_credentials` | `refresh_token` | 删除并重置授权状态 |
| `station_credentials` | `cookie` | 删除并重置授权状态 |
| `common_login_profile` | `password` | 必须包含并换成 transport key |
| `application` | `local_proxy_access_key` | 不导出；目标机生成新值 |

发现未知 scope、未知 kind、孤立 secret、缺失 owner 或不匹配引用时，导出必须失败并要求先修复数据，不能静默复制或删除。已经发布过的 legacy scope 必须先通过正式升级归一化到上述 allowlist。

## 12. 导出流程

### 12.1 前置检查

导出命令必须：

1. 验证应用处于 writable 正常模式，不处于 recovery、数据库升级或 relocation 状态。
2. 获取跨设备迁移独占 operation lease；同一时间只允许一个导出或导入。
3. 验证系统数据密钥可用，并验证当前所有 secret 可解密。
4. 验证目标文件是用户通过保存对话框选择的普通文件路径。
5. 目标扩展名固定为 `.rpd-move`。
6. 禁止写入当前数据目录、备份目录或应用安装目录中的受管理数据库文件名。
7. 根据源数据库大小和策略估算空间，要求临时目录与目标卷各自具备足够空间。

空间预检公式首版采用保守上限：

```text
temporary_required = max(source_size * 3 + 256 MiB, 512 MiB)
destination_required = max(source_size * 2 + 64 MiB, 128 MiB)
```

预检不能代替实际写入错误处理。

### 12.2 一致性快照

- 使用 SQLite Online Backup API 生成 snapshot A，不得直接复制 `.sqlite3`、WAL 和 SHM 文件。
- Online Backup API 提供的事务一致性是导出的数据边界；不要求为导出停止本地代理。
- snapshot A 生成后必须运行 `PRAGMA quick_check` 和 schema compatibility 检查。
- 导出进度不得显示站点名、Key 前后缀或错误响应正文。

### 12.3 策略转换与换钥

对 snapshot A：

1. 生成新的 `Transport Data Key` 和 `transportKeyId`。
2. 在独立工作副本中应用表级包含、排除和重置策略。
3. 逐条用源设备密钥解密需包含的 secret。
4. 逐条使用传输数据密钥、新 nonce 和原规范 AAD 重新加密。
5. 更新 `key_id` 为 `transportKeyId`。
6. 删除不导出的 session/token/cookie secret 及其引用。
7. 对所有 JSON 文本再次运行集中脱敏和大小限制。
8. 验证旧明文兼容列为空。

任何凭据失败都必须终止整个导出。不得生成“部分凭据缺失但看似成功”的迁移包。

### 12.4 重建便携数据库

- 策略转换后必须使用 `VACUUM INTO` 或等价的受控重建生成 portable SQLite B。
- B 必须是新文件，不能原地 VACUUM。
- 该步骤用于移除已删除历史、旧字段内容和 SQLite freelist 中的残留。
- B 必须设置安全的最终 pragma；不得依赖 WAL/SHM 文件。
- B 必须通过 `quick_check`、`foreign_key_check`、schema 检查、record count 检查和逐条传输密钥解密检查。
- 必须扫描 B，确认源凭据 canary、源 `local_key` 和排除 session canary 不存在。

### 12.5 加密写出

- 最终目标先写入同目录临时文件：`.<name>.<exportId>.partial`。
- 使用 age 流式加密写入载荷，不得先生成包含 Manifest 明文的中间归档。
- 写入过程中同步计算 SQLite SHA-256。
- 完成后必须 flush 并调用文件句柄 `sync_all`。
- 关闭文件后执行一次完整回读自检：从刚写出的 age 临时文件重新读取并解密到独立受控 scratch，消费到认证 EOF，验证 Manifest、transport key ID、长度、SHA-256、SQLite `quick_check` 和 transport secret 解密。只检查加密前的 portable B 不算包自检。
- 自检成功后通过唯一的 Windows `AtomicFilePublishPort` 在同一目录发布为最终文件名；不得先删除已存在的目标再 rename。
- 最终路径已存在时只允许由保存对话框显式确认覆盖；实现不得静默覆盖。
- 保存对话框关闭时，export path token 必须记录目标当时为 absent，或记录已获覆盖确认的现有目标 file ID/volume/length。发布时 absent 目标若已出现，或 existing 目标身份已变化，必须返回 `selected_file_changed` 并保留现有文件；旧覆盖确认不能授权覆盖后来出现的不同文件。
- 覆盖已批准文件时必须使用系统原子替换能力，并在失败时保留原文件；目标卷不支持可证明的原子发布时必须失败。
- 失败时删除 `.partial`；若删除失败，在受控临时文件清单中登记，下次启动清理。

### 12.6 导出完成语义

只有最终原子发布成功后 operation terminal 才能返回现有通用终态 `completed`。返回结果只包含：

```text
exportId
outputPath
sizeBytes
includedCategoryNames
excludedCategoryNames
recordCounts
```

结果不得包含密码、transport key、secret ID、masked secret 或密文。

## 13. 导入流程

### 13.1 首版冲突策略

首版只支持：

- `restore_into_empty`：当前数据库没有用户资产。
- `replace_current`：明确替换当前数据。

首版禁止自动 merge。未来 merge 必须基于领域对象、稳定 ID 和显式冲突决策实现，不得通过 SQLite `INSERT OR REPLACE` 拼接。

`restore_into_empty` 不能只检查 `stations = 0`。必须由 `MigrationDataCatalog` 为每张用户拥有的表定义 occupancy 查询；只有 Station、Key、非设备 secret、自定义路由/价格/监控、通用登录资料和 draft 等用户数据全部为空，且数据库只含 binary 创建的默认设置与 built-in 行时才视为空。发现未知表或未知 setting key 时不得判空。

### 13.2 解密前预检

- 文件必须由用户通过打开对话框显式选择。
- 文件句柄最终必须解析为可读取的普通磁盘文件，不能是目录、管道或设备。
- 允许用户选择已完整落盘的云盘文件，但必须通过最终文件句柄执行大小限制和稳定文件标识校验，不能信任选择对话框返回的原始路径。
- 加密文件大小首版最大 2.25 GiB。
- 必须先检查 age header 合法性和 KDF 参数上限，再执行高成本 KDF。
- 同一 UI 会话连续失败 5 次后，必须增加本地退避；这只限制误操作和 UI 滥用，不宣称阻止离线破解。
- 选择文件时后端必须立即以只读、禁止共享删除的方式打开文件句柄，并通过句柄取得最终路径、卷序列号和文件 ID。后续解密从该句柄读取，不得按原路径重新打开，避免选择后文件被替换的 TOCTOU 风险。

### 13.3 Staging 解密

- 在应用受控 staging 目录创建唯一目录 `imports/<importId>/`。
- 权限必须限制为当前 Windows 用户。
- age 解密后的 Manifest 保存在 `Zeroizing` 内存对象中，不写独立 JSON 文件。
- portable SQLite 以 `portable.sqlite3.partial` 写入 staging。
- 必须严格执行载荷长度上限，不允许尾随字节。
- 必须消费并认证完整 age 流后，才将文件 rename 为 `portable.sqlite3`。
- 密码错误、截断和认证失败不得修改当前数据库或启动替换流程。

### 13.4 包与数据库验证

依次执行：

1. 验证 magic、Manifest schema 和所有长度。
2. 验证 format、formatVersion、minimumImporterVersion。
3. 拒绝高于当前 binary 支持范围的 database generation 或 schema version。
4. 根据 format、`portableSchemaProfile`、database generation/schema、`exportPolicyVersion`、`encryptionVersion` 和 required features 选择只读 `PortableSchemaReader`；不存在精确 reader 时拒绝。
5. 验证文件 SHA-256 与 Manifest、尾部摘要一致。
6. 校验 SQLite 文件头、page size、page count 与长度关系，再以 defensive read-only 模式打开。
7. 校验 `sqlite_schema` 结构指纹：只允许该 reader 声明的 table/index；拒绝 trigger、view、virtual table、未知对象、未知列和未知外键。
8. 运行 `quick_check`、`foreign_key_check` 和只读 schema compatibility 检查。
9. 核对 `recordCounts`，不一致则拒绝。
10. 验证每条 secret 的 `key_id`、encryption version、scope、owner 和引用关系。
11. 使用 transport key 逐条解密所有 secret；任意失败则拒绝。
12. 执行数据分类目录检查，迁移包不得带入禁止表或禁止明文字段。

对迁移包 SQLite 的处理必须满足：

- 它始终是不可信输入，即使 age 密码和认证有效。
- 禁止在该数据库上运行 migration、DDL、ATTACH、用户提供的 SQL 或来自 `sqlite_schema` 的 SQL。
- 禁止执行包内 trigger、view 或 extension；SQLite extension loading 必须关闭。
- 连接必须启用 `query_only`、`trusted_schema=OFF` 和实现可用的 SQLite length/column/VDBE operation 限制。
- Reader 只使用 binary 内固定、参数化、allowlist SELECT。
- 所有扫描必须具有行数、单字段字节数、JSON 深度和总读取字节上限；超限返回 `package_policy_violation`。
- 较旧 schema 的兼容由版本化 reader 将允许字段映射到当前模型实现，不得通过就地升级不可信数据库实现。

在验证结束前，UI 只能显示非敏感摘要：应用版本、导出时间、类别、站点数、Key 数、是否包含历史。不得显示 masked key 或登录账号。

导入检查阶段必须使用进程内 `ImportInspectionRegistry` 保存：

- `inspectedImportId`。
- 零化包装的 transport key。
- 已打开输入文件的稳定文件标识。
- staging SQLite 路径和 SHA-256。
- 非敏感摘要、创建时间和过期时间。

Registry 条目默认 10 分钟过期，单次确认后立即消费。过期时必须同步移除条目并 zeroize transport key，再调度删除对应 portable staging。prepare 消费必须原子地把 transport key、reader 和 staging 所有权移动到唯一的 `ImportPreparationLease`，registry 中立即不可再次查询或消费；lease 在 prepare 成功、失败或取消后 zeroize key 并清理不再需要的 portable staging。不得在消费瞬间提前清零 prepare 仍需使用的 key，也不得通过复制延长 key 生命周期。transport key 不得写入 staging、journal、operation result 或前端。若应用重启、条目过期或内存状态丢失，用户必须重新输入迁移密码并重新检查；不得为了恢复向磁盘写出 transport key。

### 13.5 目标换钥与重建

确认导入后：

1. 读取目标设备数据密钥；任何非 `NotFound` 错误 fail closed。
2. 若目标为全新安装，只能在 installation lease 下、且 data-store startup decision 已证明不存在任何需恢复数据库/journal 时创建数据密钥。
3. 使用当前 binary 的受信任 migrations 在 active database 所在卷创建全新的 target staging 数据库；不得复制包内 `sqlite_schema`。
4. 通过所选 `PortableSchemaReader` 按 catalog 依赖顺序读取允许的数据，并通过目标 persistence writer 写入新库。
5. 逐条使用 transport key 解密 secret，再用目标设备数据密钥和新 nonce 加密后写入；明文不得进入中间集合。
6. 所有 secret 的 `key_id` 更新为目标 active key ID。
7. 生成新的本地代理访问 Key 并用目标数据密钥加密。
8. Session、Cookie 和短期 Token 保持清空状态。
9. `local_proxy_start_on_launch` 强制为 `false`。
10. 清理运行中状态和设备相关路径。
11. 关闭 writer 后，将该受信任 target 数据库通过 `VACUUM INTO` 重建为最终 `target.sqlite3`，移除中间 freelist；该 VACUUM 的输入已经是应用新建的受信任 schema，不是包内数据库。

最终 `target.sqlite3` 必须在未激活状态下切换为无 WAL sidecar 的关闭态文件；不得把 target 的 `-wal` / `-shm` 作为激活工件。激活后由正常 persistence startup 根据正式 runtime 配置重新启用 journal mode。

`PortableSchemaReader` 只能输出版本化的领域记录，不能向目标库返回 SQL。`MigrationTargetWriter` 必须在一个受控事务中按以下依赖阶段写入；阶段内的精确表序由 catalog 声明并接受外键测试约束：

1. 运行当前 binary migrations，生成 schema、默认设置和 built-in 行。
2. 写入不依赖 secret 的根记录：Station（secret 引用暂置空）、model alias、model base price、监控模板和已允许的普通设置。
3. 写入 Station Key（secret 引用暂置空）及其他只依赖根记录的资产元数据。
4. 逐条完成 transport -> target 换钥并写入 `secrets`；不得缓存明文集合。
5. 回填 Station / Station Key secret 引用，写入 station credential、非设备 `app_secret_bindings`，再写入引用 secret 的 `common_login_profiles_json`。
6. 写入 capability、remote key 脱敏元数据、group binding、pricing 和 monitor 配置等子记录。
7. 仅在选择历史时，按父子顺序写入 request log -> attempt、collector run -> snapshot、group rate、balance、monitor run、change event；被排除的父引用必须清空或拒绝，规则由 catalog 固定。
8. 重建 health、runtime health 和 scheduler state，生成并绑定新的 Local Key，最后执行全库外键与 catalog 不变量检查后提交事务。

任一阶段失败必须回滚并销毁未激活 target；禁止临时关闭外键后带着悬空引用提交。为解决循环或可空引用，只允许使用上述“先置空、后回填”或 SQLite deferred foreign key，不得使用 `foreign_keys=OFF` 绕过验证。

解密工作区可位于默认 AppData，但准备激活的最终 target 数据库必须与 active 数据库位于同一卷。导入必须分别检查 AppData 工作区、active 数据卷和 backup 数据卷的可用空间。无法建立同卷受控 staging 时导入失败，不得退化为跨卷 copy-and-delete。

最终数据库必须通过：

- `quick_check`
- `foreign_key_check`
- 当前 schema compatibility
- 全 secret 目标密钥解密
- 禁止明文 canary 扫描
- transport key ID 零残留检查
- transport ciphertext canary 零残留检查

### 13.6 激活前备份

两种导入模式都必须：

- 使用现有 SQLite Online Backup API 创建经过验证的本机备份。
- 备份保留当前目标设备密钥加密的凭据，仅用于本机回滚。
- 备份验证失败时不得继续。
- 返回和 UI 必须记录备份路径，但不得自动上传或打开外部程序。

`replace_current` 额外要求明确的破坏性确认；`restore_into_empty` 虽然 occupancy 为空，仍备份默认设置、设备 Local Key 和数据库身份，不能省略回滚基线。

### 13.7 重启边界激活

正在运行的 SQLite 不允许被 UI 命令直接替换。目标库和 verified backup 准备完成后，必须先冻结运行时，再写入持久化 import activation journal，并要求重启应用。

Journal 至少包含：

```json
{
  "version": 1,
  "importId": "UUIDv7",
  "phase": "prepared",
  "activeDatabasePath": "...",
  "stagedDatabasePath": "...",
  "rollbackDatabasePath": "...",
  "verifiedBackupPath": "...",
  "targetDeviceKeyId": "device:<UUIDv7>",
  "expectedActiveBeforeSha256": "...",
  "expectedActiveBeforeSizeBytes": 0,
  "expectedStagedSha256": "...",
  "expectedStagedSizeBytes": 0,
  "expectedVerifiedBackupSha256": "...",
  "expectedVerifiedBackupSizeBytes": 0,
  "expectedActiveFileId": "...",
  "expectedStagedFileId": "...",
  "expectedVerifiedBackupFileId": "...",
  "observedRollbackFileId": null,
  "createdAt": "...",
  "updatedAt": "..."
}
```

允许的 phase：

```text
prepared
activation_started
replacement_committed
activated_validated
completed
rollback_started
rolled_back
manual_recovery_required
```

正常状态转换图固定为：

```text
prepared -> activation_started -> replacement_committed -> activated_validated -> completed
replacement_committed -> rollback_started -> rolled_back -> completed
activation_started -> rollback_started -> rolled_back -> completed  # 仅当实际文件证明替换已提交但新库无效
任何未能唯一证明的状态 -> manual_recovery_required
```

恢复器 MAY 根据实际文件身份跳过 journal 中尚未来得及持久化的中间 phase，但必须补写并回读相应 phase，不能倒退或跳过文件验证。`rolled_back` 必须表示旧 active 已恢复且通过完整验证；随后写入 outcome 为 rolled back 的 receipt，再进入 `completed`。`activated_validated` 表示新 active 的数据库、schema 和全部 secret 已验证；之后若托盘、更新器或其他非持久化服务启动失败，不得因此回滚健康数据库，而是保留该 phase 并在下次启动重试正常 composition。`completed` 只在 persistence runtime 达到 `Ready`、必要 application services 完成 composition 且 receipt 已耐久化后写入。

启动激活顺序：

1. 获取 installation lease。
2. 在打开 persistence runtime 和启动代理前读取固定位置的 journal。
3. 校验 journal 路径都位于允许的数据目录和 staging 目录内。
4. 校验 active 与 staged 的文件 ID、SHA-256 和同卷关系。
5. 按 journal 的 `targetDeviceKeyId` 预加载目标设备密钥；不可用时保持旧 active 不变并停留在恢复入口，不能创建替代 key 或改用其他 active key ID。
6. 将 journal 标记为 `activation_started` 并耐久化。
7. 通过唯一的 Windows `AtomicDatabaseReplacePort` 完成 active <- staged，并让旧 active 成为 rollback。该 port 必须优先使用适合普通磁盘文件的系统原子替换 API，不能散落调用 `std::fs::rename`。
8. 根据系统调用结果和 active/staged/rollback 的实际文件 ID、长度、SHA-256 判定替换是否提交；确认后标记 `replacement_committed`。
9. 用已按 ID 加载的目标设备密钥打开新 active 并执行完整验证。
10. 验证成功后标记 `activated_validated`，再进入正常启动。
11. 正常启动稳定完成后写入非敏感迁移 receipt，标记 `completed` 并清理 journal。

准备阶段的提交顺序固定为：

1. target staging 完整验证成功。
2. verified backup 完整验证成功。
3. coordinator 拒绝新的 mutation 和维护操作；停止 admission，取消并 join 后台任务，停止并 drain 代理，drain writes。
4. 在唯一 maintenance connection 上成功执行 WAL checkpoint/truncate，随后关闭 persistence runtime 和全部 SQLite connection；确认 active `-wal` 不存在或长度为零，才可删除零长度 `-wal` / `-shm`。checkpoint 失败、sidecar 非空或无法证明无打开连接时返回 `maintenance_freeze_failed`。
5. 通过新打开且禁止共享写/删的稳定 active 主文件句柄重新取得 file ID、长度和 SHA-256；若与备份前记录的数据库身份不兼容则失败，不得继续。
6. 创建完整 `prepared` journal，经 `AtomicJournalPort` 发布并回读验证。
7. 只有 journal 已持久化后，coordinator 才原子进入 `activation_pending`，operation 才能返回 `restartRequired = true`。

若第 3 步之后、确认第 6 步 journal 发布成功之前失败，active 数据库仍未替换：当前进程必须保持拒写并显示“准备失败，需要重启”，不得尝试在同一进程重新打开 persistence runtime；确认无有效 journal 时下次启动仍打开原 active。若 journal 发布结果无法确认，必须在下次启动按 journal 文件和候选文件的实际状态进入恢复判定，不能假设未提交。

每个 phase 转换必须：

- 通过唯一 `AtomicJournalPort` 在同目录写入 `.partial` journal。
- flush、文件句柄 `sync_all`，再使用 Windows 原子替换能力发布。
- 发布后重新打开并严格解析，确认 phase、hash 和 operation ID。
- 可重复执行且根据实际文件状态收敛。

Journal 固定保存在默认应用配置目录，不随 active database 一起替换。它不得包含密码、transport key、设备 key 或凭据值。

- Journal 最大 64 KiB、顶层字段封闭、未知版本拒绝。
- `targetDeviceKeyId` 只标识 prepare 时使用的目标设备密钥，不包含 key material；启动验证必须按该 ID 加载，不能静默改用重启时的其他 active key。
- `observedRollbackFileId` 在 `prepared` / `activation_started` 必须为 null；替换证据确认后必须写入实际 rollback file ID，并从 `replacement_committed` 起保持不变。各 phase 的 required/null 字段形状必须由严格 parser 校验。
- active、staged 和 verified backup 的 file ID、length、SHA-256 必须同时匹配；只匹配路径或 hash 之一不足以授权替换或回滚。
- Journal 目录 ACL 必须限制为当前用户和系统账户；路径不得经过 symlink/junction/reparse point。
- Journal 损坏、重复字段或路径越界时直接进入 `manual_recovery_required`，不得自动删除后按无 journal 启动。

崩溃恢复必须使用哈希与文件标识判定，而不是仅依赖 journal phase：

- active 匹配旧 hash 且 staged 匹配新 hash：替换尚未提交，可安全重试。
- active 匹配新 hash 且 rollback 匹配旧 hash：替换已提交，继续验证新库。
- 新库验证失败且 rollback 匹配旧 hash：通过同一原子替换 port 回滚，并隔离失败的新库。
- active、staged、rollback 无法唯一匹配已知 hash：进入 `manual_recovery_required`。

任何无法证明唯一安全状态的情况都进入现有 recovery UI，禁止猜测选择、禁止创建新库、禁止启动代理。

### 13.8 导入完成后的处理

- 首次成功启动后提示用户重新进行站点网页登录授权。
- 明确显示本地代理 Key 已更新，需要同步到 CCSwitch 或其他本地客户端。
- 原迁移包不自动删除；文件所有权属于用户。
- staging 中间文件在成功后清理。
- verified backup 与 rollback 文件按明确保留策略处理，首版不得静默删除 verified backup。
- 应用拥有的 transport key 和密码缓冲必须在完成或失败路径清零。

## 14. 并发、取消与后台任务

### 14.1 独占关系

以下操作互斥：

- 跨设备导出
- 跨设备导入
- 数据库 generation upgrade
- 数据目录 relocation
- 数据恢复 candidate activation
- 应用更新安装阶段

使用一个明确的 `DataMaintenanceCoordinator` 管理，不通过多个布尔值拼接判断。它至少具有以下封闭状态：

```text
normal
exporting
inspecting_import
preparing_import
activation_pending
recovering
```

`exporting` 和 `inspecting_import` 只排斥其他数据维护操作，不阻塞普通业务写入；`preparing_import` 在最终冻结点前允许失败回到 `normal`；`activation_pending` 和 `recovering` 必须阻止所有持久化 mutation、后台采集、代理启动和应用更新。

阻断必须有两层且集中实现：

- command/application admission 根据 generated command mutation metadata 拒绝新 mutation，不能要求每个 command 手写判断。
- persistence runtime 在 `activation_pending` / `recovering` 防御性拒绝新 write checkout，防止后台服务绕过 command 层。

已有写任务必须在进入 `activation_pending` 前完成 drain 或被明确取消并 join。仅取消 token 而不等待任务结束不算冻结成功。

### 14.2 导出并发语义

- 导出的一致性边界由 Online Backup snapshot 确定。
- snapshot 完成后，源数据库允许继续写入。
- 后续写入不会进入本次迁移包，UI 必须显示快照时间。
- 不要求停止代理，但 snapshot 建立期间必须使用现有 persistence 协调机制，不能绕过 runtime。

### 14.3 导入并发语义

- staging 解密和验证期间可继续查看当前数据。
- 用户确认替换后先阻止新的数据维护操作；target 构建失败时可安全回到正常模式。
- target 和 verified backup 验证完成后，必须停止后台任务 admission、取消并 join 后台任务、drain 并停止本地代理、drain persistence writes、checkpoint WAL 并关闭 persistence runtime。
- 关闭 runtime 并证明 active sidecar 已清空后，重新计算 active 主数据库的稳定文件 ID、长度和 SHA-256，发布并回读 `prepared` journal，最后原子进入 `activation_pending`；具体提交点以 13.7 的固定顺序为准。
- `activation_pending` 期间 UI 只能显示导入已准备完成、重启和错误恢复入口；不得继续提供可能写入旧库的功能。
- 实际激活发生在重启前置阶段，此时 persistence runtime 和代理尚未启动。

### 14.4 取消语义

- 解密、快照、策略转换、换钥和加密写出阶段允许协作式取消。
- 一旦 journal 进入 `prepared` / coordinator 进入 `activation_pending`，导入不再允许普通取消，只能通过重启完成激活或由启动恢复器回滚。
- UI 关闭不等于取消；operation 状态必须可重新查询。
- 取消成功只表示未激活新数据，不表示所有临时清理已经同步完成。

现有内存 `OperationRegistry` 不足以承担跨重启激活状态；长任务进度可复用它，但导入激活真相必须持久化在 import journal 中。

## 15. 模块与职责划分

建议新增：

```text
src-tauri/src/application/data_migration/
  mod.rs                    # use case orchestration
  export_service.rs
  import_service.rs
  policy.rs                 # product-level export options
  errors.rs

src-tauri/src/services/portable_migration/
  mod.rs
  format.rs                 # fixed payload framing and manifest
  age_envelope.rs           # only age adapter
  catalog.rs                # exhaustive table/field classification
  snapshot.rs               # online backup boundary
  transform.rs              # include/exclude/reset policy
  rekey.rs                  # source -> transport -> target
  validate.rs
  staging.rs
  activation_journal.rs
  recovery.rs
  limits.rs

src-tauri/src/commands/data_migration.rs
src-tauri/src/ipc/dto/data_migration.rs

src/features/settings/data-migration/
  DataMigrationSection.tsx
  ExportMigrationDialog.tsx
  ImportMigrationDialog.tsx
  ImportMigrationSummary.tsx
  useDataMigrationController.ts
```

依赖方向：

- `commands` 只做 DTO 校验、授权边界和调用 facade。
- `application` 负责编排、策略确认和 operation 生命周期。
- `services/portable_migration` 负责文件格式、密码学适配、staging 和恢复，不依赖 React 或 Tauri Window。
- `persistence` 提供一致性快照、只读检查和数据库重建原语，不知道产品 UI。
- 密钥系统通过 `SecretKeyResolver` 接口注入，测试不得依赖真实 Windows Credential Manager。

禁止：

- command 直接执行 SQL。
- React 直接拼装 Manifest。
- 导出服务直接调用 `keyring::Entry`。
- 在多个模块复制 secret 表遍历逻辑。
- 使用 shell、PowerShell、`sqlite3.exe`、`zip.exe` 或外部进程完成迁移。

## 16. IPC 合同

v1 命令必须进入现有 generated IPC registry，使用闭合 DTO 和 capability 声明。命令集合固定为：

```text
get_portable_migration_capability
choose_portable_export_path
start_portable_export
get_portable_export_result
choose_portable_import_file
start_portable_import_inspection
get_portable_import_inspection
start_portable_import_prepare
get_portable_import_prepare_result
get_portable_migration_operation
get_portable_import_recovery_state
```

关键 DTO：

```ts
type PortableExportOptionsDto = {
  includeHistory: boolean;
};

type StartPortableExportInputDto = {
  outputPathToken: string;
  passphrase: string;
  passphraseConfirmation: string;
  options: PortableExportOptionsDto;
  idempotencyKey: string;
};

type InspectPortableImportInputDto = {
  inputPathToken: string;
  passphrase: string;
  idempotencyKey: string;
};

type PreparePortableImportInputDto = {
  inspectedImportId: string;
  mode: "restoreIntoEmpty" | "replaceCurrent";
  confirmationText: string;
  idempotencyKey: string;
};

type PortableMigrationOperationStartedDto = {
  operationId: string;
  resourceId: string;
  resourceKind: PortableMigrationResourceKindDto;
};

type PortableMigrationResourceKindDto = "export" | "inspection" | "import";

type PortableMigrationResultInputDto = {
  resourceId: string;
};

type PortableMigrationOperationInputDto = {
  operationId: string;
};

type PortableImportRecoveryReasonCodeDto =
  | "activation_validation_failed"
  | "atomic_replace_failed"
  | "journal_invalid"
  | "artifact_identity_mismatch"
  | "rollback_validation_failed";

type PortableImportRecoveryStateDto =
  | { state: "none" }
  | { state: "activationPending"; importId: string }
  | { state: "activated"; importId: string }
  | { state: "rolledBack"; importId: string; reasonCode: PortableImportRecoveryReasonCodeDto }
  | { state: "manualRecoveryRequired"; importId: string | null; reasonCode: PortableImportRecoveryReasonCodeDto };
```

- `start_portable_export` 返回 operation ID 与预分配 export ID；结果通过 `get_portable_export_result(exportId)` 读取。
- `start_portable_import_inspection` 返回 operation ID 与预分配 inspection ID；检查结果通过 `get_portable_import_inspection(inspectionId)` 读取。
- `start_portable_import_prepare` 是最后一次破坏性确认。它消费 inspection、创建 target、创建 verified backup、冻结写入并写入 activation journal；返回 operation ID 与 import ID。
- `replaceCurrent` 的 `confirmationText` 必须按 UTF-8 精确等于 `替换当前数据`，不做 trim/normalization；`restoreIntoEmpty` 必须传空字符串且仍由后端重做 occupancy 检查。其他值返回 `confirmation_mismatch`。
- `get_portable_import_prepare_result(importId)` 只返回非敏感状态和 `restartRequired`。
- `get_portable_import_recovery_state` 返回上述闭合 recovery state；`activationPending` 和 `rolledBack` 是状态而不是 command error。`reasonCode` 只能来自恢复模块自己的闭合脱敏 allowlist，不得传递原始 I/O 文本。
- `get_portable_migration_capability` 至少返回 `enabled`、闭合 `blockedReasons`、支持的 format/profile、当前 schema profile、历史选项和 `PortableMigrationLimitsV1` 的只读投影。`blockedReasons` 只允许 `security_policy_not_approved`、`unsupported_platform`、`security_baseline_incomplete`、`credential_store_key_missing`、`credential_store_unavailable`、`data_store_not_writable`、`maintenance_in_progress`。当前安全政策未更新时 `enabled = false` 且包含 `security_policy_not_approved`；所有 start command 后端也必须返回 `feature_unavailable`，不能只禁用按钮。多个 reason 按上述固定顺序返回并去重。
- `resourceId` 必须与命令种类匹配并由响应同时携带闭合的 `resourceKind`；各 `get_*_result` 使用 `PortableMigrationResultInputDto`，不得接受路径、密码或完整 start input 作为查询条件。
- 全局 `OperationRegistry` 负责有界进度、取消和 terminal；export/inspection/import registry 负责各自的 typed result，不能把 JSON 结果塞进自由文本 progress。
- `get_portable_migration_operation` 必须校验 operation owner 属于 portable migration，并把内部进度投影为第 17 节闭合 code；前端不得消费全局 operation DTO 的自由文本 `message` 作为迁移状态。取消复用现有 `cancel_operation`，但进入 commit barrier 后必须返回不可取消或 `result_unknown` 的现有终态语义。
- operation terminal 为 `completed` 但 typed result 缺失时必须返回 `result_unknown`，不得猜测重试非幂等操作。
- 三个 `start_*` 都必须携带 `idempotencyKey`。它在进程生命周期内按 `(commandKind, idempotencyKey)` 绑定规范化输入摘要；摘要必须排除密码原文而使用进程随机 HMAC 后的密码等价标识。相同 key + 相同摘要返回原 operation，相同 key + 不同摘要返回 `idempotency_conflict`。
- `idempotencyKey` 由前端为每次用户提交生成规范小写 UUIDv7；后端严格解析并拒绝任意字符串、空值和非 v7 UUID。export/inspection/import resource ID 也使用后端生成的规范 UUIDv7；现有通用 operation ID 继续使用正整数字符串，两者不得混用。
- 规范化摘要包含 path token 所绑定的稳定文件/目录身份、选项、导入模式和确认语义；不得只对 token 字符串或可变路径做摘要。
- typed result registry 每类最多 64 项，terminal result 默认保留 30 分钟；容量回收或过期后返回 `result_unknown`。inspection 仍使用 10 分钟的更短有效期，过期返回 `import_inspection_expired` 并立即 zeroize transport key。
- `start_portable_import_prepare` 在消费 inspection 前必须先建立 idempotency 绑定；相同请求只能有一个 prepare owner。消费后重试只能返回原 operation/result，不能再次创建 target 或 backup。
- 所有 start 命令都必须先查询/保留 idempotency binding，再消费一次性 path token 或 inspection；若 binding 已存在且摘要相同，直接返回原 operation，不得因 token 已消费而误报失败。

路径选择必须返回后端生成的短期 `pathToken`，后续命令使用 token，不接受前端任意路径字符串。Token：

- 导入 token 持有已打开只读文件句柄，并绑定卷序列号、文件 ID、初始大小和操作类型；消费时从原句柄读取。
- 导出 token 持有已验证的父目录句柄、批准的叶文件名和选择时目标状态（absent 或已确认覆盖的稳定文件身份）；最终文件通过同目录 `CreateNew` 临时文件和原子 publish 产生。
- 绑定当前进程和操作类型。
- 一次性使用。
- 默认 10 分钟过期。
- 不得序列化到日志或 localStorage。

密码不允许出现在 operation progress 和 terminal result 中。生成绑定的序列化 fixture 必须使用固定假值，并验证诊断输出不包含该假值。

## 17. UI 规格

入口位于“设置 -> 数据与备份”，分为三个紧凑区域：

1. 本机备份。
2. 同机数据目录。
3. 跨设备搬家。

### 17.1 导出向导

步骤：

1. 选择内容：核心配置固定包含；历史记录可选。
2. 设置迁移密码：密码、确认密码、显示/隐藏按钮。
3. 选择输出文件。
4. 显示非敏感摘要并开始导出。
5. 显示阶段进度和最终路径。

进度阶段使用稳定代码映射本地文案，不直接展示后端自由文本：

```text
validating_source
creating_snapshot
applying_export_policy
rekeying_secrets
rebuilding_database
encrypting_package
verifying_package
finalizing
```

### 17.2 导入向导

步骤：

1. 选择 `.rpd-move` 文件。
2. 输入迁移密码。
3. 验证并显示非敏感摘要。
4. 选择“导入到空数据”或“替换当前数据”。
5. 对替换模式要求输入固定确认文本。
6. 准备 staging、换钥和备份。
7. 准备成功后进入只读维护界面，提示必须重启完成激活；此时代理和后台任务已经停止。
8. 重启后显示成功、回滚或恢复状态。

UI 不得提供“忽略错误继续”“跳过损坏凭据”或“保留源机 Local Key”选项。

导入进度阶段同样使用闭合 code：

```text
opening_package
deriving_package_key
decrypting_package
validating_manifest
validating_portable_database
validating_transport_secrets
creating_target_database
copying_portable_data
rekeying_target_secrets
creating_verified_backup
freezing_runtime
writing_activation_journal
restart_required
```

进度可以显示已处理字节或记录数，但不得把多个阶段伪装成线性总百分比。KDF 阶段只显示不确定进度状态。

## 18. 错误模型

外部错误使用稳定 code，用户文案在前端映射。错误 detail 必须脱敏且有大小上限。v1 command contract 必须使用以下闭合 code；新增 code 需要同步更新 DTO、前端映射、fixture 和兼容测试。

v1 code：

```text
credential_store_unavailable
credential_store_corrupt
credential_store_key_missing
source_secret_validation_failed
feature_unavailable
weak_passphrase
passphrase_confirmation_mismatch
migration_busy
upgrade_in_progress
relocation_in_progress
insufficient_space
invalid_output_path
selected_file_changed
package_too_large
package_resource_limit_exceeded
package_authentication_failed
package_kdf_unsupported
package_format_unsupported
package_importer_too_old
package_feature_unsupported
package_policy_unsupported
package_encryption_unsupported
package_manifest_invalid
package_schema_too_new
package_schema_unsupported
package_integrity_failed
package_database_invalid
package_policy_violation
transport_secret_validation_failed
target_secret_rekey_failed
backup_failed
activation_prepare_failed
manual_recovery_required
path_token_expired
path_token_invalid
import_inspection_expired
import_inspection_invalid
database_not_empty
confirmation_mismatch
idempotency_conflict
maintenance_freeze_failed
operation_cancelled
result_unknown
io_failed
internal
```

- 密码错误、age 认证失败和密文截断统一为 `package_authentication_failed`。
- capability `enabled = false` 时，所有 start command 使用 `feature_unavailable`；具体闭合原因只通过 capability DTO 返回，不复制成自由文本 error detail。
- 超出已声明格式、schema、feature 或 KDF 能力使用对应 unsupported code，不映射为 `internal`。
- 未知 `requiredFeatures` 使用 `package_feature_unsupported`；未知 format/framing 使用 `package_format_unsupported`；generation/profile/schema 无 reader 使用 `package_schema_unsupported`，明确高于已知 schema 上界时使用 `package_schema_too_new`。
- `minimumImporterVersion` 高于当前 binary 使用 `package_importer_too_old`；unsupported export policy 使用 `package_policy_unsupported`；unsupported secret encryption version 使用 `package_encryption_unsupported`。`package_policy_violation` 仅表示包内容违反已支持 policy，不能用于表示版本不支持。
- 文件在选择与读取之间发生身份变化使用 `selected_file_changed`。
- token 缺失、操作类型不匹配或已消费使用 `path_token_invalid`，仅 TTL 到期使用 `path_token_expired`；inspection 缺失、类型不匹配或已消费使用 `import_inspection_invalid`，仅 TTL 到期使用 `import_inspection_expired`。
- `restore_into_empty` occupancy 不为空使用 `database_not_empty`；确认文本不匹配使用 `confirmation_mismatch`；冻结或 drain 不能在时限内证明完成使用 `maintenance_freeze_failed`。
- `DataKeyLoadError::Unavailable`、`PermissionDenied`、`Unsupported` 和无法安全细分的系统错误统一映射 `credential_store_unavailable`；`Corrupt` 映射 `credential_store_corrupt`。内部日志仍保留非敏感类别，前端不得据此决定是否创建密钥。
- 非 first-run / 非显式 pending rotation 上下文中的 `DataKeyLoadError::NotFound` 映射 `credential_store_key_missing` 并进入恢复；不得映射成可自动修复的普通 unavailable。
- 已知 operation terminal 但 typed result 无法确认使用 `result_unknown`，不得自动重试 mutation。
- 只有确实无法归类的实现缺陷使用 `internal`。

内部错误链可以记录模块、阶段、operation ID 和系统错误类别，但不得记录：

- 用户密码。
- transport key 或设备 key。
- API Key、Cookie、Token、密码。
- 完整 Manifest。
- secret ciphertext 或 nonce。
- 用户账号和站点响应正文。

## 19. 临时文件与清理

受控临时目录：

```text
<app-data>/migration-staging/exports/<exportId>/
<app-data>/migration-staging/imports/<importId>/
<active-data-dir>/.relay-pool-import-staging/<importId>/
```

要求：

- 目录名只允许应用生成的 UUIDv7。
- 所有已经存在的根目录、输入文件和父目录在使用前必须通过最终句柄解析规范路径，并验证仍位于受控根目录。尚不存在的文件不能伪造 `canonicalize`：必须先验证规范父目录句柄，再校验单一叶文件名并相对该句柄使用 `CreateNew` 创建。
- 禁止跟随 symlink、junction 或 reparse point 越出根目录。
- 安全判断不得采用字符串前缀；Windows 路径比较必须处理卷标、大小写、8.3 alias、UNC 和 reparse point，并以最终句柄的卷序列号/file ID 与受控根身份为准。
- active 数据目录内的 staging 只保存已使用目标设备密钥换钥的最终候选，不保存密码、transport key 或源密钥。
- 清理只允许针对经过验证的单个 operation 目录，禁止递归删除计算不明的宽路径。
- 启动时清理无 journal 引用、超过 24 小时的 `.partial` staging。
- 有活动 journal 的目录不得自动删除。
- 清理失败只记录路径类别和 operation ID，不记录用户选择的完整路径。
- 不承诺 SSD 上的物理安全擦除；安全性依赖临时数据库始终只含密文且密码/密钥不落盘。

## 20. 兼容性与演进

存在四个独立版本维度，不得混用：

- `formatVersion`：`.rpd-move` 外层和载荷 framing。
- `databaseSchemaVersion`：SQLite schema。
- `exportPolicyVersion`：表分类、排除和重置语义。
- `portableSchemaProfile`：允许 importer 选择精确的只读 schema reader。

兼容规则：

- `formatVersion` 是严格递增整数，不使用未定义的 major/minor 推断。
- 新增可选能力必须放入 `extensions`；修改 framing、密码学容器、顶层必需字段或现有字段语义必须提升 `formatVersion`。
- Importer 对某个已发布 format/profile 的支持期不得短于“最后一个仍导出该格式的稳定版发布后 24 个月”。移除 reader 必须发生在应用 major release、提前写入 release note，并保留独立恢复工具或旧版下载说明。
- 较旧 portable schema 只能通过对应 `PortableSchemaReader` 映射到当前新库；较新或未知 profile 必须拒绝。
- 每个受支持 format/profile 组合必须保留至少一个不含真实 secret 的加密 fixture 和期望投影；最近两个组合还必须保留故障 fixture。
- 导出只产生当前版本，不继续产生旧格式。
- Binary 必须维护显式 `PortableMigrationCompatibilityRegistry`，列出 format、profile、database generation/schema version 范围、export policy version、secret encryption version、required feature 和 reader；不得用大于/小于比较猜测兼容。
- 未来 macOS/Linux 可复用迁移格式，仅替换 `DeviceKeyStore`；不得改变 transport key 语义。

## 21. 可观测性与审计

允许记录：

- operation ID、export/import ID。
- 阶段代码、耗时、总字节数。
- 表级记录数量。
- 成功、取消、回滚或错误 code。
- 源和目标 schema 版本。

禁止记录敏感值。所有可观测字段必须经过 allowlist DTO，而不是对内部 struct 直接 `Debug`。

建议指标：

```text
portable_export_duration_ms
portable_import_prepare_duration_ms
portable_import_activation_duration_ms
portable_migration_bytes
portable_migration_secret_count
portable_migration_failure_total{stage,code}
portable_migration_rollback_total{result}
```

本地产品不上传这些指标；它们只进入受控本地诊断，并遵循现有诊断脱敏策略。

## 22. 测试要求

### 22.1 单元测试

- Manifest 严格解析、重复 key、边界长度和未知版本。
- framing 截断、尾随字节、长度溢出。
- 密码校验和零化包装类型不实现敏感 `Debug`。
- `MigrationDataCatalog` 全表覆盖。
- 每种表策略和敏感字段策略。
- secret rekey 正常往返、错误 AAD、错误 key、错误 nonce、未知 encryption version。
- Windows Credential Manager 错误分类，证明非 `NotFound` 不创建密钥。
- import journal 每个 phase 的幂等转换。
- 路径 canonicalization、junction/reparse point 和越界拒绝。

### 22.2 集成测试

必须使用三把不同密钥：source device key、transport key、target device key。

测试必须证明：

1. 源密钥可解源库，不可解便携库和目标库。
2. transport key 可解便携库，不可解源库和最终目标库。
3. 目标密钥只可解最终目标库。
4. 迁移包中不存在源设备密钥。
5. 默认导出不存在 Local Key、Cookie、Access Token、Refresh Token canary。
6. Station Key 和明确保存的登录密码能够在目标库恢复。
7. 历史关闭和历史开启分别符合策略。
8. `quick_check` 和 `foreign_key_check` 通过。
9. 导入受支持的较旧 schema 能通过对应 `PortableSchemaReader` 映射到当前新建 schema。
10. 导入较新 schema 被拒绝且当前库不变。

### 22.3 故障注入

在以下边界逐一注入失败：

- 系统凭据读取、创建和回读。
- Online Backup 中途。
- secret N/2 解密或重加密。
- `VACUUM INTO`。
- age 写入中途。
- 输出文件 flush、sync 和 rename。
- package 自检。
- staging 解密每个分块。
- 目标备份。
- journal 每次写入、sync 和 rename。
- active -> rollback rename 后崩溃。
- staged -> active rename 后崩溃。
- 新 active 验证失败。
- rollback rename 中断。

每个测试必须断言最终状态只能是以下之一：

- 原数据库仍为 active 且完整。
- 新数据库为 active 且完整。
- 自动恢复了原数据库。
- 明确进入 `manual_recovery_required`，两个候选均保留且代理未启动。

不得出现部分更新数据库或无 journal 的模糊状态。

### 22.4 恶意输入与资源限制测试

- 超大加密文件。
- 恶意 KDF work factor。
- 超大 Manifest。
- 超大 sqlite length。
- 整数溢出长度。
- 截断 age stream。
- 错误最终认证标签。
- 修改 Manifest、SQLite 或摘要。
- 非 SQLite 内容和畸形 SQLite。
- foreign key 破坏。
- 重复 JSON key。
- 超长字符串和超多表行。
- 文件替换竞态。
- junction/reparse point 越界。

### 22.5 UI 与 IPC 测试

- generated IPC schema 和序列化 fixture。
- 路径 token 过期、复用和跨操作拒绝。
- 密码不进入 query cache、localStorage、toast、错误 detail 和 operation progress。
- 导入替换必须经过明确确认。
- 操作中刷新页面后可恢复进度。
- 重启后正确展示 activation 结果。
- DemoBackend 明确返回 unsupported，不模拟成功迁移。

### 22.6 手工跨机资格测试

正式发布前至少完成：

1. Windows 10 源机 -> Windows 11 目标机。
2. Windows 11 源机 -> 新 Windows 用户目标环境。
3. 非 ASCII 站点名、账号名和长路径。
4. 数据目录位于默认路径和自定义路径。
5. 包通过 U 盘、局域网文件复制和云盘下载后导入。
6. 导入后真实 Station Key 请求、登录重新授权、CCSwitch Local Key 更新。
7. 导入期间强制结束进程后的自动恢复。

## 23. 性能目标

在参考环境 4 核 CPU、8 GiB 内存、SSD 上：

- 1 GiB 源数据库导出和导入峰值 RSS SHOULD 小于 512 MiB。
- secret 处理必须流式，内存不随 secret 总大小线性增长。
- 文件复制和 age 加解密使用固定大小缓冲区，建议 1 MiB 以内。
- UI 主线程不得执行 KDF、SQLite、hash 或文件 I/O。
- 同一阶段的操作进度更新必须同时节流：距上次至少 250 ms，且可计算进度至少变化 1%；phase/terminal 切换可立即发送。KDF 等不可计算阶段只发送开始和结束，不发送伪百分比。
- 导入验证必须有明确阶段，不使用虚假的线性百分比。

## 24. 分阶段实施

### Phase A：安全前置

- 系统凭据错误分类和启动顺序修复。
- `key_id`、`encryption_version` schema 与旧行迁移。
- Local Key 进入统一 secret 存储。
- `SecretRekeyService` 和全库 secret 验证。

发布门槛：旧库升级故障注入通过，凭据读取错误不会覆盖密钥，SQLite 不含 Local Key 明文。

### Phase B：格式与导出

- `MigrationDataCatalog`。
- `.rpd-move` framing、age adapter、Manifest。
- 一致性 snapshot、策略转换、transport rekey、重建、自检和原子输出。
- 导出 UI。

发布门槛：导出包可被独立测试工具验证，默认排除项 canary 全部通过。

### Phase C：导入准备

- path token。
- staging 解密、资源限制和包检查。
- transport -> target rekey。
- 替换前 verified backup。
- 导入预览和确认 UI。

发布门槛：失败不会修改 active 数据库，三密钥隔离测试通过。

### Phase D：激活与恢复

- import activation journal。
- 重启前置激活、自动回滚和 recovery UI 集成。
- 所有 phase 故障注入。

发布门槛：崩溃矩阵不存在数据丢失或静默选择。

### Phase E：兼容与资格

- 旧格式 fixture。
- Windows 跨机手工资格测试。
- 用户文档、本机备份与跨机迁移的区别说明。
- `docs/SECURITY_EXPORT_IMPORT.md` 更新。

## 25. 完成标准

只有同时满足以下条件才能宣称功能完成：

- 所有安全前置完成。
- `.rpd-move` v1 格式和兼容策略冻结并有 fixture。
- 源设备密钥从未进入迁移包或临时文件。
- 默认排除 session、token、cookie 和 Local Key。
- 所有长期凭据在目标机使用目标设备密钥重新加密。
- 导出结果经过完整回读自检后才对用户可见。
- 导入只通过 staging 和重启 journal 激活。
- 替换前 verified backup 成功。
- 全部单元、集成、故障注入、恶意输入、IPC 和 UI 测试通过。
- TypeScript/Vite 检查和 Cargo 检查通过。
- Windows 10/11 跨机资格测试通过。
- 日志、诊断、截图和迁移进度中无完整敏感值。
- 文档明确说明迁移密码丢失无法恢复迁移包，原设备数据密钥丢失无法解密原本机备份。

## 26. 后续扩展点

以下能力可以在 v1 稳定后扩展，但必须复用相同安全边界：

- 基于领域对象的选择性导出和 merge import。
- 面向组织公钥的 age recipient 导出，替代共享密码。
- macOS Keychain 和 Linux Secret Service 数据密钥提供者。
- 定期加密便携备份。
- 独立的密钥轮换 UI。
- 对超大历史库的分卷或分层迁移。

这些扩展不得改变以下不变量：设备主密钥不离机、便携文件必须认证加密、目标机必须重新换钥、导入必须 staging、激活必须可恢复。
