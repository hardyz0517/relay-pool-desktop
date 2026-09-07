import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { ArrowLeft } from "lucide-react";
import { Button, EmptyState, useToast } from "@/components/ui";
import { readError } from "@/lib/errors";
import { collectStationTask, startManualAuthorization } from "@/lib/api/collector";
import { openStationWebsite } from "@/lib/api/stations";
import { stationDetailReadModelQueryOptions } from "@/lib/query/resourceQueries";
import { reconcileStationDetailReadModel } from "@/lib/query/stationCollectionQuerySynchronization";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import type { CollectorSnapshot, CollectorTaskType } from "@/lib/types/collector";
import type { CollectorRun } from "@/lib/types/collectorRuns";
import type { BalanceSnapshot } from "@/lib/types/economics";
import type { GroupRateRecord, StationGroupBinding } from "@/lib/types/groupFacts";
import type { RoutingDeepLink } from "@/lib/types/routingDeepLinks";
import type { StationCredentials, StationKey } from "@/lib/types/stationKeys";
import type { StationDetailIncident, StationDetailReadModel } from "@/lib/types/stationAssets";
import type { Station } from "@/lib/types/stations";
import {
  buildStationDetailViewModel,
  type StationDetailViewModel,
} from "./stationDetailViewModels";
import {
  StationDetailContent,
  type StationDetailLoadingAction,
  type StationDetailRefreshAction,
} from "./components/StationDetailContent";
import { StationPublishedStatusSection } from "./components/StationPublishedStatusSection";
import { RechargeDialog } from "./components/RechargeDialog";
import { useStationPublishedStatus } from "./useStationPublishedStatus";

type StationDetailPageProps = {
  stationId: string | null;
  initialStation?: Station | null;
  onBack: () => void;
  onEditProvider: (stationId: string) => void;
  onOpenRoutingDeepLink?: (link: StationDetailRoutingDeepLink) => void;
};

