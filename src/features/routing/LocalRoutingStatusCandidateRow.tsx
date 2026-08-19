import type { DraggableAttributes, DraggableSyntheticListeners } from "@dnd-kit/core";
import { GripVertical } from "lucide-react";
import { createPortal } from "react-dom";
import { useEffect, useState, type ReactNode } from "react";
import { StatusBadge } from "@/components/ui";
import { toTimestampMillis } from "@/lib/time";
import type { RoutingCandidateView as LocalRoutingCandidate } from "@/lib/types/routingWorkspace";
import { cn } from "@/lib/utils";
import {
  buildCandidateDisplayFacts,
  buildCooldownDisplay,
} from "./localRoutingStatusViewModel";

type LocalRoutingStatusCandidateRowProps = {
  candidate: LocalRoutingCandidate;
  order: number;
  nowMs: number;
  dragDisabled?: boolean;
  dragAttributes?: DraggableAttributes;
  dragListeners?: DraggableSyntheticListeners;
};

export function LocalRoutingStatusCandidateHeader({ sortable = false }: { sortable?: boolean }) {
  return (
    <div className={cn(
      "hidden min-h-9 items-center gap-3 border-b border-border bg-surface-subtle px-3 py-2 text-[11px] font-medium text-muted-foreground md:grid",
      sortable
        ? "grid-cols-[24px_minmax(220px,1.6fr)_minmax(110px,.75fr)_minmax(88px,.55fr)_minmax(96px,.6fr)_minmax(80px,.5fr)_minmax(76px,.45fr)_minmax(72px,.45fr)]"
        : "grid-cols-[minmax(220px,1.6fr)_minmax(110px,.75fr)_minmax(88px,.55fr)_minmax(96px,.6fr)_minmax(80px,.5fr)_minmax(76px,.45fr)_minmax(72px,.45fr)]",
    )}>
      {sortable ? <span aria-hidden="true" /> : null}
      <span>候选密钥</span>
      <span className="text-center">参与状态</span>
      <span className="text-center">密钥评分</span>
      <span className="text-center">有效倍率</span>
      <span className="text-center">余额</span>
      <span className="text-center">冷却</span>
      <span className="text-center" title="每秒刷新">当前并发</span>
    </div>
  );
}

