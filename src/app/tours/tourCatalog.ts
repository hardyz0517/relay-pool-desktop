import type { AppPageId } from "@/lib/types/navigation";
import type {
  PublishedTourId,
  TourDefinition,
  TourId,
  TourPreparationKey,
  TourStep,
} from "./tourTypes";

export const TOUR_REVISIONS = {
  full: 3,
  basic: 2,
  dashboard: 1,
  stations: 1,
  "key-pool": 1,
  routing: 1,
  pricing: 1,
  channels: 1,
  changes: 1,
  logs: 1,
  settings: 1,
  // Legacy ids remain parseable but are not published.
  proxy: 1,
  "station-setup": 1,
  monitoring: 1,
  advanced: 1,
} as const satisfies Record<TourId, number>;

function step(
  id: string,
  route: AppPageId,
  anchor: string,
  title: string,
  description: string,
  prepareKey: TourPreparationKey = "none",
): TourStep {
  return { id, route, target: { anchor }, title, description, prepareKey };
}

const fullSteps = [
  step("full-sidebar", "dashboard", "shell-sidebar", "主要工作区", "左侧导航连接资产、路由、状态、记录和设置，是浏览 Relay Pool 各项能力的主入口。"),
  step("full-dashboard-metrics", "dashboard", "dashboard-metrics", "本地运行概览", "这里汇总经过本地入口的请求、用量与可用性，帮助快速判断 Relay Pool 当前运行情况。"),
  step("full-dashboard-risk", "dashboard", "dashboard-risk", "优先关注风险", "风险区集中展示需要处理的站点、密钥和路由问题，可结合严重度与出现时间安排处理顺序。"),
  step("full-stations-list", "stations", "stations-list", "中转站资产", "中转站代表上游服务入口，承载账号、余额、分组和倍率等站点事实。"),
  step("full-stations-status", "stations", "stations-status-fields", "站点状态与风险", "列表中的余额、Key、采集和倍率分别反映不同风险来源，需要分开判断。"),
  step("full-key-pool-list", "keyPool", "key-pool-list", "中转站 Key", "Key 是实际访问上游并参与路由调度的凭据单元，一个中转站可以管理多个 Key。"),
  step("full-key-pool-state", "keyPool", "key-pool-list", "状态与调度资格", "列表中的启用、调度资格和监控结论含义不同；只有满足资格的 Key 才能成为路由候选。"),
  step("full-routing-status", "routing", "routing-status", "当前路由状态", "路由规则把本地请求连接到合格的上游候选，这里展示当前生效范围和总体状态。", "routing-status-tab"),
  step("full-routing-candidates", "routing", "routing-status", "候选密钥", "候选结果由资格、可靠性、速度、成本和偏好等多个因素共同决定。", "routing-status-tab"),
  step("full-pricing-comparison", "pricing", "pricing-comparison", "成本条件比较", "实际倍率用于比较不同中转站分组的成本条件，但需要结合 Key 可用性共同判断。"),
  step("full-channels-tabs", "channels", "channels-tabs", "三类状态来源", "本地状态、上游官方状态与主动探针来自不同事实来源，不能互相替代。"),
  step("full-channels-local", "channels", "channels-local-results", "本地状态结果", "这里观察本地监控得到的正常、降级、错误、跳过或无数据等结果。", "channels-local-tab"),
  step("full-changes-list", "changes", "changes-list", "变更时间线", "重要告警、恢复和信息类变化按时间集中呈现，便于持续关注运行变化。"),
  step("full-logs-list", "logs", "logs-list", "请求使用记录", "每次经过本地入口的请求都可在这里追溯结果、耗时、用量和成本信息。"),
  step("full-settings-proxy", "settings", "settings-local-proxy", "统一本地入口", "客户端通过固定的 OpenAI-compatible 本地入口接入 Relay Pool；教程不会启动或停止代理。"),
  step("full-settings-system-proxy", "settings", "settings-network", "建议使用系统代理", "建议将默认网络出口设为“系统代理”。许多中转站需要通过系统代理才能正常访问；如果直连失败，请优先检查这里。教程只作提醒，不会修改当前设置。"),
  step("full-settings-tutorial", "settings", "settings-tutorial-entry", "随时重看教程", "教程中心可以重新打开完整体验，也可以针对某个页面查看更详细的说明。"),
] as const;

