import type { LucideIcon } from "lucide-react";

export type AppRouteId =
  | "dashboard"
  | "stations"
  | "keyPool"
  | "routing"
  | "pricing"
  | "channels"
  | "collectors"
  | "changes"
  | "logs"
  | "settings";

export type AppPageId = AppRouteId | "addProvider" | "editProvider" | "stationDetail" | "addKey";

export type AppRoute = {
  id: AppRouteId;
  label: string;
  description: string;
  icon: LucideIcon;
};
