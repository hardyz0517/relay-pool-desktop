import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { ArrowLeft } from "lucide-react";
import { Button, EmptyState, useToast } from "@/components/ui";
import { listChangeEventsForStation } from "@/lib/api/changeEvents";
import { collectStationTask, getLatestCollectorSnapshot } from "@/lib/api/collector";
import { listCollectorRuns } from "@/lib/api/collectorRuns";
import { listBalanceSnapshotsForStation } from "@/lib/api/economics";
import { listGroupRateRecords, listStationGroupBindings } from "@/lib/api/groupFacts";
import { getStationCredentials, listStationKeys } from "@/lib/api/stationKeys";
import { listStations } from "@/lib/api/stations";
import type { ChangeEvent } from "@/lib/types/changeEvents";
import type { CollectorSnapshot, CollectorTaskType } from "@/lib/types/collector";
import type { CollectorRun } from "@/lib/types/collectorRuns";
import type { BalanceSnapshot } from "@/lib/types/economics";
import type { GroupRateRecord, StationGroupBinding } from "@/lib/types/groupFacts";
import type { StationCredentials, StationKey } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";
import {
  buildStationDetailViewModel,
  type StationDetailViewModel,
} from "./stationDetailViewModels";
import {
  StationDetailContent,
  type StationDetailRefreshAction,
} from "./components/StationDetailContent";

type StationDetailPageProps = {
  stationId: string | null;
  initialStation?: Station | null;
  onBack: () => void;
  onEditProvider: (stationId: string) => void;
};

type DetailData = {
  station: Station;
  balances: BalanceSnapshot[];
  groupBindings: StationGroupBinding[];
  groupRates: GroupRateRecord[];
  collectorRuns: CollectorRun[];
  latestSnapshot: CollectorSnapshot | null;
  credentials: StationCredentials | null;
  stationKeys: StationKey[];
  changes: ChangeEvent[];
};

type LoadMode = "initial" | "silent";

const refreshTaskByAction: Record<StationDetailRefreshAction, CollectorTaskType> = {
  balance: "balance",
  groups: "groups",
  full: "full",
};

const refreshSuccessLabel: Record<StationDetailRefreshAction, string> = {
  balance: "余额已刷新",
  groups: "分组倍率已采集",
  full: "采集已完成",
};