const basicSteps = [
  step("basic-sidebar", "dashboard", "shell-sidebar", "主要工作区", "左侧导航对应资产、路由、状态、记录和设置；开发者模式能力不在本教程范围内。"),
  step("basic-metrics", "dashboard", "dashboard-metrics", "本地运行概览", "Relay Pool 对外提供固定本地入口，总览聚合请求、用量、余额和可用密钥。"),
  step("basic-stations", "dashboard", "nav-stations", "中转站资产", "中转站是上游服务入口，负责承载账号、余额、分组和倍率等站点事实。"),
  step("basic-key-pool", "dashboard", "nav-key-pool", "中转站 Key", "密钥池管理实际参与访问和调度的 Key；一个中转站可以拥有多个 Key。"),
  step("basic-routing", "dashboard", "nav-routing", "路由规则", "路由从合格 Key 中选择候选，把本地请求连接到合适的上游资产。"),
  step("basic-recent-usage", "dashboard", "dashboard-recent-usage", "使用结果", "近期使用可快速确认请求是否成功；完整筛选和请求详情位于使用记录。"),
  step("basic-settings", "dashboard", "nav-settings", "设置与教程", "设置中管理本地代理、网络出口、数据与备份，并可随时重新打开教程。"),
] as const;

const dashboardSteps = [
  step("dashboard-metrics", "dashboard", "dashboard-metrics", "本地路由指标", "请求、实际消耗、Token、可用密钥、平均耗时和实时流量反映本地入口的运行结果。"),
  step("dashboard-station-metrics", "dashboard", "dashboard-station-metrics", "中转站统计", "站点余额与站点侧请求、消耗来自上游资产事实，统计口径不等同于本地请求指标。"),
  step("dashboard-risk", "dashboard", "dashboard-risk", "风险提示", "这里聚合需要处理的问题，严重度和最近出现时间可以帮助决定处理顺序。"),
  step("dashboard-key-health", "dashboard", "dashboard-key-health", "密钥健康", "可用、冷却和禁用等状态会影响密钥能否继续参与路由。"),
  step("dashboard-routing-queue", "dashboard", "dashboard-routing-queue", "路由候选", "候选 Key 的并发和近期成功率用于观察当前承载情况，这里并不是手工排队列表。"),
  step("dashboard-recent-usage", "dashboard", "dashboard-recent-usage", "近期使用", "这里用于快速确认最新请求，完整筛选、生命周期和成本详情应进入使用记录。"),
] as const;

const stationsSteps = [
  step("stations-toolbar", "stations", "stations-toolbar", "筛选与新增入口", "问题筛选用于聚焦异常资产；新增按钮只是操作入口，教程不会自动打开表单。"),
  step("stations-summary", "stations", "stations-summary", "资产概况", "这里汇总站点数量与问题概况，帮助判断当前资产范围和需要关注的状态。"),
  step("stations-list", "stations", "stations-list", "中转站列表", "每一行代表一个上游中转站；没有资产时，同一区域会显示空状态。"),
  step("stations-status-fields", "stations", "stations-status-fields", "状态与风险", "列表中的禁用、无可用 Key、采集异常、余额和倍率问题属于不同层面，不能互相替代。"),
  step("stations-row-actions", "stations", "stations-list", "详情与维护入口", "有资产时可从列表进入详情和编辑；教程不会选择真实站点，也不会执行删除或充值。"),
] as const;

const keyPoolSteps = [
  step("key-pool-toolbar", "keyPool", "key-pool-toolbar", "过滤密钥", "可按中转站和启用状态缩小范围；新增 Key 只作为入口说明。"),
  step("key-pool-list", "keyPool", "key-pool-list", "密钥池", "每一项都是连接上游的独立凭据单元，Key 与中转站保持从属关系。"),
  step("key-pool-state-columns", "keyPool", "key-pool-list", "状态与调度", "列表中的启用状态与调度资格含义不同，启用的 Key 也可能暂时不能参与路由。"),
  step("key-pool-monitor-column", "keyPool", "key-pool-list", "监控结论", "列表中的未检测、检测中、正常、降级和错误用于观察连通性与健康，不等于站点官方状态。"),
  step("key-pool-provider-actions", "keyPool", "key-pool-list", "归属与维护", "有数据时，列表会显示中转站归属和维护入口；教程不会发起真实连通性测试。"),
] as const;

const routingSteps = [
  step("routing-tabs", "routing", "routing-tabs", "概览与设置", "概览用于观察当前生效结果，设置用于调整候选范围与评分偏好。"),
  step("routing-status-summary", "routing", "routing-status", "当前路由状态", "这里汇总倍率上限、分组筛选、候选状态和最近路由等当前生效信息。", "routing-status-tab"),
  step("routing-candidate-list", "routing", "routing-status", "候选密钥", "只有满足资格的 Key 才会进入候选，排序是多个因素共同作用的结果。", "routing-status-tab"),
  step("routing-candidate-factors", "routing", "routing-status", "评分因素", "可靠性、响应速度、成本和偏好是候选比较维度，不代表单一维度决定结果。", "routing-status-tab"),
  step("routing-policy-scope", "routing", "routing-policy-scope", "候选范围", "设置决定哪些分组和 Key 可以参与路由；教程只解释当前区域，不修改配置。", "routing-settings-tab"),
  step("routing-policy-profile", "routing", "routing-policy-scope", "路由偏好", "设置视图中的均衡、稳定、速度和成本等预设会调整比较权重，但不会形成绝对结果保证。", "routing-settings-tab"),
  step("routing-policy-save", "routing", "routing-policy-scope", "保存与生效", "策略修改后需要显式保存才会进入生效流程；教程只解释设置区域，不会点击保存。", "routing-settings-tab"),
] as const;