export function LocalRoutingStatusCandidateRow({
  candidate,
  order,
  nowMs,
  dragDisabled = false,
  dragAttributes,
  dragListeners,
}: LocalRoutingStatusCandidateRowProps) {
  const isSortable = Boolean(dragAttributes || dragListeners);
  const cooldownUntilMs =
    candidate.cooldownUntil == null ? null : toTimestampMillis(candidate.cooldownUntil);
  const cooldown = buildCooldownDisplay(candidate.healthState, cooldownUntilMs, nowMs);
  const displayFacts = buildCandidateDisplayFacts(candidate);
  const participationTone = !candidate.schedulable
    ? "disabled"
    : candidate.previewEligible
      ? "healthy"
      : "warning";
  const participationLabel = !candidate.schedulable
    ? "已暂停路由"
    : candidate.previewEligible
      ? "可参与"
      : "不参与";
  const [scoreDialogOpen, setScoreDialogOpen] = useState(false);

  return (
    <div className={cn(
      "grid min-h-[68px] gap-3 px-3 py-2.5 md:items-center",
      isSortable
        ? "md:grid-cols-[24px_minmax(220px,1.6fr)_minmax(110px,.75fr)_minmax(88px,.55fr)_minmax(96px,.6fr)_minmax(80px,.5fr)_minmax(76px,.45fr)_minmax(72px,.45fr)]"
        : "md:grid-cols-[minmax(220px,1.6fr)_minmax(110px,.75fr)_minmax(88px,.55fr)_minmax(96px,.6fr)_minmax(80px,.5fr)_minmax(76px,.45fr)_minmax(72px,.45fr)]",
    )}>
      {isSortable ? (
        <button
          type="button"
          aria-label="调整候选顺序"
          title="调整候选顺序"
          tabIndex={dragDisabled ? -1 : 0}
          disabled={dragDisabled}
          className={cn(
            "flex h-7 w-5 items-center justify-center self-start text-muted-foreground/45 md:self-center",
            dragDisabled ? "cursor-not-allowed" : "cursor-grab active:cursor-grabbing hover:text-muted-foreground",
          )}
          {...dragAttributes}
          {...dragListeners}
        >
          <GripVertical className="h-4 w-4" />
        </button>
      ) : null}
      <div className="min-w-0">
        <div className="flex items-center gap-2">
          <span className="text-xs font-semibold text-muted-foreground">#{order}</span>
          <span className="truncate text-[13px] font-semibold text-foreground">
            {candidate.keyName}
          </span>
        </div>
        <div className="mt-0.5 truncate text-xs text-muted-foreground">
          {candidate.stationName} · 聊天补全
        </div>
      </div>
      <MetricCell label="参与状态">
        <StatusBadge tone={participationTone}>{participationLabel}</StatusBadge>
        {!candidate.previewEligible && displayFacts.rejectReasonLabel ? (
          <div className="mt-1 text-xs text-warning-foreground">
            {displayFacts.rejectReasonLabel}
          </div>
        ) : null}
      </MetricCell>
      <MetricCell label="密钥评分">
        <button
          type="button"
          className={cn(
            "rounded px-1.5 py-0.5 text-[13px] font-semibold underline decoration-info-foreground/40 underline-offset-2 hover:bg-selected",
            candidate.score == null ? "text-muted-foreground" : "text-info-foreground",
          )}
          aria-label={`查看${candidate.keyName}的评分计算`}
          title="查看评分计算"
          onPointerDown={(event) => event.stopPropagation()}
          onPointerUp={(event) => event.stopPropagation()}
          onClick={() => setScoreDialogOpen(true)}
        >
          {formatCandidateScore(candidate.score)}
        </button>
      </MetricCell>
      <MetricCell label="有效倍率" value={displayFacts.multiplierLabel} detail={displayFacts.multiplierDetail} />
      <MetricCell
        label="余额"
        value={
          displayFacts.balanceUnit ? (
            <>
              <span className="text-success-foreground">{displayFacts.balanceAmountLabel}</span>
              {displayFacts.balanceUnit}
            </>
          ) : (
            displayFacts.balanceLabel
          )
        }
        detail={displayFacts.balanceDetail}
      />
      <MetricCell
        label="冷却"
        value={cooldown.label}
        tone={cooldown.active ? "warning" : "neutral"}
      />
      <MetricCell label="当前并发">
        <StatusBadge tone="disabled">
          {candidate.currentConcurrency == null ? "—" : String(candidate.currentConcurrency)}
        </StatusBadge>
      </MetricCell>
      <ScoreDetailsDialog
        open={scoreDialogOpen}
        keyName={candidate.keyName}
        details={candidate.scoreDetails}
        onClose={() => setScoreDialogOpen(false)}
      />
    </div>
  );
}

function ScoreDetailsDialog({
  open,
  keyName,
  details,
  onClose,
}: {
  open: boolean;
  keyName: string;
  details: LocalRoutingCandidate["scoreDetails"];
  onClose: () => void;
}) {
  useEffect(() => {
    if (!open) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onClose, open]);

  if (!open || typeof document === "undefined") return null;

  return createPortal(
    <div
      role="dialog"
      aria-modal="true"
      aria-label="密钥评分"
      className="fixed inset-0 z-50 flex items-center justify-center bg-scrim/45 p-4"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose();
      }}
    >
      <div className="max-h-[calc(100vh-32px)] w-full max-w-[620px] overflow-hidden rounded-[var(--surface-radius)] border border-border bg-surface shadow-dialog">
        <div className="flex items-center justify-between gap-4 border-b border-border px-5 py-4">
          <div className="min-w-0">
            <div className="truncate text-[15px] font-semibold text-foreground">密钥评分</div>
            <div className="mt-0.5 truncate text-xs text-muted-foreground">{keyName} · 当前策略评分</div>
          </div>
          <button
            type="button"
            className="flex h-8 w-8 items-center justify-center rounded-[var(--control-radius)] text-muted-foreground hover:bg-surface-subtle hover:text-foreground"
            aria-label="关闭"
            onClick={onClose}
          >
            <span aria-hidden="true" className="text-lg leading-none">×</span>
          </button>
        </div>
        <div className="max-h-[calc(100vh-180px)] overflow-auto">
          <ScoreBreakdown details={details} />
        </div>
      </div>
    </div>,
    document.body,
  );
}

