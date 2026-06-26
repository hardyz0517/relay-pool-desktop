import {
  Activity,
  BarChart3,
  ClipboardList,
  DatabaseZap,
  GitBranch,
  LayoutDashboard,
  Settings,
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
    label: "中转池",
    description: "站点列表、优先级和连接状态",
    icon: DatabaseZap,
  },
  {
    id: "collectors",
    label: "Sub2API 采集",
    description: "登录态、捕获接口和倍率快照",
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