const pricingSteps = [
  step("pricing-summary", "pricing", "pricing-summary", "价格与倍率", "这个页面比较不同中转站分组的实际使用成本条件，不是账单或支付页面。"),
  step("pricing-filters", "pricing", "pricing-filters", "缩小比较范围", "可以按模型类型、中转站、密钥和监控结果筛选；教程不会改变现有筛选条件。"),
  step("pricing-comparison", "pricing", "pricing-comparison", "分组倍率比较", "原始倍率与兑换率共同形成实际倍率，用于比较不同分组的成本条件。"),
  step("pricing-monitor-result", "pricing", "pricing-comparison", "可用性依据", "比较结果中的价格低不代表一定可用，监控结果和是否存在合格 Key 会共同影响可比较性。"),
  step("pricing-result-state", "pricing", "pricing-comparison", "无法比较的原因", "无 Key、未监控、运行中、无法解析和暂不可用含义不同，不能全部归为失败。"),
] as const;

const channelsSteps = [
  step("channels-tabs", "channels", "channels-tabs", "三类状态视图", "本地状态、官方状态和探针管理来自不同事实来源，不能互相替代。"),
  step("channels-local-toolbar", "channels", "channels-local-toolbar", "本地观测范围", "时间范围、启用状态和结果筛选控制当前本地监控记录的观察范围。", "channels-local-tab"),
  step("channels-local-results", "channels", "channels-local-results", "本地状态结果", "正常、降级、错误、跳过和无数据表示本地监控得到的不同结果。", "channels-local-tab"),
  step("channels-official-summary", "channels", "channels-official-summary", "上游官方状态", "该视图来自中转站公开或已采集的官方 Monitor，不由本地主动探针计算。", "channels-official-tab"),
  step("channels-official-results", "channels", "channels-official-results", "采集与服务状态", "服务可用性和数据采集状态是两件事；过期或采集失败时可能保留上次有效结果。", "channels-official-tab"),
  step("channels-monitoring-list", "channels", "channels-monitoring-list", "探针配置", "这里管理本地主动检测对象、范围和结果；教程不会新建、运行或取消探针。", "channels-monitoring-tab"),
] as const;

const changesSteps = [
  step("changes-view-filter", "changes", "changes-view-filter", "查看范围", "全部、未读和活动视图含义不同；未读只表示阅读状态，不代表问题仍在发生。"),
  step("changes-severity-filter", "changes", "changes-severity-filter", "严重程度", "信息、警告和严重用于排列关注度，不等于同一个问题的生命周期状态。"),
  step("changes-list", "changes", "changes-list", "变更时间线", "告警、恢复和信息类变更按时间出现，没有符合条件的记录也是正常状态。"),
  step("changes-unread-actions", "changes", "changes-unread-actions", "未读与处理", "标记已读只改变阅读状态，不会解决业务问题；教程不会自动标记或清空。"),
  step("changes-settings-entry", "changes", "changes-settings-entry", "通知与策略入口", "这里进入告警条件和投递策略设置；教程不会打开或修改临时设置页。"),
] as const;

const logsSteps = [
  step("logs-display-controls", "logs", "logs-display-controls", "显示与刷新", "精简显示用于调整表格密度，刷新会重新读取记录；清空会删除本地记录，教程不会触发这些操作。"),
  step("logs-list", "logs", "logs-list", "请求记录", "每行代表经过本地入口的一次请求，空状态表示当前筛选范围内没有记录。"),
  step("logs-outcome-columns", "logs", "logs-list", "结果与耗时", "状态码、首字延迟和总耗时反映请求的不同阶段，不能只看一个数字判断问题。"),
  step("logs-cost-columns", "logs", "logs-list", "Token 与成本", "列表同时展示 Token、倍率和成本元数据；缺少相关元数据时，不应把请求理解为零成本。"),
] as const;

