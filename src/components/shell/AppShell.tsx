import type { ReactNode } from "react";
import { Circle, Copy, Power } from "lucide-react";
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
  const activeRoute = appRoutes.find((route) => route.id === activeRouteId);

  return (
    <div className="flex h-screen min-h-[640px] overflow-hidden bg-background text-foreground">
      <aside className="flex w-[196px] shrink-0 flex-col border-r border-cyan-100 bg-white/90 backdrop-blur">
        <div className="border-b border-border px-3 py-2.5">
          <div className="truncate text-[13px] font-semibold tracking-wide text-slate-800">
            Relay Pool Desktop
          </div>
          <div className="mt-0.5 truncate text-[11px] text-muted-foreground">
            本地 AI 中转池调度器
          </div>
        </div>

        <nav className="flex-1 space-y-0.5 p-1.5">
          {appRoutes.map((route) => {
            const Icon = route.icon;
            const active = route.id === activeRouteId;

            return (
              <button
                key={route.id}
                type="button"
                onClick={() => onRouteChange(route.id)}
                className={cn(
                  "flex h-9 w-full cursor-pointer items-center gap-2 rounded-md px-2.5 text-left text-[13px] transition-colors",
                  active
                    ? "bg-teal-50 text-teal-700 shadow-[inset_3px_0_0_rgb(13,148,136)]"
                    : "text-slate-600 hover:bg-cyan-50 hover:text-slate-800",
                )}
              >
                <Icon className="h-4 w-4 shrink-0" />
                <span>{route.label}</span>
              </button>
            );
          })}
        </nav>

        <div className="border-t border-cyan-100 px-3 py-2.5 text-xs text-muted-foreground">
          <div className="flex items-center justify-between">
            <span>Local Proxy</span>
            <span className="flex items-center gap-1 text-amber-600">
              <Circle className="h-2 w-2 fill-current" />
              未启动
            </span>
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
