import { useEffect, useLayoutEffect, useMemo, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { markNavigation, navigationMarks } from "@/app/navigationPerformance";
import { appRoutes } from "@/app/routes";
import { LocalProxyRadarIcon } from "@/components/shell/LocalProxyRadarIcon";
import { shellLayout } from "@/components/ui/layout";
import {
  proxyStatusQueryOptions,
  settingsQueryOptions,
} from "@/lib/query/resourceQueries";
import { alertingCurrentQueryOptions as currentAlertingQueryOptions } from "@/lib/queries/alertingQueries";
import { cn } from "@/lib/utils";
import type { AppRouteId } from "@/lib/types/navigation";

type AppShellProps = {
  activeRouteId: AppRouteId;
  children: ReactNode;
  navigationSequence: number;
  onRouteChange: (routeId: AppRouteId) => void;
};

export function AppShell({
  activeRouteId,
  children,
  navigationSequence,
  onRouteChange,
}: AppShellProps) {
  const { data: alertingPage } = useQuery(currentAlertingQueryOptions({ limit: 200 }));
  const { data: proxyStatus = null } = useQuery(proxyStatusQueryOptions(2_000));
  const { data: settings = null } = useQuery(settingsQueryOptions());

  const visibleRoutes = useMemo(
    () =>
      appRoutes.filter((route) => route.id !== "collectors" || settings?.developerModeEnabled),
    [settings?.developerModeEnabled],
  );

  useEffect(() => {
    if (activeRouteId === "collectors" && settings && !settings.developerModeEnabled) {
      onRouteChange("settings");
    }
  }, [activeRouteId, onRouteChange, settings]);

  const changeUnreadCount = alertingPage?.unseenCount ?? 0;
  const proxyRunning = proxyStatus?.running ?? false;

  useLayoutEffect(() => {
    markNavigation(navigationMarks.indicator(navigationSequence));
  }, [navigationSequence]);

  return (
    <div className="flex h-dvh min-h-0 overflow-hidden bg-background text-foreground">
      <aside
        className="flex min-h-0 shrink-0 flex-col border-r border-border bg-surface"
        style={{ width: shellLayout.sidebarWidth }}
      >
        <nav className="flex min-h-0 flex-1 flex-col items-center gap-1 overflow-y-auto px-2 py-2 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
          {visibleRoutes.map((route) => {
            const Icon = route.icon;
            const active = route.id === activeRouteId;

            return (
              <button
                key={route.id}
                type="button"
                data-navigation-route-id={route.id}
                onClick={() => onRouteChange(route.id)}
                title={route.label}
                aria-label={route.label}
                className={cn(
                  "relative flex h-10 w-10 cursor-pointer items-center justify-center rounded-[var(--surface-radius)] transition-colors",
                  active
                    ? "bg-selected text-selected-foreground"
                    : "text-muted-foreground hover:bg-hover hover:text-foreground",
                )}
              >
                <Icon className="h-4.5 w-4.5" />
                {route.id === "changes" && changeUnreadCount > 0 && (
                  <span className="absolute right-1 top-1 min-w-4 rounded-full bg-danger-solid px-1 text-[10px] font-semibold leading-4 text-on-solid">
                    {changeUnreadCount > 99 ? "99+" : changeUnreadCount}
                  </span>
                )}
              </button>
            );
          })}
        </nav>

        <div className="flex flex-col items-center gap-2 border-t border-border px-2 py-3">
          <span
            className="flex h-10 w-10 items-center justify-center rounded-[var(--surface-radius)] border border-border bg-surface"
            title={proxyRunning ? "本地代理运行中" : "本地代理未启动"}
            aria-label={proxyRunning ? "本地代理运行中" : "本地代理未启动"}
          >
            <LocalProxyRadarIcon
              active={proxyRunning}
              className={cn(
                "h-6 w-6",
                proxyRunning ? "text-success-foreground" : "text-muted-foreground",
              )}
            />
          </span>
        </div>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col">
        <main className="min-h-0 flex-1 overflow-hidden bg-background">
          {children}
        </main>
      </div>
    </div>
  );
}