const settingsSteps = [
  step("settings-local-proxy", "settings", "settings-local-proxy", "本地代理入口", "客户端连接固定的 OpenAI-compatible 本地入口；教程不会启动、停止或复制敏感值。"),
  step("settings-network", "settings", "settings-network", "后台网络出口", "直连、系统代理和手动代理用于软件后台访问上游，不等同于本地服务监听地址。"),
  step("settings-pricing", "settings", "settings-pricing", "模型定价", "模型基础价格为成本换算提供基准，并可从这里进入独立页面维护。"),
  step("settings-data-backup", "settings", "settings-data-backup", "数据与备份", "这里管理数据目录、备份和迁移入口；教程不会选择目录、导入、导出或恢复数据。"),
  step("settings-theme", "settings", "settings-theme", "外观设置", "外观选项只影响本机界面显示，不改变代理、路由或上游配置。"),
  step("settings-tutorial-entry", "settings", "settings-tutorial-entry", "使用教程", "可以随时打开教程中心，重看单页教程或完整体验。"),
] as const;

export const TOUR_CATALOG = {
  full: { id: "full", category: "recommended", order: 1, title: "完整体验 Relay Pool", summary: "精选浏览全部普通工作区，建立完整的产品结构认知。", revision: TOUR_REVISIONS.full, estimatedMinutes: 5, steps: fullSteps },
  basic: { id: "basic", category: "recommended", order: 2, title: "快速认识 Relay Pool", summary: "了解本地入口、上游资产、路由和主要工作区。", revision: TOUR_REVISIONS.basic, estimatedMinutes: 2, steps: basicSteps },
  dashboard: { id: "dashboard", category: "page", order: 1, title: "总览", summary: "读懂本地指标、站点统计、风险、密钥健康和近期使用。", revision: TOUR_REVISIONS.dashboard, estimatedMinutes: 3, steps: dashboardSteps },
  stations: { id: "stations", category: "page", order: 2, title: "中转站资产", summary: "了解上游资产、站点状态、风险和维护入口。", revision: TOUR_REVISIONS.stations, estimatedMinutes: 2, steps: stationsSteps },
  "key-pool": { id: "key-pool", category: "page", order: 3, title: "密钥池", summary: "区分 Key 的启用状态、调度资格、监控结论和归属。", revision: TOUR_REVISIONS["key-pool"], estimatedMinutes: 2, steps: keyPoolSteps },
  routing: { id: "routing", category: "page", order: 4, title: "路由规则", summary: "理解候选范围、评分因素、路由偏好和保存生效。", revision: TOUR_REVISIONS.routing, estimatedMinutes: 4, steps: routingSteps },
  pricing: { id: "pricing", category: "page", order: 5, title: "价格 / 倍率", summary: "理解实际倍率、可用性依据和无法比较的不同状态。", revision: TOUR_REVISIONS.pricing, estimatedMinutes: 3, steps: pricingSteps },
  channels: { id: "channels", category: "page", order: 6, title: "渠道状态", summary: "区分本地状态、上游官方状态和主动探针。", revision: TOUR_REVISIONS.channels, estimatedMinutes: 3, steps: channelsSteps },
  changes: { id: "changes", category: "page", order: 7, title: "变更中心", summary: "查看告警、恢复、严重程度、阅读状态和通知入口。", revision: TOUR_REVISIONS.changes, estimatedMinutes: 2, steps: changesSteps },
  logs: { id: "logs", category: "page", order: 8, title: "使用记录", summary: "追溯请求结果、耗时、尝试次数、Token 和成本。", revision: TOUR_REVISIONS.logs, estimatedMinutes: 2, steps: logsSteps },
  settings: { id: "settings", category: "page", order: 9, title: "设置", summary: "了解本地代理、后台网络、定价、备份和教程入口。", revision: TOUR_REVISIONS.settings, estimatedMinutes: 3, steps: settingsSteps },
} as const satisfies Record<PublishedTourId, TourDefinition<PublishedTourId>>;

export const PUBLISHED_TOUR_IDS = Object.freeze(
  Object.values(TOUR_CATALOG)
    .sort((left, right) => {
      const categoryOrder = { recommended: 0, page: 1 } as const;
      return categoryOrder[left.category] - categoryOrder[right.category] || left.order - right.order;
    })
    .map((tour) => tour.id) as PublishedTourId[],
);

export const PUBLISHED_TOURS: readonly TourDefinition<PublishedTourId>[] = Object.freeze(
  PUBLISHED_TOUR_IDS.map((id) => TOUR_CATALOG[id]),
);

export function getTourDefinition(tourId: string): TourDefinition | undefined {
  return isPublishedTourId(tourId) ? TOUR_CATALOG[tourId] : undefined;
}

export function isPublishedTourId(tourId: string): tourId is PublishedTourId {
  return Object.prototype.hasOwnProperty.call(TOUR_CATALOG, tourId);
}