type StationDetailRoutingDeepLink = Extract<RoutingDeepLink, { kind: "station" }> & {
  source: "station_endpoint_health";
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
  incidents: StationDetailIncident[];
};

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
  onOpenRoutingDeepLink,
}: StationDetailPageProps) {
  const toast = useToast();
  const queryClient = useQueryClient();
  const publishedStatus = useStationPublishedStatus(stationId);
  const detailQuery = useActivityQuery(stationDetailReadModelQueryOptions(stationId));
  const refetchDetail = detailQuery.refetch;
  const mountedRef = useRef(true);
  const refreshRequestRef = useRef(0);
  const activeStationIdRef = useRef<string | null>(stationId);
  const [sectionError, setSectionError] = useState<string | null>(null);
  const [loadingAction, setLoadingAction] = useState<StationDetailLoadingAction | null>(null);
  const [rechargeCenterOpen, setRechargeCenterOpen] = useState(false);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      refreshRequestRef.current += 1;
    };
  }, []);

  const isRefreshCurrent = useCallback((id: string, requestId: number) => {
    return (
      mountedRef.current &&
      refreshRequestRef.current === requestId &&
      activeStationIdRef.current === id
    );
  }, []);

  useEffect(() => {
    activeStationIdRef.current = stationId;
    refreshRequestRef.current += 1;
    setLoadingAction(null);
    setRechargeCenterOpen(false);
    setSectionError(null);
  }, [stationId]);

  useEffect(() => {
    if (!stationId) return;
    let disposed = false;
    let inFlight = false;
    const tracker = new Map<string, number>();
    const reconcile = () => {
      if (disposed || inFlight) return;
      inFlight = true;
      void reconcileStationDetailReadModel(queryClient, stationId, tracker)
        .catch(() => undefined)
        .finally(() => {
          inFlight = false;
        });
    };
    const reconcileWhenVisible = () => {
      if (document.visibilityState === "visible") reconcile();
    };
    document.addEventListener("visibilitychange", reconcileWhenVisible);
    window.addEventListener("pageshow", reconcile);
    const intervalId = window.setInterval(reconcile, 30_000);
    reconcile();
    return () => {
      disposed = true;
      document.removeEventListener("visibilitychange", reconcileWhenVisible);
      window.removeEventListener("pageshow", reconcile);
      window.clearInterval(intervalId);
    };
  }, [queryClient, stationId]);

  const detailData = useMemo<DetailData | null>(() => {
    const detail = detailQuery.data?.data;
    if (detail) {
      return detailDataFromReadModel(detail);
    }
    return initialStation?.id === stationId ? createDetailDataSeed(initialStation) : null;
  }, [detailQuery.data, initialStation, stationId]);

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
      const result = await collectStationTask(stationId, refreshTaskByAction[action]);
      if (!isRefreshCurrent(stationId, requestId)) {
        return;
      }
      const refreshed = await refetchDetail();
      if (refreshed.error) {
        throw refreshed.error;
      }
      if (!refreshed.data || !isRefreshCurrent(stationId, requestId)) {
        return;
      }
      const nextData = detailDataFromReadModel(refreshed.data.data);
      if (result.snapshot.status === "manual_required") {
        toast.info(
          `「${nextData.station.name}」需重新授权`,
          result.snapshot.errorMessage ?? "当前登录状态已失效，请重新进行窗口授权。",
        );
      } else if (result.snapshot.status === "failed") {
        toast.error(
          `「${nextData.station.name}」采集失败`,
          result.snapshot.errorMessage ?? "采集任务未能完成。",
        );
      } else {
        toast.success(refreshSuccessLabel[action]);
      }
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
  }, [isRefreshCurrent, loadingAction, refetchDetail, stationId, toast]);

  const handleManualAuthorization = useCallback(async () => {
    if (!stationId || loadingAction) {
      return;
    }
    setLoadingAction("authorize");
    setSectionError(null);
    try {
      await startManualAuthorization(stationId);
      toast.success("人工授权窗口已打开");
    } catch (requestError) {
      const message = readError(requestError);
      setSectionError(message);
      toast.error("打开人工授权失败", message);
    } finally {
      setLoadingAction(null);
    }
  }, [loadingAction, stationId, toast]);

  if (stationId && detailQuery.isPending && !detailData) {
    return (
      <div className="rounded-[var(--surface-radius)] border border-border bg-surface px-4 py-5 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">
        正在读取中转站详情...
      </div>
    );
  }

  if (!viewModel) {
    return (
      <EmptyState
        title={!stationId ? "未选择中转站" : detailQuery.error ? readError(detailQuery.error) : "未找到中转站"}
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
    <>
      <StationDetailContent
        viewModel={viewModel}
        loadingAction={loadingAction}
        sectionError={sectionError}
        onBack={onBack}
        onEdit={() => onEditProvider(viewModel.station.id)}
        onOpenWebsite={() => void openStationWebsite(viewModel.station.websiteUrl)}
        onOpenRechargeCenter={() => setRechargeCenterOpen(true)}
        onOpenRoutingDeepLink={
          onOpenRoutingDeepLink
            ? () =>
                onOpenRoutingDeepLink({
                  kind: "station",
                  stationId: viewModel.station.id,
                  source: "station_endpoint_health",
                })
            : undefined
        }
        onAuthorize={() => void handleManualAuthorization()}
        onRefresh={(action) => void handleRefresh(action)}
        publishedStatusSection={
          <StationPublishedStatusSection
            stationName={viewModel.station.name}
            stationType={viewModel.station.stationType}
            workspace={publishedStatus.workspace}
            isLoading={publishedStatus.isLoading}
            isError={publishedStatus.isError}
            isRefreshing={publishedStatus.isRefreshing}
            isRefreshError={publishedStatus.isRefreshError}
            onRefresh={publishedStatus.refresh}
            onRetryWorkspace={publishedStatus.retryWorkspace}
          />
        }
      />
      <RechargeDialog
        station={rechargeCenterOpen ? viewModel.station : null}
        onClose={() => setRechargeCenterOpen(false)}
        onAuthorize={() => void handleManualAuthorization()}
        onOpenUrl={(url) => openStationWebsite(url)}
      />
    </>
  );
}

function detailDataFromReadModel(
  detail: StationDetailReadModel,
): DetailData {
  return {
    station: {
      ...detail.asset.station,
      collectionSummary: detail.asset.collectionSummary,
      authorizationSummary: detail.asset.authorizationSummary,
    },
    credentials: detail.credentials,
    stationKeys: detail.asset.keys,
    groupBindings: detail.groupBindings,
    groupRates: detail.groupRates,
    collectorRuns: detail.collectorRuns,
    latestSnapshot: detail.latestSnapshot,
    balances: detail.balances,
    incidents: detail.incidents,
  };
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
    incidents: [],
  };
}
