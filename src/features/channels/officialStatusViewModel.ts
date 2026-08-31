import type { StationPublishedStatusOverview, StationPublishedStatusOverviewInput, StationPublishedStatusOverviewRow, StationPublishedStatusOutcome, StationPublishedStatusSourceState } from "@/lib/types/stationPublishedStatus";
import type { StatusTrendCell, StatusTrendTone } from "@/components/status/StatusTrend";

export type OfficialStatusFilters = { search: string; stationId: string; outcome: "all" | StationPublishedStatusOutcome; sourceState: "all" | StationPublishedStatusSourceState };
export const defaultOfficialStatusFilters: OfficialStatusFilters = { search: "", stationId: "", outcome: "all", sourceState: "all" };
export type OfficialStatusRowView = StationPublishedStatusOverviewRow & { trend: StatusTrendCell[]; currentLabel: string; sourceStateLabel: string; availabilityLabel: string; lastCheckedLabel: string };
export type OfficialStatusView = { rows: OfficialStatusRowView[]; nextCursor: string | null; summary: Record<string, number>; readAtMs: number | null };

export function createOfficialStatusInput(
  filters: OfficialStatusFilters,
  cursor: string | null = null,
  limit = 100,
): StationPublishedStatusOverviewInput {
  return { filter: { search: filters.search.trim() || null, stationId: filters.stationId || null, outcome: filters.outcome === "all" ? null : filters.outcome, sourceState: filters.sourceState === "all" ? null : filters.sourceState }, cursor, limit };
}
export function buildOfficialStatusView(data: StationPublishedStatusOverview | undefined): OfficialStatusView {
  if (!data) return { rows: [], nextCursor: null, summary: {}, readAtMs: null };
  return { rows: data.rows.map(toRow), nextCursor: data.page.nextCursor, summary: data.summary, readAtMs: data.readAtMs };
}
function toRow(row: StationPublishedStatusOverviewRow): OfficialStatusRowView {
  return {
    ...row,
    currentLabel: outcomeLabel(row.currentOutcome),
    sourceStateLabel: sourceStateLabel(row.sourceState),
    availabilityLabel: row.recentAvailabilityPercent == null ? "--" : `${row.recentAvailabilityPercent.toFixed(2)}%`,
    lastCheckedLabel: formatTime(row.upstreamCheckedAtMs),
    trend: [...row.recentSamples].reverse().map((sample, i) => ({
      id: sample.id || `${row.rowKey}-${i}`,
      tone: sampleTone(sample.outcome),
      label: outcomeLabel(sample.outcome),
      modelLabel: sample.model,
      timeLabel: formatTime(sample.checkedAtMs),
      availabilityLabel: outcomeLabel(sample.outcome),
      latencyLabel: sample.latencyMs == null ? "--" : `${sample.latencyMs} ms`,
    })),
  };
}
export function sourceStateLabel(value: StationPublishedStatusSourceState): string {
  return value === "available" ? "正常" : value === "degraded" ? "部分解析" : value === "failed" ? "失败" : value === "authorization_required" ? "需要授权" : "未知";
}
function sampleTone(value: StationPublishedStatusOutcome): StatusTrendTone { return value === "available" || value === "degraded" || value === "unavailable" ? value : "missing"; }
function outcomeLabel(value: StationPublishedStatusOutcome) { return value === "available" ? "正常" : value === "degraded" ? "降级" : value === "unavailable" ? "错误" : "未知"; }
function formatTime(value: number | null) { return value == null ? "--" : new Date(value).toLocaleString("zh-CN", { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" }); }
