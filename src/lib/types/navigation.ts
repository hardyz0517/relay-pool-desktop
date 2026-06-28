import type { LucideIcon } from "lucide-react";

export type AppRouteId =
  | "dashboard"
  | "stations"
  | "keyPool"
  | "channels"
  | "collectors"
  | "pricing"
  | "routing"
  | "logs"
  | "settings";

export type AppPageId = AppRouteId | "addProvider";

export type AppRoute = {
  id: AppRouteId;
  label: string;
  description: string;
  icon: LucideIcon;
};
