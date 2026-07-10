# Windows 自动更新设计

## 1. 目标

为 Relay Pool Desktop 增加面向 Windows 的应用内自动更新能力。应用启动后异步检查公开 GitHub Releases；发现新版本时由用户确认，随后下载经过签名验证的更新包、停止本地代理、安装并重启应用。

第一阶段只支持 Windows x86_64、NSIS current-user 安装，不支持 Windows ARM64、macOS、Linux、静默强制更新、多发布通道、增量更新、自动降级、下载取消或断点续传。

## 2. 当前基线

- 项目使用 Tauri 2、React、TypeScript 和 Vite。
- `package.json`、`src-tauri/Cargo.toml` 和 `src-tauri/tauri.conf.json` 的版本目前均为 `0.0.0`。
- `src-tauri/tauri.conf.json` 当前设置了 `bundle.active: false`。
- 项目尚未安装 Tauri updater/process 插件，也没有 GitHub Actions 发布工作流。
- Relay Pool Desktop 会运行本地代理，因此更新安装前必须协调代理关闭，不能直接强制退出进程。

## 3. 技术路线

采用 Tauri 2 官方 updater：

- Rust 侧使用 `tauri-plugin-updater` 和 `tauri-plugin-process`。
- 前端使用 `@tauri-apps/plugin-updater` 和 `@tauri-apps/plugin-process`。
- GitHub Actions 使用 `tauri-apps/tauri-action` 构建 Windows NSIS 安装包并创建 GitHub Release。
- 发布产物包括安装包、updater artifact、签名文件和 `latest.json`。
- `src-tauri/tauri.conf.json` 启用 `bundle.active`，将 `bundle.targets` 固定为 `nsis`，启用 `bundle.createUpdaterArtifacts`，并使用 updater 默认推荐的 Windows `passive` 安装模式。
- NSIS 使用 current-user 安装范围，避免自动更新依赖管理员权限。
- 客户端从固定地址读取更新元数据：
  `https://github.com/hardyz0517/relay-pool-desktop/releases/latest/download/latest.json`。
- updater 公钥编译进应用；私钥注入 GitHub Actions Secrets，并另做离线加密备份。
- `src-tauri/capabilities/default.json` 至少授予 `updater:default` 和 `process:allow-restart`；不授予 updater/process 插件的无关权限。

Tauri updater 签名与 Windows Authenticode 代码签名是两套机制。updater 签名用于验证更新包完整性，属于第一阶段必需项。Authenticode 用于降低 Windows SmartScreen 警告，建议在正式公开分发前接入，但不阻塞 updater MVP。

## 4. 客户端架构

更新能力放在独立的 `src/features/updater/` 模块，由全局更新控制器持有状态。应用外壳只负责启动检查和挂载全局对话框；设置页通过同一控制器显示当前版本、最近检查结果并触发手动检查。更新逻辑不得重复实现在设置页中。

建议状态模型：

- `idle`：尚未检查或本次流程已结束。
- `checking`：正在读取并验证更新元数据。
- `available`：发现新版本，等待用户决定。
- `downloading`：正在下载更新包，并持续更新进度。
- `preparing`：下载完成，正在排空并停止本地代理。
- `installing`：已开始安装，即将退出并重启。
- `failed`：本次检查、下载或安装准备失败，可以重试。

控制器必须防止并发检查和重复弹窗。开发模式和普通浏览器/Vite 环境不执行启动自动检查；手动检查也应返回明确的“当前环境不支持更新”结果，而不是产生未处理异常。

最近检查结果只保留在当前应用进程的内存中，第一阶段不新增数据库字段。用户选择稍后更新、更新对象被替换或流程结束时，控制器必须调用 `Update.close()` 释放 updater resource。发行说明按纯文本或经过安全处理的 Markdown 渲染，不得直接传入 `dangerouslySetInnerHTML`。

## 5. 用户流程

1. 应用主界面完成初始化后等待 3 至 5 秒。
2. 后台检查 `latest.json`，不得阻塞应用启动或本地代理运行。
3. 没有新版本时静默结束。
4. 检查失败时不弹阻塞式错误框；设置页记录最近检查失败并提供重试。
5. 发现新版本时弹出更新对话框，显示当前版本、目标版本和发行说明。
6. 用户选择“稍后更新”时，本次运行不再自动提醒；用户仍可在设置页手动检查。
7. 用户选择“立即更新”后开始下载，展示进度和已下载大小。Tauri updater 2.x 不提供下载取消 API，因此下载阶段不显示无法兑现的取消操作。
8. 下载完成后停止接受新的代理请求，并给正在处理的请求一个有上限的排空窗口。
9. 本地代理排空并停止后安装更新，再调用 process 插件重启应用。数据库、采集器和监控器沿用正常进程退出生命周期，不由代理协调器越权关闭。
10. 如果关闭失败或超过超时，不执行安装；当前版本继续运行并提示用户稍后重试。

