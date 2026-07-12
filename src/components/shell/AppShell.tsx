import { useEffect, useMemo, type ReactNode } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Circle } from "lucide-react";
import { appRoutes } from "@/app/routes";
import { shellLayout } from "@/components/ui/layout";
import { CHANGE_EVENTS_UPDATED_EVENT } from "@/lib/api/changeEvents";
import { PROXY_STATUS_UPDATED_EVENT } from "@/lib/api/proxy";
import { SETTINGS_UPDATED_EVENT } from "@/lib/api/settings";
import type { ProxyStatus } from "@/lib/types/proxy";
import type { AppSettings } from "@/lib/types/settings";
import {
  changeEventsQueryOptions,
  proxyStatusQueryOptions,
  settingsQueryOptions,
} from "@/lib/query/resourceQueries";
import { queryKeys } from "@/lib/query/queryKeys";
import { cn } from "@/lib/utils";
import { unreadChangeCount } from "@/features/changes/changeEventViewModels";
import type { AppRouteId } from "@/lib/types/navigation";

type AppShellProps = {
  activeRouteId: AppRouteId;
  children: ReactNode;
  onRouteChange: (routeId: AppRouteId) => void;
};

export function AppShell({
  activeRouteId,
  children,
  onRouteChange,
}: AppShellProps) {
  const queryClient = useQueryClient();
  const { data: changeEvents = [] } = useQuery(changeEventsQueryOptions(10_000));
  const { data: proxyStatus = null } = useQuery(proxyStatusQueryOptions(2_000));
  const { data: settings = null } = useQuery(settingsQueryOptions());

  useEffect(() => {
    function refreshChangeEvents() {
      void queryClient.invalidateQueries({ queryKey: queryKeys.changeEvents });
    }

    window.addEventListener(CHANGE_EVENTS_UPDATED_EVENT, refreshChangeEvents);
    return () => window.removeEventListener(CHANGE_EVENTS_UPDATED_EVENT, refreshChangeEvents);
  }, [queryClient]);

  useEffect(() => {
    function handleProxyStatusUpdated(event: Event) {
      queryClient.setQueryData(queryKeys.proxyStatus, (event as CustomEvent<ProxyStatus>).detail);
    }

    window.addEventListener(PROXY_STATUS_UPDATED_EVENT, handleProxyStatusUpdated);
    return () => window.removeEventListener(PROXY_STATUS_UPDATED_EVENT, handleProxyStatusUpdated);
  }, [queryClient]);

  useEffect(() => {
    function handleSettingsUpdated(event: Event) {
      const nextSettings = (event as CustomEvent<AppSettings>).detail;
      if (nextSettings) {
        queryClient.setQueryData(queryKeys.settings, nextSettings);
        return;
      }
      void queryClient.invalidateQueries({ queryKey: queryKeys.settings });
    }

    window.addEventListener(SETTINGS_UPDATED_EVENT, handleSettingsUpdated);
    return () => window.removeEventListener(SETTINGS_UPDATED_EVENT, handleSettingsUpdated);
  }, [queryClient]);

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

  const changeUnreadCount = useMemo(() => unreadChangeCount(changeEvents), [changeEvents]);
  const proxyRunning = proxyStatus?.running ?? false;

  return (
    <div className="flex h-dvh min-h-0 overflow-hidden bg-background text-foreground">
      <aside
        className="flex min-h-0 shrink-0 flex-col border-r border-border bg-white"
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
                onClick={() => onRouteChange(route.id)}
                title={route.label}
                aria-label={route.label}
                className={cn(
                  "relative flex h-10 w-10 cursor-pointer items-center justify-center rounded-[var(--surface-radius)] transition-colors",
                  active
                    ? "bg-slate-900 text-white"
                    : "text-slate-500 hover:bg-slate-100 hover:text-slate-900",
                )}
              >
                <Icon className="h-4.5 w-4.5" />
                {route.id === "changes" && changeUnreadCount > 0 && (
                  <span className="absolute right-1 top-1 min-w-4 rounded-full bg-rose-600 px-1 text-[10px] font-semibold leading-4 text-white">
                    {changeUnreadCount > 99 ? "99+" : changeUnreadCount}
                  </span>
                )}
              </button>
            );
          })}
        </nav>

        <div className="flex flex-col items-center gap-2 border-t border-border px-2 py-3">
          <span
            className="flex h-10 w-10 items-center justify-center rounded-[var(--surface-radius)] border border-border bg-white"
            title={proxyRunning ? "本地代理运行中" : "本地代理未启动"}
            aria-label={proxyRunning ? "本地代理运行中" : "本地代理未启动"}
          >
            <Circle
              className={cn(
                "h-2.5 w-2.5 fill-current",
                proxyRunning ? "text-emerald-500" : "text-amber-500",
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
