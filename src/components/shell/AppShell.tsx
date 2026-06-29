import type { ReactNode } from "react";
import { Circle, Copy, Power, Square } from "lucide-react";
import { appRoutes } from "@/app/routes";
import { IconButton } from "@/components/ui";
import { shellLayout } from "@/components/ui/layout";
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
  const activeRoute = appRoutes.find((route) => route.id === activeRouteId);

  return (
    <div className="flex h-screen min-h-[640px] overflow-hidden bg-background text-foreground">
      <aside
        className="flex shrink-0 flex-col border-r border-border bg-white"
        style={{ width: shellLayout.sidebarWidth }}
      >
        <div className="grid h-[57px] grid-cols-[20px_minmax(0,1fr)] items-center gap-3 overflow-hidden border-b border-border px-4">
          <div className="flex items-center justify-center">
            <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border border-border bg-white text-slate-700">
              <Square className="h-4 w-4 fill-current" />
            </div>
          </div>
          <div className="min-w-0 overflow-hidden whitespace-nowrap">
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

        <nav className="flex flex-1 flex-col items-center gap-1 px-2 py-2">
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
                  "flex h-10 w-10 cursor-pointer items-center justify-center rounded-[var(--surface-radius)] transition-colors",
                  active
                    ? "bg-slate-900 text-white"
                    : "text-slate-500 hover:bg-slate-100 hover:text-slate-900",
                )}
              >
                <Icon className="h-4.5 w-4.5" />
              </button>
            );
          })}
        </nav>

        <div className="flex flex-col items-center gap-2 border-t border-border px-2 py-3">
          <span
            className="flex h-10 w-10 items-center justify-center rounded-[var(--surface-radius)] border border-border bg-white"
            title="本地代理未启动"
            aria-label="本地代理未启动"
          >
            <Circle className="h-2.5 w-2.5 fill-current text-amber-500" />
          </span>
          <IconButton label="复制本地入口">
            <Copy className="h-4 w-4" />
          </IconButton>
        </div>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-[var(--shell-header-height)] shrink-0 items-center justify-end border-b border-border bg-white px-4">
          <div className="flex items-center overflow-hidden rounded-xl border border-cyan-100 bg-cyan-50/70 text-xs text-slate-600">
            <div className="hidden items-center gap-1.5 border-r border-cyan-100 px-2.5 py-1.5 lg:flex">
              <Circle className="h-2 w-2 fill-current text-amber-500" />
              <span>本地代理未启动</span>
            </div>
            <div className="hidden border-r border-cyan-100 px-2.5 py-1.5 md:block">
              Key 池优先
            </div>
            <div className="hidden items-center gap-1.5 border-r border-cyan-100 px-2.5 py-1.5 md:flex">
              <Power className="h-3.5 w-3.5" />
              <span>127.0.0.1:8787/v1</span>
            </div>
            <IconButton label="复制本地入口" variant="ghost" className="h-7 rounded-none px-2">
              <Copy className="h-4 w-4" />
            </IconButton>
          </div>
        </header>

        <main className="min-h-0 flex-1 overflow-auto bg-background p-[var(--shell-page-gap)]">
          {children}
        </main>
      </div>
    </div>
  );
}