更新对话框提供“稍后更新”和“立即更新”。下载或准备阶段失败时提供“重试”和“打开 GitHub Release 页面”。不得在应用启动瞬间弹窗，也不得在用户确认前自动下载更新包。

## 6. 本地代理协调

当前 `ProxyRuntimeState::stop()` 会立即设置停止信号、清除活动请求计数，并且只 join listener/accept 线程；每个请求工作线程是独立线程，没有被该方法 join。因此现有 `stop()` 不能证明在途请求已经完成，实施自动更新前必须先补齐代理排空能力。

安装前准备流程由后端提供单一、可测试的协调接口，前端不直接拼接多个代理命令。代理内部增加 `running -> draining -> stopped` 生命周期和独立的 `accepting_requests` 标志。该接口负责：

- 进入 `draining` 后保持 listener 存活，但对新请求快速返回可重试的 `503`，不再启动新的代理工作线程。
- 保留并观察现有 `active_requests` 计数，默认最多等待 30 秒归零。
- 计数归零后调用代理停止逻辑，确认 listener 已停止，再返回 `stopped`。
- 排空超时时恢复 `accepting_requests`，回到 `running`，当前版本继续提供服务。
- 返回可区分的成功、超时和关闭失败结果。

只有协调接口返回成功后，客户端才调用 updater 安装并重启。更新安装失败且应用没有退出时，应尝试重新启动代理；如果代理无法恢复，则明确提示用户当前代理状态需要人工检查。

## 7. 版本和发布规则

采用 SemVer，以 `package.json` 的 `version` 作为应用版本的唯一来源。`src-tauri/tauri.conf.json` 的 `version` 设置为 `../package.json`，由 Tauri 直接读取该字段；`src-tauri/Cargo.toml` 的 package version 继续保持一致。

CI 要求以下版本解析结果一致：

- `package.json`
- `src-tauri/Cargo.toml`
- `src-tauri/tauri.conf.json` 解析出的 package.json version
- Git tag 去掉前缀 `v` 后的值

提供一个版本更新脚本同时修改 `package.json` 和 `src-tauri/Cargo.toml`。CI 在构建前验证上述四者一致，不一致则立即失败。

版本规则：

- 修复版本，例如 `0.1.1`，用于向后兼容的缺陷修复。
- 次版本，例如 `0.2.0`，用于向后兼容的新功能。
- 主版本，例如 `1.0.0`，用于稳定发布或包含明确不兼容变化的版本。

第一阶段只有 stable 通道。GitHub 的 latest Release 始终代表稳定版，不使用 beta/nightly endpoint。

首个包含 updater 公钥、endpoint 和 capability 的版本定义为引导基线 `0.1.0`。更早且不包含 updater 的构建无法自动获得该能力，必须由用户手动安装 `0.1.0`；第一条真实自动更新路径从 `0.1.0 -> 0.1.1` 开始验证。

## 8. GitHub Actions 发布流程

1. 开发者运行版本更新脚本并提交版本与发行说明。
2. 创建并推送 `vX.Y.Z` 标签。
3. tag workflow 在 `windows-latest` 上运行 TypeScript/Vite 检查和 Cargo 检查，并将构建目标固定为 `x86_64-pc-windows-msvc`。
4. CI 验证 tag、package.json version、Cargo package version 和 Tauri 解析出的应用版本一致。
5. `tauri-apps/tauri-action` 构建 Windows x86_64 NSIS bundle 和 updater artifacts。
6. Action 创建 Draft Release，并上传安装包、签名和 `latest.json`。
7. Draft 阶段验证安装包可安装、所有预期产物存在、签名文件非空且 `latest.json` 结构和下载文件名正确。
8. 使用专用测试配置构建一个指向本地静态服务器的旧版本客户端。测试脚本下载 Draft 产物，将 `latest.json` 的 artifact URL 改写到本地服务器，并保持真实 artifact 和签名不变，以完成检查、下载、签名验证、安装和重启的发布前端到端测试。测试配置可以单独启用 `dangerousInsecureTransportProtocol` 访问 localhost HTTP，但该配置不得合并进正式构建；正式 GitHub endpoint 始终使用 HTTPS。
9. 发布前测试通过后，维护者将 Draft 发布为正式 Release。Draft 和 prerelease 不会被 `/releases/latest/` 返回，因此不能把 Draft 当作真实 GitHub latest endpoint 的升级测试。
10. 正式发布后，从已安装的上一稳定版本验证公共 `/releases/latest/download/latest.json` 可达、版本和 artifact URL 正确。该步骤验证 GitHub 公共分发链路，不替代第 8 步的安装测试。

GitHub 仓库必须配置：

- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

`GITHUB_TOKEN` 使用 GitHub Actions 自动提供的令牌，workflow 权限缩小为完成发布所需的 `contents: write`。发布 Action 固定到经过审核的完整 commit SHA，不使用可漂移的分支引用；签名 Secrets 不得暴露给 pull request workflow。仓库规则应限制 `v*` tag 的创建权限。

签名私钥、密码和后续的 Authenticode 证书不得写入仓库、构建日志或发行说明。

### 8.1 Updater 签名密钥保管

