import { PageScaffold } from "@/components/shell/PageScaffold";
import { StationDetailPanel } from "./components/StationDetailPanel";
import { StationListItem } from "./components/StationListItem";
import { mockStations } from "@/lib/mock";

export function StationsPage() {
  const selectedStation = mockStations[0];

  return (
    <PageScaffold
      eyebrow="Stations"
      title="中转池"
      description="左侧为站点排序与状态，右侧为所选站点详情。当前仅使用假数据。"
    >
      <div className="grid gap-4 xl:grid-cols-[360px_minmax(0,1fr)]">
        <div className="space-y-2">
          {mockStations.map((station) => (
            <StationListItem
              key={station.id}
              station={station}
              active={station.id === selectedStation.id}
            />
          ))}
        </div>
        <StationDetailPanel station={selectedStation} />
      </div>
    </PageScaffold>
  );
}
