# Relay Pool Desktop 开发规则

## 项目定位

- Relay Pool Desktop 是本地桌面工具，不是网站、SaaS、中转站后台或营销页。
- 技术栈以 Tauri 2 + React + TypeScript + Vite + Tailwind CSS 为主。
- 对外目标是固定 OpenAI-compatible 本地入口；对内目标是管理多个 Sub2API / NewAPI / OpenAI-compatible 中转站。
- 当前阶段优先做清晰骨架和可验证的小步迭代，不提前实现复杂业务逻辑。

## UI 方向

- 第一版 UI 默认采用浅色简约主题，参考 CCSwitch 的白色桌面工具感。
- 默认不要做深色主题，不要做 VS Code 暗色风。
- 不做传统网站首页、SaaS 控制台、企业后台模板、营销落地页或过度装饰的界面。
- 控件保持克制、紧凑、可扫描；表格和状态徽标优先服务日常使用。
- UI 改动应保持本地工具感：浅灰窗口背景、白色或近白色面板、细边框、低饱和状态色和高信息密度。

## 功能边界

- 不加入账号系统、支付系统、团队权限、云同步、插件市场。
- 不把项目做成完整替代 CCSwitch；它应当与 CCSwitch 配合使用。
- 不直接复制 AGPL / LGPL 项目的核心实现；参考项目需要保留边界意识和必要 attribution。
- 后续实现代理、采集、路由、健康检测、日志前，先阅读 `docs/PROJECT_PLAN.md` 和相关模块文件。

## 数据与安全

- 不提交 API key、cookie、token、日志、用户本地数据库或本地配置。
- 不在日志、错误信息或截图中暴露完整 key / cookie。
- 本地数据库、缓存、日志和 `.env` 文件必须留在 `.gitignore` 中。

## Git 与交付

- 不要使用 `git add .` 或 `git add -A`。
- 只按任务范围 stage 明确路径。
- 修改前先查看当前工作区，避免覆盖用户已有改动。
- 每次完成后说明：改了什么、如何启动、如何验证、还有哪些未完成。

## 验证要求

- 前端改动至少运行可用的 TypeScript / Vite 检查。
- Tauri / Rust 改动至少运行可用的 Cargo 检查。
- 如果某项检查无法运行，必须说明实际原因，不要假装通过。