function formatCandidateScore(score: number | null) {
  return score == null ? "—" : `${Math.round(score / 100)} 分`;
}

function ScoreBreakdown({
  details,
}: {
  details: LocalRoutingCandidate["scoreDetails"];
}) {
  if (!details) {
    return <div className="p-4 text-sm text-muted-foreground">暂无评分明细。</div>;
  }

  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const factors = [
    {
      key: "reliability",
      label: "可靠性",
      factor: details.reliability,
      summary: "根据历史成功率计算，样本较少时会加入先验平滑。",
      formula: "窗口分：(成功次数 + 先验成功) / (成功次数 + 失败次数 + 先验总数)；最终分：近24小时分 × 近期权重 + 历史分 × 历史权重",
    },
    {
      key: "responsiveness",
      label: "响应速度",
      factor: details.responsiveness,
      summary: "分别计算近期和历史 P95 延迟，再按样本量混合。",
      formula: "窗口速度分：max(0, 1 - 最近延迟 / 延迟上限)（最近延迟取窗口 P95）；最终分：近24小时速度分 × 近期权重 + 历史速度分 × 历史权重",
    },
    {
      key: "cost",
      label: "成本",
      factor: details.cost,
      summary: "根据密钥的有效倍率计算；倍率越低代表请求成本越低。",
      formula: "1 / (1 + 密钥有效倍率)",
    },
    {
      key: "preference",
      label: "偏好",
      factor: details.preference,
      summary: "根据候选基础优先级和策略修正计算。",
      formula: "10,000 - 候选优先级，再应用策略修正",
    },
  ] as const;

  return (
    <div className="grid gap-4 p-5 text-sm">
      <section className="border-b border-border pb-4" aria-labelledby="score-summary-title">
        <div id="score-summary-title" className="text-xs font-medium text-muted-foreground">最终评分</div>
        <div className="mt-1 text-2xl font-semibold tabular-nums text-info-foreground">{formatCandidateScore(details.total)}</div>
        <div className="mt-2 text-xs leading-5 text-muted-foreground">
          主要贡献：{factors.map(({ label, factor }) => `${label} ${formatContribution(factor.contribution)}`).join(" · ")}
        </div>
      </section>

      <section className="grid gap-3" aria-labelledby="score-breakdown-title">
        <div>
          <h3 id="score-breakdown-title" className="text-sm font-semibold text-foreground">评分构成</h3>
          <p className="mt-0.5 text-xs text-muted-foreground">因子分 × 权重 = 对最终评分的贡献</p>
        </div>
        <div className="grid grid-cols-[minmax(120px,1fr)_68px_58px_68px] gap-2 border-b border-border pb-1 text-[11px] text-muted-foreground">
          <span>评分因子</span>
          <span className="text-right">因子分</span>
          <span className="text-right">权重</span>
          <span className="text-right">贡献</span>
        </div>
        {factors.map(({ key, label, factor, summary, formula }) => {
          const isExpanded = Boolean(expanded[key]);
          return (
            <div key={key} className="grid gap-2 border-b border-border/70 pb-3 last:border-b-0 last:pb-0">
              <div className="grid grid-cols-[minmax(120px,1fr)_68px_58px_68px] items-baseline gap-2 text-[13px] tabular-nums">
                <span className="font-medium text-foreground">{label}</span>
                <span className="text-right text-muted-foreground">{formatBasisPoints(factor.score)}</span>
                <span className="text-right text-muted-foreground">{formatBasisPoints(factor.weight)}</span>
                <span className="text-right font-semibold text-foreground">{formatContribution(factor.contribution)}</span>
              </div>
              <div className="h-1 overflow-hidden rounded-full bg-surface-subtle" role="progressbar" aria-label={`${label}因子分`} aria-valuemin={0} aria-valuemax={100} aria-valuenow={factor.score / 100}>
                <div className="h-full rounded-full bg-info-foreground/70" style={{ width: `${Math.min(100, factor.score / 100)}%` }} />
              </div>
              <p className="text-xs leading-4 text-muted-foreground">{summary}</p>
              <div className="text-xs text-muted-foreground">{summaryInputs(label, factor)}</div>
              {(key === "reliability" || key === "responsiveness") && factor.windowDetails ? (
                <ScoreWindowDetails kind={key} details={factor.windowDetails} />
              ) : null}
              <button
                type="button"
                className="w-fit text-xs font-medium text-info-foreground hover:underline"
                aria-expanded={isExpanded}
                onClick={() => setExpanded((current) => ({ ...current, [key]: !isExpanded }))}
              >
                {isExpanded ? "收起计算详情" : "查看计算详情"}
              </button>
              {isExpanded ? (
                <div className="grid gap-2 rounded-[var(--control-radius)] border border-border bg-surface-subtle px-3 py-2 text-xs">
                  <div className="font-medium text-foreground">计算详情</div>
                  <div><span className="text-muted-foreground">公式：</span>{formula}</div>
                  <div className="grid gap-1">
                    <div className="text-muted-foreground">输入参数</div>
                    {factor.inputs.length > 0 ? factor.inputs.map((input) => (
                      <div key={input.label} className="flex items-baseline justify-between gap-3">
                        <span>{input.label}</span>
                        <span className="font-medium tabular-nums text-foreground">{input.value}</span>
                      </div>
                    )) : <span className="text-muted-foreground">暂无输入数据</span>}
                  </div>
                  <div className="flex items-baseline justify-between border-t border-border pt-1">
                    <span className="text-muted-foreground">计算结果</span>
                    <span className="font-semibold tabular-nums text-foreground">{formatBasisPoints(factor.score)}</span>
                  </div>
                </div>
              ) : null}
            </div>
          );
        })}
      </section>
    </div>
  );
}

