# Relay Pool Desktop

Relay Pool Desktop 是一个本地桌面端 AI 中转池管理工具。它不是网站，也不是 SaaS；目标是在本机提供固定 OpenAI-compatible 入口，并在本地管理多个 Sub2API / NewAPI / OpenAI-compatible 中转站。

当前状态：early development。项目刚进入骨架阶段，尚未实现真实代理、采集、路由、数据库或请求日志。

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
- AppShell 已包含左侧导航、顶部状态栏和七个占位页面。

## 项目边界

- 不加入账号、支付、云同步或多用户系统。
- 不提交 key、cookie、日志、用户本地数据库或本地配置。
- 具体业务逻辑会在后续阶段按模块逐步实现。
