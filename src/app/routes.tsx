import {
  Activity,
  BarChart3,
  ClipboardList,
  DatabaseZap,
  FlaskConical,
  GitBranch,
  LayoutDashboard,
  KeyRound,
  Radio,
  Settings,
} from "lucide-react";
import type { AppRoute } from "@/lib/types/navigation";

export const appRoutes: AppRoute[] = [
  {
    id: "dashboard",
    label: "总览",
    description: "当前风险、本地代理和关键运行摘要",
    icon: LayoutDashboard,
  },
  {
    id: "stations",
    label: "中转站资产",
    description: "站点资产、余额、倍率、采集和路由参与状态",
    icon: DatabaseZap,
  },
  {
    id: "keyPool",
    label: "密钥池",
    description: "所有密钥的可用性和优先级",
    icon: KeyRound,
  },
  {
    id: "routing",
    label: "路由规则",
    description: "默认策略、模型映射和选择解释",
    icon: GitBranch,
  },
  {
    id: "pricing",
    label: "价格 / 倍率",
    description: "跨站点模型价格、分组倍率和可用性对比",
    icon: BarChart3,
  },
  {
    id: "channels",
    label: "渠道状态",
    description: "密钥延迟、成功率和最近状态",
    icon: Radio,
  },
  {
    id: "collectors",
    label: "采集中心",
    description: "高级工具中调试采集、登录态和快照识别",
    icon: FlaskConical,
  },
  {
    id: "changes",
    label: "变更中心",
    description: "余额、密钥、采集、价格、倍率、模型和路由变化",
    icon: Activity,
  },
  {
    id: "logs",
    label: "使用记录",
    description: "请求、耗时、成本和重试轨迹",
    icon: ClipboardList,
  },
  {
    id: "settings",
    label: "设置",
    description: "本地代理、安全和数据目录",
    icon: Settings,
  },
];
