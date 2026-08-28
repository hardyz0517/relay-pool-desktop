import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useToast } from "@/components/ui";
import { getStationPublishedStatusOverview } from "@/lib/api/stationPublishedStatusOverview";
import { readError } from "@/lib/errors";
import { queryKeys } from "@/lib/query/queryKeys";
import { stationPublishedStatusOverviewQueryOptions } from "@/lib/query/resourceQueries";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import {
  buildOfficialStatusView,
  createOfficialStatusInput,
  defaultOfficialStatusFilters,
  type OfficialStatusFilters,
} from "./officialStatusViewModel";

export const OFFICIAL_STATUS_PAGE_SIZE_OPTIONS = [20, 50, 100] as const;

export function useOfficialStatusController() {
  const toast = useToast();
  const queryClient = useQueryClient();
  const [filters, setFilters] = useState<OfficialStatusFilters>(defaultOfficialStatusFilters);
  const [page, setPage] = useState(1);
  const [pageSize, setPageSizeState] = useState<number>(100);
  const [pageCursors, setPageCursors] = useState<Record<number, string | null>>({ 1: null });
  const [knownTotal, setKnownTotal] = useState(0);
  const [jumpingPage, setJumpingPage] = useState<number | null>(null);
  const paginationRequestId = useRef(0);
  const cursor = pageCursors[page] ?? null;
  const input = useMemo(
    () => createOfficialStatusInput(filters, cursor, pageSize),
    [cursor, filters, pageSize],
  );
  const query = useActivityQuery(stationPublishedStatusOverviewQueryOptions(input, 60_000));
  const view = useMemo(() => buildOfficialStatusView(query.data), [query.data]);

  useEffect(() => {
    if (!query.data) return;
    setKnownTotal(query.data.summary.monitorTotal ?? 0);
    if (!query.data.page.nextCursor) return;
    setPageCursors((current) => current[page + 1] === query.data?.page.nextCursor
      ? current
      : { ...current, [page + 1]: query.data!.page.nextCursor });
  }, [page, query.data]);

  const resetPagination = useCallback(() => {
    paginationRequestId.current += 1;
    setJumpingPage(null);
    setPage(1);
    setPageCursors({ 1: null });
    setKnownTotal(0);
  }, []);

  const update = useCallback(<K extends keyof OfficialStatusFilters>(
    key: K,
    value: OfficialStatusFilters[K],
  ) => {
    resetPagination();
    setFilters((current) => ({ ...current, [key]: value }));
  }, [resetPagination]);

  const totalPages = Math.max(1, Math.ceil(knownTotal / pageSize));

  async function changePage(requestedPage: number) {
    const targetPage = Math.min(Math.max(1, Math.floor(requestedPage)), totalPages);
    if (targetPage === page) return;
    if (pageCursors[targetPage] !== undefined) {
      setPage(targetPage);
      return;
    }

    const requestId = paginationRequestId.current;
    const anchorPage = Math.max(
      ...Object.keys(pageCursors)
        .map(Number)
        .filter((candidate) => Number.isFinite(candidate) && candidate < targetPage),
    );
    let scanPage = anchorPage;
    let scanCursor = pageCursors[anchorPage] ?? null;
    const discovered: Record<number, string | null> = {};
    setJumpingPage(targetPage);

    try {
      while (scanPage < targetPage) {
        const result = await getStationPublishedStatusOverview(
          createOfficialStatusInput(filters, scanCursor, pageSize),
        );
        setKnownTotal(result.summary.monitorTotal ?? 0);
        if (!result.page.nextCursor) break;
        scanPage += 1;
        scanCursor = result.page.nextCursor;
        discovered[scanPage] = scanCursor;
      }

      if (requestId !== paginationRequestId.current) return;
      setPageCursors((current) => ({ ...current, ...discovered }));
      if (discovered[targetPage] !== undefined) setPage(targetPage);
    } catch (error) {
      if (requestId === paginationRequestId.current) {
        toast.error("跳转分页失败", readError(error));
      }
    } finally {
      if (requestId === paginationRequestId.current) setJumpingPage(null);
    }
  }

  function setPageSize(value: number) {
    const next = OFFICIAL_STATUS_PAGE_SIZE_OPTIONS.includes(
      value as (typeof OFFICIAL_STATUS_PAGE_SIZE_OPTIONS)[number],
    ) ? value : 100;
    setPageSizeState(next);
    resetPagination();
  }

  const startIndex = view.rows.length > 0 ? (page - 1) * pageSize + 1 : 0;
  const endIndex = view.rows.length > 0 ? startIndex + view.rows.length - 1 : 0;

  return {
    filters,
    setSearch: (value: string) => update("search", value),
    setStationId: (value: string) => update("stationId", value),
    setOutcome: (value: OfficialStatusFilters["outcome"]) => update("outcome", value),
    setSourceState: (value: OfficialStatusFilters["sourceState"]) => update("sourceState", value),
    page,
    pageSize,
    pageInfo: { currentPage: page, totalPages, startIndex, endIndex, total: knownTotal },
    changePage,
    setPageSize,
    paginationBusy: query.isFetching || jumpingPage !== null,
    input,
    query,
    view,
    refresh: () => query.refetch({ throwOnError: true }),
    invalidate: () => queryClient.invalidateQueries({ queryKey: queryKeys.stationPublishedStatusRoot }),
  };
}