- updater 私钥只生成一次，GitHub Actions Secret 保存用于发布的副本，另在独立位置保存一份加密离线备份。
- 私钥密码与私钥备份分开保管，并限制可读取人员。
- 首次正式发布前执行一次恢复演练：从离线备份恢复到临时环境，对测试 artifact 签名，并用内置公钥验证。
- 丢失当前私钥意味着已安装客户端不能验证后续更新，不能依赖“重新生成一对密钥”恢复。
- 单一 GitHub `latest.json` 不能安全覆盖错过过渡版本的旧公钥客户端，因此第一阶段不做日常密钥轮换。若必须轮换，应先设计动态更新 endpoint，按客户端当前版本提供由旧密钥签署的桥接版本；否则必须明确停止支持仍信任旧公钥的客户端。

## 9. 错误处理

- GitHub 不可达、超时或 `latest.json` 请求失败：本次自动检查静默结束，设置页显示最近失败状态。
- 元数据格式错误或更新包签名不匹配：拒绝安装，并显示“更新包验证失败”。
- 下载中断：继续运行当前版本，允许重试或打开 Release 页面手动下载。
- 本地代理无法安全停止：不启动安装器，避免强制中断请求。
- 安装器启动失败：尝试恢复本地代理，继续运行当前版本，并提供手动下载入口。
- 发行说明为空或无法解析：仍允许更新，界面显示简短的默认说明。

更新失败不能清除用户数据库、凭据或配置，也不能让应用停留在虚假的“正在安装”状态。

## 10. 数据兼容和回滚

自动更新不提供应用内降级。若某版本存在问题，应发布更高版本修复，例如用 `0.2.1` 修复 `0.2.0`。

数据库迁移必须遵循向前兼容原则，并在修改前生成可恢复的本地备份。已经运行新版本迁移的数据不得假设旧版本仍能读取。涉及不可逆迁移的 Release 必须在发行说明中明确标注，并通过旧版本到新版本的升级测试。

## 11. 测试策略

### 单元测试

- 版本比较和无更新判断。
- 更新状态转换和并发检查保护。
- 发行说明缺失、元数据错误和下载错误映射。
- 下载进度和大小格式化。

### 前端测试

- 启动自动检查只触发一次。
- 无更新时不显示对话框。
- 有更新时正确展示版本和发行说明。
- 用户确认后进入下载状态，失败后可以重试。
- “稍后更新”在本次运行内抑制重复提醒。
- 浏览器/Vite 环境不会误调用 Tauri updater。

### Rust 测试

- 空闲代理从 `running` 经 `draining` 进入 `stopped`。
- 进入 `draining` 后，新请求收到可重试的 `503`，不会增加活动请求数。
- 有在途请求时保留活动请求计数并等待排空。
- 排空超时返回明确错误、不进入安装，并恢复到可接受请求的 `running` 状态。
- listener 停止失败时返回明确错误；安装启动失败时能够尝试重新启动代理。

### 发布冒烟测试

- 在干净 Windows x86_64 环境安装使用测试更新配置的旧版本。
- 启动本地代理并产生一次受控测试请求。
- 使用 Draft 的真实签名 artifact 和本地静态 `latest.json` fixture，验证检查、确认、下载和签名校验。
- 使用被篡改的 artifact 验证签名失败时拒绝安装。
- 验证代理进入 draining、拒绝新请求、安全停止、安装器执行和应用重启。
- 验证数据库、凭据和设置在升级后保持完整。
- 验证 updater 失败时旧版本仍可继续使用。
- 正式发布后，从上一稳定版本验证 GitHub public latest endpoint 返回本次版本和可下载 artifact。

## 12. 完成标准

- Windows x86_64 NSIS current-user 安装包可以正常安装和启动，updater 使用 passive 安装模式。
- `0.1.0` 作为需手动安装的 updater 引导基线，自动更新验收至少覆盖 `0.1.0 -> 0.1.1`。
- `vX.Y.Z` tag 能稳定生成 Draft GitHub Release、签名产物和 `latest.json`。
- Draft 的真实签名 artifact 能通过本地静态更新源完成发布前端到端升级测试。
- 已安装旧版本能够检测到正式发布的新版本。
- 更新包签名验证失败时绝不安装。
- 用户确认后能够查看下载进度并完成安装重启。
- 安装前能够将本地代理从 `running` 排空到 `stopped`；超时或安装失败时恢复服务并保留当前版本。
- 更新检查和下载失败不影响应用及本地代理继续工作。
- 设置页能够显示当前版本、最近检查结果并手动重试。
- package.json、Cargo package、Tauri 解析版本与 Git tag 的一致性由 CI 强制验证。
- Tauri capability 只开放 updater 默认工作流和 process restart 所需权限。
- updater 私钥存在经过恢复演练的离线加密备份。

## 13. 参考资料

- [Tauri Updater](https://v2.tauri.app/plugin/updater/)
- [Tauri GitHub Actions Pipeline](https://v2.tauri.app/distribute/pipelines/github/)
