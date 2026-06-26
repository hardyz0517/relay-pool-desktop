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
      <aside className="flex w-60 shrink-0 flex-col border-r border-border bg-[#10141b]">
        <div className="border-b border-border px-4 py-3">
          <div className="text-sm font-semibold tracking-wide">
            Relay Pool Desktop
          </div>
          <div className="mt-1 text-xs text-muted-foreground">
            本地 AI 中转池调度器
          </div>
        </div>

        <nav className="flex-1 space-y-1 p-2">
          {appRoutes.map((route) => {
            const Icon = route.icon;
            const active = route.id === activeRouteId;

            return (
              <button
                key={route.id}
                type="button"
                onClick={() => onRouteChange(route.id)}
                className={cn(
                  "flex w-full items-center gap-2 rounded-md px-3 py-2 text-left text-sm transition-colors",
                  active
                    ? "bg-accent/20 text-foreground"
                    : "text-muted-foreground hover:bg-muted hover:text-foreground",
                )}
              >
                <Icon className="h-4 w-4 shrink-0" />
                <span>{route.label}</span>
              </button>
            );
          })}
        </nav>

        <div className="border-t border-border px-4 py-3 text-xs text-muted-foreground">
          <div className="flex items-center justify-between">
            <span>Local Proxy</span>
            <span className="flex items-center gap-1 text-amber-300">
              <Circle className="h-2 w-2 fill-current" />
              未启动
            </span>
          </div>
        </div>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-12 shrink-0 items-center justify-between border-b border-border bg-[#0d1117] px-4">
          <div>
            <div className="text-sm font-medium">{activeRoute?.label}</div>
            <div className="text-xs text-muted-foreground">
              {activeRoute?.description}
            </div>
          </div>

          <div className="flex items-center gap-2">
            <div className="hidden items-center gap-2 rounded-md border border-border bg-muted/45 px-3 py-1.5 text-xs text-muted-foreground md:flex">
              <Power className="h-3.5 w-3.5" />
              <span>127.0.0.1:8787/v1</span>
            </div>
            <Button variant="outline" className="h-8 px-2" title="复制本地入口">
              <Copy className="h-4 w-4" />
            </Button>
          </div>
        </header>

        <main className="min-h-0 flex-1 overflow-auto bg-[#0b0f14] p-4">
          {children}
        </main>
      </div>
    </div>
  );
}
