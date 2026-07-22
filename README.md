# Relay Pool Desktop

<p align='center'>
  <strong>本地 AI 中转资产管理与 OpenAI-compatible 路由网关</strong>
  <br />
  <span>A local desktop control plane for relay stations, station keys, pricing facts, health signals, and localhost routing.</span>
</p>

<p align='center'>
  <a href='https://github.com/hardyz0517/relay-pool-desktop/releases/latest'><img alt='Release' src='https://img.shields.io/github/v/release/hardyz0517/relay-pool-desktop?label=release&color=2563eb' /></a>
  <img alt='Version' src='https://img.shields.io/badge/version-v0.3.2-0f766e' />
  <img alt='Preview' src='https://img.shields.io/badge/status-technical%20preview-f59e0b' />
  <img alt='Platform' src='https://img.shields.io/badge/platform-Windows%20x64-111827' />
  <img alt='Tauri' src='https://img.shields.io/badge/Tauri-2.x-24c8db' />
  <img alt='React' src='https://img.shields.io/badge/React-18-61dafb' />
  <img alt='Rust' src='https://img.shields.io/badge/Rust-native-b7410e' />
</p>

<p align='center'>
  <a href='https://github.com/hardyz0517/relay-pool-desktop/releases/latest'><strong>下载最新版</strong></a>
  ·
  <a href='docs/PROJECT_PLAN.md'>项目规划</a>
  ·
  <a href='docs/PRODUCT_MODEL.md'>产品模型</a>
  ·
  <a href='https://qm.qq.com/q/G1bJsrIbOG'>QQ 交流群</a>
</p>

---

**当前版本：v0.3.2（技术预览）**。核心管理、本地代理、路由、采集、请求日志、Windows 预览安装包和应用内更新检查已经进入可运行状态；接口、数据结构、兼容范围和安装方式仍可能变化。请在真实凭据环境中谨慎升级，并先保留必要备份。

Relay Pool Desktop 是一款面向个人开发者和本地 AI 工具用户的桌面控制台。它把多个 Sub2API / NewAPI / OpenAI-compatible 中转站账号、站内 Key、余额、分组倍率、模型能力、健康状态和请求记录收束到本机，并向 Codex、Claude Code、Gemini CLI、CCSwitch 等客户端提供固定的 OpenAI-compatible 入口。

你无需在每个客户端里反复更换上游地址和 Key。Relay Pool Desktop 在本机负责采集站点事实、筛选可用 Key、执行路由与失败切换，并留下足够解释一次请求为什么这样走的运行记录。

## 为什么需要它

| 日常痛点 | Relay Pool Desktop 的处理方式 |
| --- | --- |
| 多个中转站、多个 Key 分散在不同后台 | 用 Station / Station Key 模型统一管理站点账号、API Base URL 和实际路由 Key |
| 客户端频繁改 Provider、Base URL 和 Key | 对外固定暴露 localhost OpenAI-compatible 入口，客户端配置保持稳定 |
| 不知道哪把 Key 还能用、为什么失败 | 汇总健康、冷却、请求结果、fallback 和错误摘要 |
| 只知道倍率，不知道路由时如何使用 | 将分组、倍率、模型能力、余额和价格整理为可被界面与路由消费的本地事实 |
| 上游故障时排查成本高 | 请求日志记录候选筛选、最终选择、耗时、usage、估算成本和失败原因 |

## 它如何工作

```text
Codex / Claude Code / Gemini CLI / CCSwitch
                       |
                       v
            http://127.0.0.1:<local-port>/v1
                       |
                       v
          Relay Pool Desktop (Tauri + Rust)
           |       |        |        |
        能力匹配  健康状态  优先级  价格事实
                       |
                       v
        Sub2API / NewAPI / OpenAI-compatible
```

Relay Pool Desktop 不是云端中转服务，也不是 CCSwitch 的替代品。CCSwitch 可以继续负责本机 AI 工具配置；Relay Pool Desktop 则负责它背后的中转站资产、Station Key、采集事实和本地路由决策。

## 核心能力

