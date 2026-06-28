import { useMemo, useState } from "react";
import { AppShell } from "@/components/shell/AppShell";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, SectionCard } from "@/components/ui";
import { CollectorsPage } from "@/features/collectors/CollectorsPage";
import { DashboardPage } from "@/features/dashboard/DashboardPage";
import { LogsPage } from "@/features/logs/LogsPage";
import { PricingPage } from "@/features/pricing/PricingPage";
import { RoutingPage } from "@/features/routing/RoutingPage";
import { KeyPoolPage } from "@/features/key-pool/KeyPoolPage";
import { SettingsPage } from "@/features/settings/SettingsPage";
import { ChannelStatusPage } from "@/features/channels/ChannelStatusPage";
import { StationsPage } from "@/features/stations/StationsPage";
import { ArrowLeft } from "lucide-react";
import type { AppPageId } from "@/lib/types/navigation";
import type { AppRouteId } from "@/lib/types/navigation";

export function App() {
  const [activeRouteId, setActiveRouteId] = useState<AppPageId>("dashboard");
  const activeShellRouteId: AppRouteId = activeRouteId === "addProvider" ? "dashboard" : activeRouteId;

  const page = useMemo(() => {
    switch (activeRouteId) {
      case "addProvider":
        return (
          <PageScaffold
            title="添加 Provider"
            description="沉浸式添加流程会在下一步接入。"
            actions={
              <Button variant="secondary" onClick={() => setActiveRouteId("dashboard")}>
                <ArrowLeft className="h-4 w-4" />
                返回工作台
              </Button>
            }
          >
            <SectionCard title="准备中" description="当前仍停留在工作台阶段。">
              <div className="text-sm text-slate-700">
                Add Provider 页面将在后续任务中接入。
              </div>
            </SectionCard>
          </PageScaffold>
        );
      case "stations":
        return <StationsPage />;
      case "keyPool":
        return <KeyPoolPage />;
      case "channels":
        return <ChannelStatusPage />;
      case "collectors":
        return <CollectorsPage />;
      case "pricing":
        return <PricingPage />;
      case "routing":
        return <RoutingPage />;
      case "logs":
        return <LogsPage />;
      case "settings":
        return <SettingsPage />;
      case "dashboard":
      default:
        return <DashboardPage onNavigate={setActiveRouteId} />;
    }
  }, [activeRouteId]);

  return (
    <AppShell activeRouteId={activeShellRouteId} onRouteChange={setActiveRouteId}>
      {page}
    </AppShell>
  );
}