function ScoreWindowDetails({
  kind,
  details,
}: {
  kind: "reliability" | "responsiveness";
  details: NonNullable<NonNullable<LocalRoutingCandidate["scoreDetails"]>["reliability"]["windowDetails"]>;
}) {
  const isReliability = kind === "reliability";
  return (
    <div className="grid gap-2 rounded-[var(--control-radius)] border border-border/70 bg-surface-subtle px-3 py-2 text-xs">
      <div className="font-medium text-foreground">窗口明细</div>
      <div className="grid gap-2 md:grid-cols-2">
        <WindowColumn
          title="近24小时"
          count={details.recentObservationCount}
          effectiveMass={details.recentEffectiveMassBasisPoints}
          score={isReliability ? details.recentScore : details.recentResponsivenessBasisPoints}
          weight={isReliability ? details.recentWeightBasisPoints : details.recentResponsivenessWeightBasisPoints}
          success={isReliability ? details.recentSuccessMassBasisPoints : undefined}
          failure={isReliability ? details.recentFailureMassBasisPoints : undefined}
          p95={isReliability ? undefined : details.recentP95LatencyMs}
        />
        <WindowColumn
          title={`历史基线（${details.historicalAgeWindowDays}天，${details.historicalHalfLifeDays}天半衰）`}
          count={details.historicalObservationCount}
          effectiveMass={details.historicalEffectiveMassBasisPoints}
          score={isReliability ? details.historicalScore : details.historicalResponsivenessBasisPoints}
          weight={isReliability ? details.historicalWeightBasisPoints : details.historicalResponsivenessWeightBasisPoints}
          success={isReliability ? details.historicalSuccessMassBasisPoints : undefined}
          failure={isReliability ? details.historicalFailureMassBasisPoints : undefined}
          p95={isReliability ? undefined : details.historicalP95LatencyMs}
        />
      </div>
      <div className="border-t border-border pt-1 text-muted-foreground">
        最终分 = {formatBasisPoints(isReliability ? details.recentScore : details.recentResponsivenessBasisPoints)} × {formatBasisPoints(isReliability ? details.recentWeightBasisPoints : details.recentResponsivenessWeightBasisPoints)} + {formatBasisPoints(isReliability ? details.historicalScore : details.historicalResponsivenessBasisPoints)} × {formatBasisPoints(isReliability ? details.historicalWeightBasisPoints : details.historicalResponsivenessWeightBasisPoints)}
      </div>
    </div>
  );
}

