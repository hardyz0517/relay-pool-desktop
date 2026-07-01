import type { ReactNode } from "react";
import { Circle, Copy } from "lucide-react";
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
        <main className="min-h-0 flex-1 overflow-auto bg-background p-[var(--shell-page-gap)]">
          {children}
        </main>
      </div>
    </div>
  );
}
