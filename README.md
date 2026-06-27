# Relay Pool Desktop

Relay Pool Desktop 是一个本地桌面端 AI 中转站与 Key 池管理工具。它不是网站，也不是 SaaS；目标是在本机提供固定 OpenAI-compatible 入口，并在本地管理多个 Sub2API / NewAPI / OpenAI-compatible 中转站账号与它们下面的可路由 API Key。

当前状态：early development。Phase 2 已完成本地 SQLite 数据层和 stations/settings 持久化；Phase 2.5 已完成柔和桌面控制台 UI reset；Phase 3 已引入 station account、多 API key 数据模型和 collector prototype；P3.1 已将采集页调整为“信息采集”控制台，并开始抽象通用 Collector Adapter；P4 已把主线修正为登录态信息采集；P4.1 已收口产品模型并补齐 Key 池 MVP；P5.0 已落地本地 OpenAI-compatible 代理核心，支持本机监听、Key 池 fallback、请求日志和基础 `/v1/responses` 兼容。后续继续补流式、协议兼容和更细的路由策略。

## 第一版目标

- 本地 Tauri 桌面 App。
- 对外提供固定 OpenAI-compatible 本地入口。
- 支持添加多个中转站。
- 优先支持 Sub2API / Sub2API 魔改站采集。
- 支持余额监控、倍率采集、低价路由、健康检测、失败 fallback 和请求日志。
- 支持一键复制 CCSwitch provider 配置。

## 开发命令

```bash
pnpm install
pnpm dev
pnpm build
pnpm tauri:dev
```

## 当前骨架

- React + TypeScript + Vite 前端。
- Tailwind CSS 样式入口。
- 预留 `src/components/ui` 作为 shadcn/ui 组件目录。
- Tauri 2 Rust 端目录已建立。
- AppShell 已包含左侧导航、顶部状态栏和七个页面入口。
- Phase 1 已将页面升级为浅色真实感假数据 UI。
- Phase 2 已接入本地 SQLite，持久化中转站账号和部分本地设置。
- Phase 2.5 已完成 Sub2API 式柔和卡片控制台 + CCSwitch 式本地桌面导航。
- Phase 3 引入“站点账号”模型、一个站点下多把 API Key、登录账号字段和 Sub2API 非登录态探测 / 采集快照原型。
- P3.1 将“Sub2API 采集”改为“信息采集”，主界面展示采集结论、识别结果、接口探测结果和历史快照，脱敏 raw snapshot 默认收进开发者详情。
- P4 / P4.1 将信息采集主线修正为登录态信息采集，并把站点账号、Station Key、Key 池和渠道状态职责拆开。
- P5 将开始本地 OpenAI-compatible 代理 MVP，先按 Key 池优先级 fallback，再逐步扩展流式、价格和健康策略。
- 第一版 UI 方向为参考 CCSwitch 的浅色、简约、克制、紧凑桌面工具风格；深色主题仅作为后续可选项预留。

## 项目边界

- 不加入账号、支付、云同步或多用户系统。
- 不提交 key、cookie、日志、用户本地数据库或本地配置。
- 具体 proxy、路由、价格归一化和 WebView 登录捕获会在后续阶段按模块逐步补强；当前已具备本地代理核心和 request log 落库。
