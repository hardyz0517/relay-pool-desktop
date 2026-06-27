import {
  Activity,
  BarChart3,
  ClipboardList,
  DatabaseZap,
  GitBranch,
  LayoutDashboard,
  KeyRound,
  Settings,
  Radar,
} from "lucide-react";
import type { AppRoute } from "@/lib/types/navigation";

export const appRoutes: AppRoute[] = [
  {
    id: "dashboard",
    label: "总览",
    description: "本地入口、路由状态和最近活动",
    icon: LayoutDashboard,
  },
  {
    id: "stations",
    label: "中转站",
    description: "站点账号、登录信息和采集来源",
    icon: DatabaseZap,
  },
  {
    id: "keyPool",
    label: "Key 池",
    description: "所有站点 Key 的统一管理视图",
    icon: KeyRound,
  },
  {
    id: "channels",
    label: "渠道状态",
    description: "延迟、可用率和请求状态条",
    icon: Radar,
  },
  {
    id: "collectors",
    label: "信息采集",
    description: "账号、余额、分组、倍率和接口能力",
    icon: Activity,
  },
  {
    id: "pricing",
    label: "价格表",
    description: "模型价格归一化与站点对比",
    icon: BarChart3,
  },
  {
    id: "routing",
    label: "路由规则",
    description: "手动优先、最低价和失败切换",
    icon: GitBranch,
  },
  {
    id: "logs",
    label: "请求日志",
    description: "请求、耗时、成本和 fallback 轨迹",
    icon: ClipboardList,
  },
  {
    id: "settings",
    label: "设置",
    description: "本地代理、数据目录和外观",
    icon: Settings,
  },
];