| 模块 | 已覆盖能力 |
| --- | --- |
| 中转站资产 | 管理多个站点账号、前端网址、API Base URL、登录状态、余额来源、分组来源和采集状态 |
| Station Key 池 | 查看启用状态、优先级、模型范围、协议能力、余额、健康、备用状态和远端同步结果 |
| 信息采集 | 采集余额、分组、倍率、模型、账号状态和 collector run / snapshot 信息 |
| 价格与倍率 | 按模型系列比较跨站点分组倍率，并维护模型基准价格与归一化价格事实 |
| 本地网关 | 支持 `GET /v1/models`、`POST /v1/chat/completions`、`POST /v1/responses` 和 usage 查询 |
| 路由策略 | 根据模型、协议能力、健康、冷却、优先级、余额和价格事实筛选候选 Key |
| 可观察性 | 总览、渠道状态、请求日志、变更中心、路由模拟和失败摘要 |
| 本地安全 | SQLite 本地存储、系统凭据库保护数据密钥、AES-GCM 加密敏感字段、界面与日志脱敏 |

## Relay Pool 的侧重点

[CCSwitch](https://github.com/farion1231/cc-switch) 主要管理本机 AI 工具、Provider 配置和通用代理；[Cockpit Tools](https://github.com/jlcodes99/cockpit-tools) 主要管理官方 AI IDE 账号、订阅配额和多开实例。Relay Pool Desktop 不追求覆盖更多客户端，而是继续向上游深入，管理真实中转站账号、站内 Key 及其运行时路由。

| 工具 | 主要管理对象 | 更适合解决的问题 |
| --- | --- | --- |
| CCSwitch | AI 工具与 Provider 配置 | 跨客户端配置、切换和通用代理 |
| Cockpit Tools | 官方 IDE 账号与应用实例 | 账号切换、配额监控和多实例运行 |
| Relay Pool Desktop | 中转站账号与 Station Key | 站点采集、Key 池路由、价格与健康决策 |

Relay Pool Desktop 的优势不在于替代这些工具，而在于补齐它们背后的中转资产控制层：

- **从站点账号深入到实际路由 Key**：区分负责登录和采集的 Station 与真正参与请求转发的 Station Key，并在适配器支持时扫描、同步或创建远端 Key。
- **让采集事实直接参与运行时决策**：将余额、分组、倍率、模型能力、健康和价格整理为本地事实，再用于候选筛选、排序、冷却和 fallback，而不是只保存一组静态 URL 与 Key。
- **同时考虑成本与可用性**：按模型与协议能力、健康状态、优先级、余额和价格事实选择候选，并通过跨站点分组倍率比较识别更合适的上游。
- **提供从变化到请求的解释链**：通过采集记录、变更中心、路由模拟和请求日志说明数据何时变化、某把 Key 为什么被选择或拒绝，以及 fallback 后最终走向哪里。
- **保留现有客户端工作流**：Codex、Claude Code、Gemini CLI 或 CCSwitch 只需连接固定的本地入口；后续调整中转站和 Key 池时，无需反复修改每个客户端的上游配置。

## 支持范围

当前优先适配 **Sub2API** 及其常见变体，同时提供分层的 **NewAPI** 与通用 **OpenAI-compatible** 支持。不同站点的接口路径、鉴权方式和字段结构并不统一，实际可用的余额、倍率、模型或远端 Key 能力取决于对应适配器与站点实现。

本项目当前不包含：

- 账号、支付、团队权限或多用户后台；
- 云同步或托管式代理服务；
- 对所有中转站魔改版本的兼容承诺；
- 可直接替代 CCSwitch 的客户端配置管理体系。

## 下载安装

Windows 用户可以从 [GitHub Releases](https://github.com/hardyz0517/relay-pool-desktop/releases/latest) 下载最新版 NSIS 安装包；已安装版本也可以在应用内设置页手动检查更新，更新元数据来自公开的 `latest.json`。

当前发布通道仍是技术预览：

- 主要验证平台是 Windows x86_64；
- 安装范围为当前用户，不需要管理员权限；
- 准备安装更新时会协调停止本地代理，再安装并重启应用；
- macOS、Linux、Windows ARM64、强制静默更新、增量更新和多发布通道暂未支持；
- 预览安装包可能仍触发系统安全提示，正式代码签名与更完整的兼容矩阵会在后续版本继续补齐。

## 从源码运行

### 环境要求

- Windows 10/11（当前主要开发与验证平台）；
- [Node.js](https://nodejs.org/) 20 或更高版本；
- [pnpm](https://pnpm.io/) 11；
- [Rust](https://www.rust-lang.org/tools/install) stable toolchain；
- [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/)（Windows 需要 WebView2 与 Microsoft C++ Build Tools）。

### 启动桌面应用

```powershell
git clone https://github.com/hardyz0517/relay-pool-desktop.git
cd relay-pool-desktop
pnpm install
pnpm tauri:dev
```

`pnpm dev` 只启动 Vite 前端，适合界面开发；涉及 SQLite、采集、凭据或本地代理时，应使用 `pnpm tauri:dev` 启动完整桌面应用。

### 构建检查

```powershell
# TypeScript 类型检查与 Vite 构建
pnpm build

# Tauri 应用构建
pnpm tauri:build
```

项目仍处于技术预览阶段。发布版本通过 tag 触发的 GitHub Actions 构建 Windows NSIS 安装包、updater artifact 和 `latest.json`；本地构建主要用于开发验证，不等同于正式发布流程。

## 项目结构

```text
src/
  app/                 应用入口与页面路由
  components/          桌面 Shell 与通用 UI
  features/            中转站、Key 池、路由、价格、渠道、日志等功能
  lib/                 前端 API、类型、查询与视图投影
src-tauri/src/
  commands/            Tauri 命令边界
  models/              Rust 领域模型
  application/         持久化用例与应用服务
  persistence/         SQLx Store、migration、备份与恢复
  services/            采集、路由、代理、监控与凭据服务
docs/                   产品模型、阶段设计、安全策略与研究资料
scripts/                聚焦业务契约的回归检查脚本
```

主要技术栈：

- [Tauri 2](https://tauri.app/) + Rust
- React 18 + TypeScript + Vite
- Tailwind CSS
- SQLite（`SQLx`）
- Windows Credential Manager + AES-GCM

## 安全说明

Relay Pool Desktop 会在本机处理真实上游凭据。请勿提交 API Key、密码、Cookie、Token、本地数据库、日志或配置文件，也不要在 Issue 和截图中暴露这些信息。

默认日志和导出以元数据为主，不记录 Prompt 与 Response 正文。数据库备份可能包含加密后的凭据密文，并依赖原系统凭据库中的数据密钥；它不等同于可跨设备恢复的加密导出。

详细边界见 [Security Export and Import Policy](docs/SECURITY_EXPORT_IMPORT.md)。

## 路线图

当前工作重点是提高真实站点采集的兼容性与恢复能力、完善路由事实层和可观察性，并继续打磨 v0.3.x Windows 预览发布与自动更新流程。更完整的兼容矩阵、迁移体验和正式分发体验将在发布流程继续成熟后提供。

详细规划与领域术语：

- [项目规划](docs/PROJECT_PLAN.md)
- [产品模型](docs/PRODUCT_MODEL.md)
- [本地代理设计](docs/PHASE_5_LOCAL_PROXY_PLAN.md)
- [路由策略设计](docs/PHASE_6_ROUTING_POLICY_PLAN.md)
- [安全与凭据治理](docs/PHASE_8_SECURITY_CREDENTIAL_GOVERNANCE_PLAN.md)

## 参与项目

欢迎通过 [Issues](https://github.com/hardyz0517/relay-pool-desktop/issues) 报告可复现的问题或讨论兼容需求，也欢迎提交范围清晰的 Pull Request。涉及中转站适配时，请提供脱敏后的请求路径、状态码和响应结构，不要附带真实凭据或用户数据。

仓库当前尚未添加开源许可证。在许可证明确之前，源码默认保留全部权利，不应视为已获得复制、分发或衍生使用授权。