function WindowColumn({
  title,
  count,
  effectiveMass,
  score,
  weight,
  success,
  failure,
  p95,
}: {
  title: string;
  count: number;
  effectiveMass: number;
  score: number;
  weight: number;
  success?: number;
  failure?: number;
  p95?: number | null;
}) {
  return (
    <div className="grid gap-0.5">
      <div className="font-medium text-foreground">{title}</div>
      <div className="flex justify-between gap-2"><span>样本</span><span className="tabular-nums text-foreground">{count}</span></div>
      <div className="flex justify-between gap-2"><span>有效样本（衰减后）</span><span className="tabular-nums text-foreground">{formatMass(effectiveMass)}</span></div>
      {success !== undefined && failure !== undefined ? (
        <div className="flex justify-between gap-2"><span>成功 / 失败</span><span className="tabular-nums text-foreground">{formatMass(success)} / {formatMass(failure)}</span></div>
      ) : null}
      {p95 !== undefined ? (
        <div className="flex justify-between gap-2"><span>P95 延迟</span><span className="tabular-nums text-foreground">{p95 == null ? "暂无" : formatLatency(p95)}</span></div>
      ) : null}
      <div className="flex justify-between gap-2"><span>窗口分 / 采用权重</span><span className="tabular-nums font-medium text-foreground">{formatBasisPoints(score)} / {formatBasisPoints(weight)}</span></div>
    </div>
  );
}

function formatMass(value: number) {
  return `${(value / 10_000).toFixed(value % 10_000 === 0 ? 0 : 1)} 次`;
}

function formatLatency(value: number) {
  return value >= 1000 ? `${(value / 1000).toFixed(2).replace(/\.00$/, "")} s` : `${value} ms`;
}

function formatBasisPoints(value: number) {
  const percent = value / 100;
  return `${Number.isInteger(percent) ? percent : percent.toFixed(1)}%`;
}

function formatContribution(value: number) {
  return `+${(value / 100).toFixed(1)}`;
}

function summaryInputs(
  label: string,
  factor: NonNullable<LocalRoutingCandidate["scoreDetails"]>["reliability"],
) {
  if (label === "成本") {
    const multiplier = factor.inputs.find((input) => input.label === "密钥有效倍率")?.value;
    const hasMultiplier = Boolean(multiplier && multiplier !== "暂无数据");
    if (!hasMultiplier) {
      return <span className="text-warning-foreground">密钥倍率暂不可用，当前采用默认中性分 {formatBasisPoints(factor.score)}。</span>;
    }
    return (
      <span>
        密钥有效倍率 {multiplier} · 采用倍率代理分 {formatBasisPoints(factor.score)}
      </span>
    );
  }

  return factor.inputs.length > 0
    ? factor.inputs
        .map((input) => `${summaryInputLabel(label, input.label)} ${summaryInputValue(input.label, input.value)}`)
        .join(" · ")
    : "暂无统计数据";
}

function summaryInputLabel(factorLabel: string, inputLabel: string) {
  if (factorLabel === "偏好" && inputLabel === "候选优先级") return "基础优先级";
  return inputLabel;
}

function summaryInputValue(label: string, value: string) {
  if (!["最近平均延迟", "延迟上限"].includes(label)) return value;
  const milliseconds = Number.parseFloat(value);
  if (!Number.isFinite(milliseconds)) return value;
  if (milliseconds >= 1000) {
    return `${(milliseconds / 1000).toFixed(2).replace(/\.00$/, "")} s`;
  }
  return `${milliseconds} ms`;
}

function MetricCell({
  label,
  value,
  detail,
  tone = "neutral",
  children,
}: {
  label: string;
  value?: ReactNode;
  detail?: ReactNode;
  tone?: "neutral" | "warning";
  children?: ReactNode;
}) {
  return (
    <div className="min-w-0 md:flex md:flex-col md:items-center md:text-center">
      <div className="text-[11px] text-muted-foreground md:hidden">{label}</div>
      {children ?? (
        <div
          className={
            tone === "warning"
              ? "text-[13px] font-semibold text-warning-foreground"
              : "text-[13px] font-semibold text-foreground"
          }
        >
          {value}
        </div>
      )}
      {detail ? <div className="truncate text-[11px] text-muted-foreground">{detail}</div> : null}
    </div>
  );
}