export function StationDetailPage({
  stationId,
  initialStation = null,
  onBack,
  onEditProvider,
}: StationDetailPageProps) {
  const toast = useToast();
  const mountedRef = useRef(true);
  const loadRequestRef = useRef(0);
  const refreshRequestRef = useRef(0);
  const activeStationIdRef = useRef<string | null>(stationId);
  const [detailData, setDetailData] = useState<DetailData | null>(() =>
    initialStation && initialStation.id === stationId ? createDetailDataSeed(initialStation) : null,
  );
  const [initialLoading, setInitialLoading] = useState(false);
  const [pageError, setPageError] = useState<string | null>(null);
  const [sectionError, setSectionError] = useState<string | null>(null);
  const [loadingAction, setLoadingAction] = useState<StationDetailRefreshAction | null>(null);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      loadRequestRef.current += 1;
      refreshRequestRef.current += 1;
    };
  }, []);

  const isLoadCurrent = useCallback((id: string, requestId: number) => {
    return (
      mountedRef.current &&
      loadRequestRef.current === requestId &&
      activeStationIdRef.current === id
    );
  }, []);

  const isRefreshCurrent = useCallback((id: string, requestId: number) => {
    return (
      mountedRef.current &&
      refreshRequestRef.current === requestId &&
      activeStationIdRef.current === id
    );
  }, []);

  const loadDetail = useCallback(async (id: string, mode: LoadMode) => {
    const requestId = loadRequestRef.current + 1;
    loadRequestRef.current = requestId;

    if (mode === "initial") {
      setInitialLoading(true);
      setPageError(null);
      setSectionError(null);
      setDetailData(null);
    }

    try {
      const [
        stations,
        credentials,
        stationKeys,
        groupBindings,
        groupRates,
        collectorRuns,
        latestSnapshot,
        balanceSnapshots,
        changeEvents,
      ] = await Promise.all([
        listStations(),
        getStationCredentials(id),
        listStationKeys(id),
        listStationGroupBindings(id),
        listGroupRateRecords(id),
        listCollectorRuns(id),
        getLatestCollectorSnapshot(id),
        listBalanceSnapshotsForStation(id),
        listChangeEventsForStation(id),
      ]);
      const station = stations.find((item) => item.id === id);

      if (!station) {
        throw new Error("未找到中转站");
      }

      if (!isLoadCurrent(id, requestId)) {
        return null;
      }

      const nextData: DetailData = {
        station,
        credentials,
        stationKeys,
        groupBindings,
        groupRates,
        collectorRuns,
        latestSnapshot,
        balances: balanceSnapshots,
        changes: changeEvents,
      };
      setDetailData(nextData);
      setPageError(null);
      setSectionError(null);
      return nextData;
    } catch (requestError) {
      const message = readError(requestError);
      if (isLoadCurrent(id, requestId)) {
        if (mode === "initial") {
          setPageError(message);
          setDetailData(null);
        } else {
          setSectionError(message);
        }
      }
      throw requestError;
    } finally {
      if (isLoadCurrent(id, requestId) && mode === "initial") {
        setInitialLoading(false);
      }
    }
  }, [isLoadCurrent]);

  useLayoutEffect(() => {
    activeStationIdRef.current = stationId;
    refreshRequestRef.current += 1;
    setLoadingAction(null);

    if (!stationId) {
      loadRequestRef.current += 1;
      setDetailData(null);
      setInitialLoading(false);
      setPageError("未选择中转站");
      setSectionError(null);
      return;
    }

    if (initialStation?.id === stationId) {
      setDetailData((current) =>
        current?.station.id === stationId ? current : createDetailDataSeed(initialStation),
      );
      setInitialLoading(false);
      setPageError(null);
      setSectionError(null);
      void loadDetail(stationId, "silent").catch(() => undefined);
      return;
    }

    void loadDetail(stationId, "initial").catch(() => undefined);
  }, [initialStation, loadDetail, stationId]);

  const viewModel = useMemo<StationDetailViewModel | null>(() => {
    if (!detailData) {
      return null;
    }
    return buildStationDetailViewModel(detailData);
  }, [detailData]);

  const handleRefresh = useCallback(async (action: StationDetailRefreshAction) => {
    if (!stationId || loadingAction) {
      return;
    }

    const requestId = refreshRequestRef.current + 1;
    refreshRequestRef.current = requestId;
    setLoadingAction(action);
    setSectionError(null);
    try {
      await collectStationTask(stationId, refreshTaskByAction[action]);
      if (!isRefreshCurrent(stationId, requestId)) {
        return;
      }
      const nextData = await loadDetail(stationId, "silent");
      if (!nextData || !isRefreshCurrent(stationId, requestId)) {
        return;
      }
      toast.success(refreshSuccessLabel[action]);
    } catch (requestError) {
      const message = readError(requestError);
      if (isRefreshCurrent(stationId, requestId)) {
        setSectionError(message);
        toast.error("采集失败", message);
      }
    } finally {
      if (isRefreshCurrent(stationId, requestId)) {
        setLoadingAction(null);
      }
    }
  }, [isRefreshCurrent, loadDetail, loadingAction, stationId, toast]);

  if (initialLoading) {
    return (
      <div className="rounded-[var(--surface-radius)] border border-border bg-white px-4 py-5 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">
        正在读取中转站详情...
      </div>
    );
  }

  if (!viewModel) {
    return (
      <EmptyState
        title={pageError ?? "未找到中转站"}
        description="返回中转站资产后可重新选择。"
        action={
          <Button variant="secondary" onClick={onBack}>
            <ArrowLeft className="h-4 w-4" />
            返回
          </Button>
        }
      />
    );
  }

  return (
    <StationDetailContent
      viewModel={viewModel}
      loadingAction={loadingAction}
      sectionError={sectionError}
      onBack={onBack}
      onEdit={() => onEditProvider(viewModel.station.id)}
      onRefresh={(action) => void handleRefresh(action)}
    />
  );
}

function readError(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function createDetailDataSeed(station: Station): DetailData {
  return {
    station,
    balances: [],
    groupBindings: [],
    groupRates: [],
    collectorRuns: [],
    latestSnapshot: null,
    credentials: null,
    stationKeys: [],
    changes: [],
  };
}
