# Relay Pool Desktop

> A local desktop control plane for AI relay services, API keys, and OpenAI-compatible routing.

**技术预览版**：核心管理与本地路由流程已经可以运行，但接口、数据结构、兼容范围和安装方式仍可能变化。目前仓库尚未提供稳定安装包，请从源码运行。

Relay Pool Desktop 是一款面向个人开发者的本地桌面工具。它将多个 AI 中转站账号和 API Key 汇集到一个可观察、可控制的 Key 池中，并向 Codex、Claude Code、Gemini CLI、CCSwitch 等本地客户端提供固定的 OpenAI-compatible 入口。

你无需在每个客户端里反复更换上游地址和 Key。Relay Pool Desktop 在本机负责采集站点信息、筛选可用 Key、执行路由与失败切换，并记录足以排查问题的请求元数据。

## 它如何工作

```text
Codex / Claude Code / Gemini CLI / CCSwitch
                       |
                       v
          Relay Pool Desktop (localhost)
           |       |        |        |
        能力匹配  健康状态  优先级  价格事实
                       |
                       v
        Sub2API / NewAPI / OpenAI-compatible
```

Relay Pool Desktop 不是云端中转服务，也不会替代 CCSwitch：CCSwitch 可以继续负责本机 AI 工具配置，Relay Pool Desktop 则负责它背后的中转站资产、Key 池和本地请求路由。

## 核心能力

### 中转站资产与 Key 池

- 在本地管理多个中转站账号、Base URL 和 Station Key。
- 查看 Key 的启用状态、优先级、模型范围、协议能力、余额与健康信息。
- 扫描和同步受支持站点的远端 Key，并在适配器支持时创建远端 Key。
- 以 Station 管理账号与采集，以 Station Key 作为实际请求路由单元。

### 信息采集与价格比较

- 采集余额、分组、倍率、模型和账号状态，并保留采集任务与快照信息。
- 将不同来源的价格、倍率和余额整理为可供界面与路由使用的本地事实。
- 按模型系列查看跨站点分组倍率，识别当前可比较的更低成本选项。
- 通过变更中心跟踪余额、Key、采集、价格、倍率、模型和路由影响变化。

### 本地 OpenAI-compatible 网关

- 在 `localhost` 暴露固定的本地 API 入口。
- 支持 `GET /v1/models`、`POST /v1/chat/completions`、`POST /v1/responses` 和本地 usage 查询。
- 支持 Chat Completions 与 Responses 的非流式请求和 SSE 流式透传。
- 根据模型与协议能力、健康状态、冷却状态、优先级和价格事实筛选候选 Key。
- 在可重试的鉴权、限流或上游故障场景中执行 fallback，并记录选择解释。

### 运行状态与可观察性

- 在总览页查看本地代理、请求、失败率、余额和成本摘要。
- 查看每个渠道的成功率、延迟、冷却状态和近期请求结果。
- 配置不会消耗过多 token 的定时渠道探测。
- 在请求日志中查看路由结果、耗时、usage、估算成本、fallback 和错误摘要。

### 本地数据与凭据保护

- 使用 SQLite 在本机保存站点、Key 元数据、采集结果、路由设置和请求元数据。
- 使用系统凭据库保护数据密钥，并通过 AES-GCM 加密本地敏感字段。
- 对界面、错误、采集快照和日志中的 Key、Cookie、Token 等敏感值进行脱敏。
- 默认导出不包含完整 Key、密码、Cookie、会话、Prompt、Response 或密文。

## 支持范围

当前优先适配 **Sub2API** 及其常见变体，同时提供分层的 **NewAPI** 与通用 **OpenAI-compatible** 支持。不同站点的接口路径、鉴权方式和字段结构并不统一，实际可用的余额、倍率、模型或远端 Key 能力取决于对应适配器与站点实现。

本项目当前不包含：

- 账号、支付、团队权限或多用户后台；
- 云同步或托管式代理服务；
- 对所有中转站魔改版本的兼容承诺；
- 可直接替代 CCSwitch 的客户端配置管理体系。

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

项目仍处于技术预览阶段，Tauri 分发打包尚未作为稳定发布渠道启用。

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
  services/            SQLite、采集、路由、代理、监控与凭据服务
docs/                   产品模型、阶段设计、安全策略与研究资料
scripts/                聚焦业务契约的回归检查脚本
```

主要技术栈：

- [Tauri 2](https://tauri.app/) + Rust
- React 18 + TypeScript + Vite
- Tailwind CSS
- SQLite（`rusqlite`）
- Windows Credential Manager + AES-GCM

## 安全说明

Relay Pool Desktop 会在本机处理真实上游凭据。请勿提交 API Key、密码、Cookie、Token、本地数据库、日志或配置文件，也不要在 Issue 和截图中暴露这些信息。

默认日志和导出以元数据为主，不记录 Prompt 与 Response 正文。数据库备份可能包含加密后的凭据密文，并依赖原系统凭据库中的数据密钥；它不等同于可跨设备恢复的加密导出。

详细边界见 [Security Export and Import Policy](docs/SECURITY_EXPORT_IMPORT.md)。

## 路线图

当前工作重点是提高真实站点采集的兼容性与恢复能力、完善路由事实层和可观察性，并建立可验证的 Windows 更新与发布流程。稳定安装包、完整兼容矩阵和迁移体验将在发布流程成熟后提供。

详细规划与领域术语：

- [项目规划](docs/PROJECT_PLAN.md)
- [产品模型](docs/PRODUCT_MODEL.md)
- [本地代理设计](docs/PHASE_5_LOCAL_PROXY_PLAN.md)
- [路由策略设计](docs/PHASE_6_ROUTING_POLICY_PLAN.md)
- [安全与凭据治理](docs/PHASE_8_SECURITY_CREDENTIAL_GOVERNANCE_PLAN.md)

## 参与项目

欢迎通过 [Issues](https://github.com/hardyz0517/relay-pool-desktop/issues) 报告可复现的问题或讨论兼容需求，也欢迎提交范围清晰的 Pull Request。涉及中转站适配时，请提供脱敏后的请求路径、状态码和响应结构，不要附带真实凭据或用户数据。

仓库当前尚未添加开源许可证。在许可证明确之前，源码默认保留全部权利，不应视为已获得复制、分发或衍生使用授权。
