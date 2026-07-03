# Relay Pool Desktop

Relay Pool Desktop 是一个本地桌面端 AI 中转站与 Key 池管理工具。它不是网站，不是 SaaS，不是中转站后台，也不是 CCSwitch 的升级版或替代品。它和 CCSwitch 的关系是配合关系：

- CCSwitch 负责管理本机 AI 工具配置；
- Relay Pool Desktop 负责管理多个真实中转站，并对外暴露一个固定的本地 API 入口；
- Codex、Claude Code、Gemini CLI、CCSwitch 等工具只需要连接 Relay Pool Desktop 的本地入口；
- 背后的中转站账号管理、余额监控、倍率采集、Key 池排序、低价路由、失败切换由 Relay Pool Desktop 完成。

一句话定义：

> Relay Pool Desktop 是一个本地 AI 中转站与 Key 池调度器：对外提供固定 OpenAI-compatible 入口，对内管理多个 Sub2API / NewAPI / OpenAI-compatible 中转站账号及其下的 API Key，自动采集余额和倍率，并根据 Key 池优先级、模型能力、协议能力、健康状态和价格 / 余额策略进行本地路由。

当前状态：

- Phase 2 / 2.5 已完成本地 SQLite 数据层和 UI reset；
- Phase 3 / 3.1 已完成信息采集原型和站点账号 / 多 key 数据模型；
- P4 / P4.1 已完成登录态信息采集主线和 Key 池 MVP；
- P5 已完成本地 OpenAI-compatible 网关主干；
- P6 已完成模型 / 协议 / 健康路由层；
- P7 已完成价格归一化、余额快照、请求成本和 cheap_first 路由展示。
- P8 正在推进安全与凭据治理：本地加密凭据、统一脱敏、日志 / 快照安全边界和代理暴露面复核。

## 第一版目标

- 本地 Tauri 桌面 App；
- 对外提供固定 OpenAI-compatible 本地入口；
- 支持添加多个中转站；
- 优先支持 Sub2API / Sub2API 魔改站采集；
- 支持余额监控、倍率采集、Key 池路由、健康检测、失败 fallback 和请求日志；
- 支持一键复制 CCSwitch provider 配置。

## 开发命令

```bash
pnpm install
pnpm dev
pnpm build
pnpm tauri:dev
```

## 当前骨架

- React + TypeScript + Vite 前端；
- Tailwind CSS 样式入口；
- `src/components/ui` 作为 shadcn/ui 组件目录；
- Tauri 2 Rust 端目录已建立；
- AppShell 已包含左侧导航、顶部状态栏和七个页面入口；
- Phase 1 已将页面升级为浅色真实感假数据 UI；
- Phase 2 已接入本地 SQLite，持久化中转站账号和部分本地设置；
- Phase 2.5 已完成 Sub2API 式柔和卡片控制台 + CCSwitch 式本地桌面导航；
- Phase 3 引入“站点账号”模型、一个站点下多把 API Key、登录账号字段和非登录态探测 / 采集快照原型；
- P4 / P4.1 将信息采集主线修正为登录态信息采集，并把站点账号、Station Key、Key 池和渠道状态职责拆开；
- P5 已具备本地 OpenAI-compatible 网关骨架，按 Key 池优先级 fallback，聚合模型列表，并支持非流式与 SSE 流式透传；
- P6 已具备模型 / 协议 / 健康感知路由，Key 池可配置能力范围，路由规则页可模拟选择结果，请求日志会记录路由解释；
- P7 已完成价格 / 余额 / 成本层。
- P8 聚焦安全与凭据治理，避免真实 API key、站点密码、token / cookie、prompt / response 和本地日志在数据库、UI 或导入导出路径中泄露。

Security note: default exports and logs are metadata-only. Real keys, passwords, tokens, cookies, prompts, and responses are excluded from default export paths.

## 项目边界

- 不加入账号、支付、云同步或多用户系统；
- 不提交 key、cookie、日志、用户本地数据库或本地配置；
- 不在日志里打印完整 API key；
- 不把项目做成网站或 SaaS。
