import { useState, type ReactNode } from "react";
import { ChevronLeft, ChevronRight, Circle, Copy, Power, Square } from "lucide-react";
import { appRoutes } from "@/app/routes";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
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
  const [collapsed, setCollapsed] = useState(false);
  const activeRoute = appRoutes.find((route) => route.id === activeRouteId);

  return (
    <div className="flex h-screen min-h-[640px] overflow-hidden bg-background text-foreground">
      <aside
        className={cn(
          "flex shrink-0 flex-col border-r border-cyan-100 bg-white/90 backdrop-blur transition-[width] duration-200",
          collapsed ? "w-[64px]" : "w-[196px]",
        )}
      >
        <div className="grid h-[57px] grid-cols-[64px_minmax(0,1fr)] items-center overflow-hidden border-b border-border">
          <div className="flex items-center justify-center">
            <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-cyan-100 bg-teal-50 text-teal-700">
              <Square className="h-4 w-4 fill-current" />
            </div>
          </div>
          <div
            className={cn(
              "min-w-0 overflow-hidden pr-3 transition-opacity duration-200 ease-out",
              collapsed ? "opacity-0" : "opacity-100",
            )}
          >
            <div className="min-w-0">
              <div className="truncate text-[13px] font-semibold tracking-wide text-slate-800">
                Relay Pool Desktop
              </div>
              <div className="mt-0.5 truncate text-[11px] text-muted-foreground">
                本地 AI 中转站调度器
              </div>
            </div>
          </div>
        </div>

        <nav className={cn("flex-1 space-y-0.5", collapsed ? "p-1" : "p-1.5")}>
          {appRoutes.map((route) => {
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
                  "grid h-9 w-full cursor-pointer grid-cols-[64px_minmax(0,1fr)] items-center overflow-hidden rounded-md text-left text-[13px] transition-colors",
                  active
                    ? "bg-teal-50 text-teal-700 shadow-[inset_3px_0_0_rgb(13,148,136)]"
                    : "text-slate-600 hover:bg-cyan-50 hover:text-slate-800",
                )}
              >
                <span className="flex items-center justify-center">
                  <Icon className="h-4 w-4 shrink-0" />
                </span>
                <span
                  className={cn(
                    "min-w-0 overflow-hidden pr-2 transition-opacity duration-200 ease-out",
                    collapsed ? "opacity-0" : "opacity-100",
                  )}
                >
                  {route.label}
                </span>
              </button>
            );
          })}
        </nav>

        <div className="border-t border-cyan-100 px-0 py-2.5 text-xs text-muted-foreground">
          <div className="grid grid-cols-[64px_minmax(0,1fr)] items-center">
            <span className="flex items-center justify-center">
              <Circle className="h-2 w-2 fill-current text-amber-600" />
            </span>
            <span
              className={cn(
                "min-w-0 overflow-hidden pr-3 transition-opacity duration-200 ease-out",
                collapsed ? "opacity-0" : "opacity-100",
              )}
            >
              <span className="flex items-center justify-between">
                <span>Local Proxy</span>
                <span className="text-amber-600">未启动</span>
              </span>
            </span>
          </div>
          <div className="mt-2">
            <Button
              variant="ghost"
              className="grid h-8 w-full grid-cols-[64px_minmax(0,1fr)] items-center overflow-hidden rounded-lg px-0 text-[13px]"
              onClick={() => setCollapsed((value) => !value)}
              title={collapsed ? "展开侧边栏" : "收起侧边栏"}
              aria-label={collapsed ? "展开侧边栏" : "收起侧边栏"}
            >
              <span className="flex items-center justify-center">
                {collapsed ? <ChevronRight className="h-4 w-4" /> : <ChevronLeft className="h-4 w-4" />}
              </span>
              <span
                className={cn(
                  "min-w-0 overflow-hidden pr-2 transition-opacity duration-200 ease-out",
                  collapsed ? "opacity-0" : "opacity-100",
                )}
              >
                {collapsed ? "展开" : "收起"}
              </span>
            </Button>
          </div>
        </div>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-11 shrink-0 items-center justify-between border-b border-cyan-100 bg-white/88 px-4 backdrop-blur">
          <div>
            <div className="text-[13px] font-medium text-slate-800">
              {activeRoute?.label}
            </div>
            <div className="text-xs text-muted-foreground">
              {activeRoute?.description}
            </div>
          </div>

          <div className="flex items-center overflow-hidden rounded-xl border border-cyan-100 bg-cyan-50/70 text-xs text-slate-600">
            <div className="hidden items-center gap-1.5 border-r border-cyan-100 px-2.5 py-1.5 lg:flex">
              <Circle className="h-2 w-2 fill-current text-amber-500" />
              <span>代理未启动</span>
            </div>
            <div className="hidden border-r border-cyan-100 px-2.5 py-1.5 md:block">
              手动优先
            </div>
            <div className="hidden items-center gap-1.5 border-r border-cyan-100 px-2.5 py-1.5 md:flex">
              <Power className="h-3.5 w-3.5" />
              <span>127.0.0.1:8787/v1</span>
            </div>
            <Button variant="ghost" className="h-7 rounded-none px-2" title="复制本地入口">
              <Copy className="h-4 w-4" />
            </Button>
          </div>
        </header>

        <main className="min-h-0 flex-1 overflow-auto bg-background p-4">
          {children}
        </main>
      </div>
    </div>
  );
}
