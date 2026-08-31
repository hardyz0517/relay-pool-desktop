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

type MathMlIntrinsicProps = Record<string, unknown>;

declare global {
  namespace JSX {
    interface IntrinsicElements {
      math: MathMlIntrinsicProps;
      mrow: MathMlIntrinsicProps;
      msub: MathMlIntrinsicProps;
      msup: MathMlIntrinsicProps;
      msubsup: MathMlIntrinsicProps;
      mi: MathMlIntrinsicProps;
      mn: MathMlIntrinsicProps;
      mo: MathMlIntrinsicProps;
      mfrac: MathMlIntrinsicProps;
      mspace: MathMlIntrinsicProps;
      mtable: MathMlIntrinsicProps;
      mtr: MathMlIntrinsicProps;
      mtd: MathMlIntrinsicProps;
    }
  }
}

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
  const scoreStatus = candidate.scoreStatus;
  const participationTone = !candidate.schedulable || scoreStatus === "unavailable"
    ? "disabled"
    : scoreStatus === "scored"
      ? "healthy"
      : "warning";
  const participationLabel = !candidate.schedulable
    ? "已暂停路由"
    : scoreStatusLabel(scoreStatus);
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
        {scoreStatus !== "scored" && displayFacts.rejectReasonLabel ? (
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
          disabled={scoreStatus !== "scored"}
          onPointerDown={(event) => event.stopPropagation()}
          onPointerUp={(event) => event.stopPropagation()}
          onClick={() => {
            if (scoreStatus === "scored") setScoreDialogOpen(true);
          }}
        >
          {formatCandidateScore(candidate.score, scoreStatus)}
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
        <StatusBadge
          tone={candidate.currentConcurrency != null && candidate.currentConcurrency > 0 ? "healthy" : "disabled"}
          className="rounded-[4px]"
        >
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

function scoreStatusLabel(status: LocalRoutingCandidate["scoreStatus"]) {
  switch (status) {
    case "scored": return "可参与";
    case "candidate_limit": return "候选上限外";
    case "probe_discovery": return "仅恢复探测";
    case "unavailable": return "评分暂不可用";
    default: return "未进入评分";
  }
}

function formatCandidateScore(
  score: number | null,
  status: LocalRoutingCandidate["scoreStatus"],
) {
  if (status !== "scored") return scoreStatusLabel(status);
  return score == null ? "—" : `${Math.round(score / 100)} 分`;
}

export function ScoreBreakdown({
  details,
}: {
  details: LocalRoutingCandidate["scoreDetails"];
}) {
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});

  if (!details) {
    return <div className="p-4 text-sm text-muted-foreground">暂无评分明细。</div>;
  }

  const factors = [
    {
      key: "reliability",
      label: "可靠性",
      factor: details.reliability,
      formula: <ReliabilityFormula />,
    },
    {
      key: "responsiveness",
      label: "响应速度",
      factor: details.responsiveness,
      formula: <ResponsivenessFormula />,
    },
    {
      key: "cost",
      label: "成本",
      factor: details.cost,
      formula: <CostFormula />,
    },
    {
      key: "preference",
      label: "偏好",
      factor: details.preference,
      formula: <PreferenceFormula />,
    },
  ] as const;

  return (
    <div className="grid gap-4 p-5 text-sm">
      <section className="border-b border-border pb-4" aria-labelledby="score-summary-title">
        <div id="score-summary-title" className="text-xs font-medium text-muted-foreground">最终评分</div>
        <div className="mt-1 text-2xl font-semibold tabular-nums text-info-foreground">{Math.round(details.total / 100)} 分</div>
        <div className="mt-2 text-xs leading-5 text-muted-foreground">
          主要贡献：{factors.map(({ label, factor }) => `${label} ${formatContribution(factor.contribution)}`).join(" · ")}
        </div>
      </section>

      <section className="grid gap-3" aria-labelledby="score-breakdown-title">
        <div>
          <h3 id="score-breakdown-title" className="text-sm font-semibold text-foreground">评分构成</h3>
        </div>
        <div className="grid grid-cols-[minmax(120px,1fr)_68px_58px_68px] gap-2 border-b border-border pb-1 text-[11px] text-muted-foreground">
          <span>评分因子</span>
          <span className="text-right">因子分</span>
          <span className="text-right">权重</span>
          <span className="text-right">贡献</span>
        </div>
        {factors.map(({ key, label, factor, formula }) => {
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
              {!isExpanded ? (
                <div className="text-xs leading-4 text-muted-foreground">{factorSummary(label, factor)}</div>
              ) : null}
              <button
                type="button"
                className="w-fit text-xs font-medium text-info-foreground hover:underline"
                aria-expanded={isExpanded}
                onClick={() => setExpanded((current) => ({ ...current, [key]: !isExpanded }))}
              >
                {isExpanded ? "收起详情" : "查看详情"}
              </button>
              {isExpanded ? (
                <div className="grid gap-3 rounded-[var(--control-radius)] border border-border bg-surface-subtle px-3 py-2 text-xs">
                  <div className="font-medium text-foreground">计算详情</div>
                  <FormulaBlock>{formula}</FormulaBlock>
                  {key === "reliability" || key === "responsiveness" ? (
                    factor.windowDetails ? (
                      <ScoreWindowDetails kind={key} details={factor.windowDetails} />
                    ) : (
                      <UnavailableScoreWindowDetails />
                    )
                  ) : (
                    <div className="grid gap-1">
                      <div className="text-muted-foreground">输入参数</div>
                      {factor.inputs.length > 0 ? factor.inputs.map((input) => (
                        <div key={input.label} className="flex items-baseline justify-between gap-3">
                          <span className="whitespace-nowrap">{input.label}</span>
                          <span className="whitespace-nowrap font-medium tabular-nums text-foreground">{input.value}</span>
                        </div>
                      )) : <span className="text-muted-foreground">暂无输入数据</span>}
                    </div>
                  )}
                  <div className="flex items-baseline justify-between border-t border-border pt-2">
                    <span className="text-muted-foreground">计算结果</span>
                    <span className="font-semibold tabular-nums text-info-foreground">{formatBasisPoints(factor.score)}</span>
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

function FormulaBlock({ children }: { children: ReactNode }) {
  return (
    <div className="overflow-x-auto rounded-[var(--control-radius)] border border-border/70 bg-surface px-2.5 py-2.5">
      <div className="grid min-w-max gap-2 text-[15px] leading-7 text-foreground [font-family:'STIX_Two_Math','Cambria_Math','Times_New_Roman',serif]">
        {children}
      </div>
    </div>
  );
}

function FormulaLine({ children, ariaLabel }: { children: ReactNode; ariaLabel: string }) {
  return (
    <math xmlns="http://www.w3.org/1998/Math/MathML" display="block" aria-label={ariaLabel}>
      <mrow>{children}</mrow>
    </math>
  );
}

function ReliabilityFormula() {
  return (
    <>
      <FormulaLine ariaLabel="可靠性窗口加权成功率">
        <msub><mi>R</mi><mrow><mi>s</mi><mo>,</mo><mi>ω</mi></mrow></msub>
        <mo>=</mo>
        <mfrac>
          <mrow>
            <msub><mo>Σ</mo><mi>i</mi></msub>
            <mo>(</mo><msub><mi>w</mi><mi>i</mi></msub><mo>·</mo><msub><mi>s</mi><mi>i</mi></msub><mo>)</mo>
          </mrow>
          <mrow><msub><mo>Σ</mo><mi>i</mi></msub><msub><mi>w</mi><mi>i</mi></msub></mrow>
        </mfrac>
        <mspace width="1em" />
        <mo>(</mo><msub><mi>n</mi><mrow><mi>s</mi><mo>,</mo><mi>ω</mi></mrow></msub><mo>≥</mo><msub><mi>n</mi><mrow><mi>min</mi><mo>,</mo><mi>ω</mi></mrow></msub><mo>)</mo>
      </FormulaLine>
      <FormulaLine ariaLabel="样本不足时采用乐观可靠性">
        <msub><mi>R</mi><mrow><mi>s</mi><mo>,</mo><mi>ω</mi></mrow></msub>
        <mo>=</mo>
        <msub><mi>R</mi><mi>opt</mi></msub>
        <mspace width="1em" />
        <mo>(</mo><msub><mi>n</mi><mrow><mi>s</mi><mo>,</mo><mi>ω</mi></mrow></msub><mo>&lt;</mo><msub><mi>n</mi><mrow><mi>min</mi><mo>,</mo><mi>ω</mi></mrow></msub><mo>)</mo>
      </FormulaLine>
      <FormulaLine ariaLabel="近期可靠性置信度">
        <msub><mi>c</mi><mi>s</mi></msub>
        <mo>=</mo>
        <mi>min</mi><mo>(</mo><mn>0.9</mn><mo>,</mo>
        <mfrac>
          <msub><mi>n</mi><mrow><mi>s</mi><mo>,</mo><mn>24</mn><mi>h</mi></mrow></msub>
          <mrow><msub><mi>n</mi><mrow><mi>s</mi><mo>,</mo><mn>24</mn><mi>h</mi></mrow></msub><mo>+</mo><mn>20</mn></mrow>
        </mfrac>
        <mo>)</mo>
      </FormulaLine>
      <FormulaLine ariaLabel="近期和历史可靠性融合">
        <mi>R</mi><mo>=</mo><msub><mo>Σ</mo><mi>s</mi></msub><msub><mi>λ</mi><mi>s</mi></msub><mo>·</mo>
        <mo>[</mo><msub><mi>c</mi><mi>s</mi></msub><mo>·</mo><msub><mi>R</mi><mrow><mi>s</mi><mo>,</mo><mn>24</mn><mi>h</mi></mrow></msub>
        <mo>+</mo><mo>(</mo><mn>1</mn><mo>−</mo><msub><mi>c</mi><mi>s</mi></msub><mo>)</mo><mo>·</mo><msub><mi>R</mi><mrow><mi>s</mi><mo>,</mo><mi>hist</mi></mrow></msub><mo>]</mo>
      </FormulaLine>
      <FormulaLine ariaLabel="样本时间衰减权重">
        <msub><mi>w</mi><mi>i</mi></msub><mo>=</mo><mi>w</mi><mo>(</mo><msub><mi>a</mi><mi>i</mi></msub><mo>)</mo><mo>=</mo>
        <mo stretchy="true">{"{"}</mo>
        <mtable rowspacing="0.45em" columnalign="left">
          <mtr>
            <mtd><msup><mn>2</mn><mrow><mo>−</mo><mfrac><msub><mi>a</mi><mi>i</mi></msub><mn>72</mn></mfrac></mrow></msup></mtd>
            <mtd><mrow><mn>0</mn><mo>≤</mo><msub><mi>a</mi><mi>i</mi></msub><mo>≤</mo><mn>24</mn></mrow></mtd>
          </mtr>
          <mtr>
            <mtd><msup><mn>2</mn><mrow><mo>−</mo><mfrac><mn>24</mn><mn>72</mn></mfrac></mrow></msup><mo>·</mo><msup><mn>2</mn><mrow><mo>−</mo><mfrac><mrow><msub><mi>a</mi><mi>i</mi></msub><mo>−</mo><mn>24</mn></mrow><mn>24</mn></mfrac></mrow></msup></mtd>
            <mtd><mrow><msub><mi>a</mi><mi>i</mi></msub><mo>&gt;</mo><mn>24</mn></mrow></mtd>
          </mtr>
        </mtable>
      </FormulaLine>
      <FormulaLine ariaLabel="成功状态取零或一">
        <msub><mi>s</mi><mi>i</mi></msub><mo>∈</mo><mo>{"{"}</mo><mn>0</mn><mo>,</mo><mn>1</mn><mo>{"}"}</mo>
      </FormulaLine>
    </>
  );
}

function ResponsivenessFormula() {
  return (
    <>
      <FormulaLine ariaLabel="响应速度窗口加权延迟">
        <msub><mi>L</mi><mrow><mi>s</mi><mo>,</mo><mi>ω</mi></mrow></msub><mo>=</mo>
        <mfrac>
          <mrow><msub><mo>Σ</mo><mi>i</mi></msub><mo>(</mo><msub><mi>w</mi><mi>i</mi></msub><mo>·</mo><msub><mi>l</mi><mi>i</mi></msub><mo>)</mo></mrow>
          <mrow><msub><mo>Σ</mo><mi>i</mi></msub><msub><mi>w</mi><mi>i</mi></msub></mrow>
        </mfrac>
        <mspace width="1em" /><mo>(</mo><msubsup><mi>n</mi><mrow><mi>s</mi><mo>,</mo><mi>ω</mi></mrow><mi>L</mi></msubsup><mo>≥</mo><msub><mi>n</mi><mrow><mi>min</mi><mo>,</mo><mi>ω</mi></mrow></msub><mo>)</mo>
      </FormulaLine>
      <FormulaLine ariaLabel="样本不足时采用乐观延迟">
        <msub><mi>L</mi><mrow><mi>s</mi><mo>,</mo><mi>ω</mi></mrow></msub><mo>=</mo><msub><mi>L</mi><mi>opt</mi></msub>
        <mspace width="1em" /><mo>(</mo><msubsup><mi>n</mi><mrow><mi>s</mi><mo>,</mo><mi>ω</mi></mrow><mi>L</mi></msubsup><mo>&lt;</mo><msub><mi>n</mi><mrow><mi>min</mi><mo>,</mo><mi>ω</mi></mrow></msub><mo>)</mo>
      </FormulaLine>
      <FormulaLine ariaLabel="近期响应速度置信度">
        <msubsup><mi>c</mi><mi>s</mi><mi>L</mi></msubsup><mo>=</mo><mi>min</mi><mo>(</mo><mn>0.9</mn><mo>,</mo>
        <mfrac><msubsup><mi>n</mi><mrow><mi>s</mi><mo>,</mo><mn>24</mn><mi>h</mi></mrow><mi>L</mi></msubsup><mrow><msubsup><mi>n</mi><mrow><mi>s</mi><mo>,</mo><mn>24</mn><mi>h</mi></mrow><mi>L</mi></msubsup><mo>+</mo><mn>20</mn></mrow></mfrac><mo>)</mo>
      </FormulaLine>
      <FormulaLine ariaLabel="近期和历史响应速度融合">
        <mi>L</mi><mo>=</mo><msub><mo>Σ</mo><mi>s</mi></msub><msub><mi>λ</mi><mi>s</mi></msub><mo>·</mo><mo>[</mo>
        <msubsup><mi>c</mi><mi>s</mi><mi>L</mi></msubsup><mo>·</mo><msub><mi>L</mi><mrow><mi>s</mi><mo>,</mo><mn>24</mn><mi>h</mi></mrow></msub><mo>+</mo>
        <mo>(</mo><mn>1</mn><mo>−</mo><msubsup><mi>c</mi><mi>s</mi><mi>L</mi></msubsup><mo>)</mo><mo>·</mo><msub><mi>L</mi><mrow><mi>s</mi><mo>,</mo><mi>hist</mi></mrow></msub><mo>]</mo>
      </FormulaLine>
      <FormulaLine ariaLabel="响应速度因子换算">
        <mi>V</mi><mo>=</mo><mo>⌊</mo><mfrac><mrow><msup><mn>10</mn><mn>4</mn></msup><mo>·</mo><mo>(</mo><mn>120000</mn><mo>−</mo><mi>min</mi><mo>(</mo><mi>L</mi><mo>,</mo><mn>120000</mn><mo>)</mo><mo>)</mo></mrow><mn>120000</mn></mfrac><mo>⌋</mo>
      </FormulaLine>
      <FormulaLine ariaLabel="样本时间衰减权重">
        <msub><mi>w</mi><mi>i</mi></msub><mo>=</mo><mi>w</mi><mo>(</mo><msub><mi>a</mi><mi>i</mi></msub><mo>)</mo>
      </FormulaLine>
    </>
  );
}

function CostFormula() {
  return (
    <FormulaLine ariaLabel="成本因子换算">
      <mi>C</mi><mo>(</mo><mi>m</mi><mo>)</mo><mo>=</mo><mo>⌊</mo>
      <mfrac><mrow><msup><mn>10</mn><mn>4</mn></msup><mo>·</mo><msup><mn>10</mn><mn>6</mn></msup></mrow><mrow><msup><mn>10</mn><mn>6</mn></msup><mo>+</mo><mi>round</mi><mo>(</mo><msup><mn>10</mn><mn>6</mn></msup><mo>·</mo><mi>m</mi><mo>)</mo></mrow></mfrac>
      <mo>⌋</mo>
    </FormulaLine>
  );
}

function PreferenceFormula() {
  return (
    <FormulaLine ariaLabel="优先级因子换算">
      <mi>P</mi><mo>(</mo><mi>p</mi><mo>)</mo><mo>=</mo><msup><mn>10</mn><mn>4</mn></msup><mo>−</mo><mi>clamp</mi><mo>(</mo><mi>p</mi><mo>,</mo><mn>0</mn><mo>,</mo><msup><mn>10</mn><mn>4</mn></msup><mo>)</mo>
    </FormulaLine>
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
  const groups = buildWindowComparisonGroups(kind, details);
  return (
    <div className="grid gap-2">
      <div className="font-medium text-foreground">窗口明细</div>
      <WindowComparison
        groups={groups}
        historicalSubtitle={`24 小时以前，${details.historicalAgeWindowDays} 天窗口`}
      />
      <div className="break-words border-t border-border bg-surface-subtle/60 px-2 py-2 text-xs font-medium leading-5 text-muted-foreground">
        <span className="text-foreground">最终分</span> = {formatBasisPoints(isReliability ? details.recentScore : details.recentResponsivenessBasisPoints)} × {formatBasisPoints(isReliability ? details.recentWeightBasisPoints : details.recentResponsivenessWeightBasisPoints)} + {formatBasisPoints(isReliability ? details.historicalScore : details.historicalResponsivenessBasisPoints)} × {formatBasisPoints(isReliability ? details.historicalWeightBasisPoints : details.historicalResponsivenessWeightBasisPoints)}
      </div>
      {isReliability ? (
        <MonitoringSampleStatus
          status={details.monitoringSourceStatus}
          hasSamples={details.recentMonitoringSampleCount + details.historicalMonitoringSampleCount > 0}
        />
      ) : (
        <MonitoringSampleStatus
          status={details.monitoringSourceStatus}
          hasSamples={details.recentMonitoringLatencySampleCount + details.historicalMonitoringLatencySampleCount > 0}
          messagePrefix="响应速度"
        />
      )}
    </div>
  );
}

type ScoreWindowDetailsSnapshot = NonNullable<NonNullable<LocalRoutingCandidate["scoreDetails"]>["reliability"]["windowDetails"]>;

type WindowComparisonRow = {
  label: string;
  recent: ReactNode;
  historical: ReactNode;
  wrapValues?: boolean;
};

type WindowComparisonGroup = {
  label: string;
  rows: WindowComparisonRow[];
};

function buildWindowComparisonGroups(
  kind: "reliability" | "responsiveness",
  details: ScoreWindowDetailsSnapshot,
): WindowComparisonGroup[] {
  const isReliability = kind === "reliability";
  const recent = {
    count: isReliability ? details.recentObservationCount : details.recentLatencySampleCount,
    realSamples: isReliability ? details.recentRealSampleCount : details.recentRealLatencySampleCount,
    monitoringSamples: isReliability ? details.recentMonitoringSampleCount : details.recentMonitoringLatencySampleCount,
    realMinimumMet: isReliability ? details.recentReliabilityMinimumMet : details.recentRealLatencyMinimumMet,
    monitoringMinimumMet: isReliability ? details.recentMonitoringSampleCount >= details.recentMinimumSamples : details.recentMonitoringLatencyMinimumMet,
    effectiveMass: isReliability ? details.recentEffectiveMassBasisPoints : details.recentLatencyEffectiveMassBasisPoints,
    score: isReliability ? details.recentScore : details.recentResponsivenessBasisPoints,
    weight: isReliability ? details.recentWeightBasisPoints : details.recentResponsivenessWeightBasisPoints,
    success: details.recentSuccessMassBasisPoints,
    failure: details.recentFailureMassBasisPoints,
    weightedLatency: details.recentWeightedLatencyMs,
    realWeightedLatency: details.recentRealWeightedLatencyMs,
    monitoringWeightedLatency: details.recentMonitoringWeightedLatencyMs,
  };
  const historical = {
    count: isReliability ? details.historicalObservationCount : details.historicalLatencySampleCount,
    realSamples: isReliability ? details.historicalRealSampleCount : details.historicalRealLatencySampleCount,
    monitoringSamples: isReliability ? details.historicalMonitoringSampleCount : details.historicalMonitoringLatencySampleCount,
    realMinimumMet: isReliability ? details.historicalReliabilityMinimumMet : details.historicalRealLatencyMinimumMet,
    monitoringMinimumMet: isReliability ? details.historicalMonitoringSampleCount >= details.historicalMinimumSamples : details.historicalMonitoringLatencyMinimumMet,
    effectiveMass: isReliability ? details.historicalEffectiveMassBasisPoints : details.historicalLatencyEffectiveMassBasisPoints,
    score: isReliability ? details.historicalScore : details.historicalResponsivenessBasisPoints,
    weight: isReliability ? details.historicalWeightBasisPoints : details.historicalResponsivenessWeightBasisPoints,
    success: details.historicalSuccessMassBasisPoints,
    failure: details.historicalFailureMassBasisPoints,
    weightedLatency: details.historicalWeightedLatencyMs,
    realWeightedLatency: details.historicalRealWeightedLatencyMs,
    monitoringWeightedLatency: details.historicalMonitoringWeightedLatencyMs,
  };
  const sampleLabel = isReliability ? "实际流量样本" : "实际流量延迟样本";
  const monitoringLabel = isReliability ? "监控样本" : "监控延迟样本";
  const groups: WindowComparisonGroup[] = [
    {
      label: "样本",
      rows: [
        { label: sampleLabel, recent: recent.realSamples, historical: historical.realSamples },
        { label: monitoringLabel, recent: recent.monitoringSamples, historical: historical.monitoringSamples },
        { label: "纳入评分样本合计", recent: recent.count, historical: historical.count },
      ],
    },
  ];

  if (isReliability) {
    groups.push({
      label: "结果统计",
      rows: [
        { label: "有效样本（衰减后）", recent: formatMass(recent.effectiveMass), historical: formatMass(historical.effectiveMass) },
        { label: "成功 / 失败", recent: `${formatMass(recent.success)} / ${formatMass(recent.failure)}`, historical: `${formatMass(historical.success)} / ${formatMass(historical.failure)}` },
      ],
    });
  } else {
    groups.push({
      label: "结果统计",
      rows: [
        { label: "实际流量加权平均延迟", recent: formatLatency(recent.realWeightedLatency), historical: formatLatency(historical.realWeightedLatency) },
        { label: "监控加权平均延迟", recent: formatLatency(recent.monitoringWeightedLatency), historical: formatLatency(historical.monitoringWeightedLatency) },
        { label: "加权平均延迟", recent: formatLatency(recent.weightedLatency), historical: formatLatency(historical.weightedLatency) },
        { label: "有效样本（衰减后）", recent: formatMass(recent.effectiveMass), historical: formatMass(historical.effectiveMass) },
        { label: "来源权重（实际 / 监控）", recent: `${formatBasisPoints(details.responsivenessRealSourceWeightBasisPoints)} / ${formatBasisPoints(details.responsivenessMonitoringSourceWeightBasisPoints)}`, historical: `${formatBasisPoints(details.responsivenessRealSourceWeightBasisPoints)} / ${formatBasisPoints(details.responsivenessMonitoringSourceWeightBasisPoints)}` },
      ],
    });
  }

  groups.push({
    label: "样本门槛",
    rows: [
      {
        label: "实际流量门槛",
        recent: formatWindowThreshold(recent.realSamples, details.recentMinimumSamples, recent.realMinimumMet),
        historical: formatWindowThreshold(historical.realSamples, details.historicalMinimumSamples, historical.realMinimumMet),
        wrapValues: true,
      },
      {
        label: "监控门槛",
        recent: formatWindowThreshold(recent.monitoringSamples, details.recentMinimumSamples, recent.monitoringMinimumMet),
        historical: formatWindowThreshold(historical.monitoringSamples, details.historicalMinimumSamples, historical.monitoringMinimumMet),
        wrapValues: true,
      },
    ],
  });
  groups.push({
    label: "窗口计算",
    rows: [
      { label: "窗口分", recent: formatBasisPoints(recent.score), historical: formatBasisPoints(historical.score) },
      { label: "采用权重", recent: formatBasisPoints(recent.weight), historical: formatBasisPoints(historical.weight) },
      { label: "窗口范围", recent: "最近 24 小时", historical: `24 小时以前 · ${details.historicalAgeWindowDays} 天窗口` },
      { label: "时间衰减半衰期", recent: "—", historical: `${details.historicalHalfLifeDays} 天` },
    ],
  });
  return groups;
}

function formatWindowThreshold(count: number, minimumSamples: number, minimumMet: boolean) {
  return (
    <span className="inline-flex max-w-full flex-wrap items-center justify-center gap-x-1 gap-y-1 text-center">
      <span className="whitespace-nowrap tabular-nums">{count}/{minimumSamples}</span>
      <span aria-hidden="true">，</span>
      <StatusBadge
        tone={minimumMet ? "healthy" : "warning"}
        className="h-5 rounded-[4px] px-1.5 text-[11px]"
      >
        {minimumMet ? "样本充足" : "样本不足"}
      </StatusBadge>
    </span>
  );
}

function WindowComparison({
  groups,
  historicalSubtitle,
}: {
  groups: WindowComparisonGroup[];
  historicalSubtitle: string;
}) {
  return (
    <>
      <div className="hidden border-y border-border/70 md:block">
        <div className="grid grid-cols-[minmax(164px,1.15fr)_minmax(0,1fr)_minmax(0,1fr)] items-end gap-x-4 px-1 py-2 text-[11px] font-medium text-muted-foreground">
          <span className="min-w-0">指标</span>
          <span className="min-w-0 whitespace-nowrap text-center">最近 24 小时</span>
          <span className="min-w-0 text-center">
            <span className="block whitespace-nowrap">历史数据</span>
            <span className="mt-0.5 block break-words text-[10px] font-normal leading-4 text-muted-foreground/75">（{historicalSubtitle}）</span>
          </span>
        </div>
        {groups.map((group) => (
          <div key={group.label}>
            <div className="px-1 pb-1 pt-3 text-[11px] font-medium text-muted-foreground/80">{group.label}</div>
            {group.rows.map((row) => (
              <div
                key={row.label}
                className="grid min-h-9 grid-cols-[minmax(164px,1.15fr)_minmax(0,1fr)_minmax(0,1fr)] items-center gap-x-4 border-t border-border/45 px-1 py-2 text-[13px]"
              >
                <span className="whitespace-nowrap text-muted-foreground">{row.label}</span>
                <span className={cn(
                  "min-w-0 text-center tabular-nums text-foreground",
                  row.wrapValues ? "whitespace-normal" : "whitespace-nowrap",
                )}>{row.recent}</span>
                <span className={cn(
                  "min-w-0 text-center tabular-nums text-foreground",
                  row.wrapValues ? "whitespace-normal" : "whitespace-nowrap",
                )}>{row.historical}</span>
              </div>
            ))}
          </div>
        ))}
      </div>
      <div className="grid gap-3 md:hidden">
        <WindowStack title="最近 24 小时" groups={groups} value="recent" />
        <WindowStack title="历史数据" subtitle={`（${historicalSubtitle}）`} groups={groups} value="historical" />
      </div>
    </>
  );
}

function WindowStack({
  title,
  subtitle,
  groups,
  value,
}: {
  title: string;
  subtitle?: string;
  groups: WindowComparisonGroup[];
  value: "recent" | "historical";
}) {
  return (
    <div className="border-y border-border/70 py-2">
      <div className="grid gap-0.5 px-1 pb-1.5">
        <span className="whitespace-nowrap text-xs font-medium text-foreground">{title}</span>
        {subtitle ? <span className="break-words text-[10px] text-muted-foreground/75">{subtitle}</span> : null}
      </div>
      {groups.map((group) => (
        <div key={group.label}>
          <div className="px-1 pb-1 pt-2 text-[11px] font-medium text-muted-foreground/80">{group.label}</div>
          {group.rows.map((row) => (
            <div key={row.label} className="grid min-h-9 grid-cols-[minmax(0,1fr)_auto] items-center gap-3 border-t border-border/45 px-1 py-2 text-[13px]">
              <span className="whitespace-nowrap text-muted-foreground">{row.label}</span>
              <span className={cn(
                "text-right tabular-nums text-foreground",
                row.wrapValues ? "whitespace-normal" : "whitespace-nowrap",
              )}>{row[value]}</span>
            </div>
          ))}
        </div>
      ))}
    </div>
  );
}

function MonitoringSampleStatus({
  status,
  hasSamples,
  messagePrefix = "监控样本",
}: {
  status: NonNullable<NonNullable<LocalRoutingCandidate["scoreDetails"]>["reliability"]["windowDetails"]>["monitoringSourceStatus"];
  hasSamples: boolean;
  messagePrefix?: string;
}) {
  if (status === "comparable" && hasSamples) return null;
  const message = status === "incomparable"
    ? hasSamples
      ? "监控样本按整把密钥统计，已按监控来源权重参与评分。"
      : "监控来源按整把密钥统计，当前窗口暂无有效样本。"
    : status === "no_evidence"
      ? "当前没有有效的监控观测。"
      : status === "weight_zero"
        ? "监控观测存在，但监控权重为 0，未参与评分。"
        : status === "disabled"
          ? "监控来源未启用。"
          : "监控样本按整把密钥统计，已按监控来源权重参与评分。";
  return <div className="border-t border-border pt-1 text-muted-foreground">{messagePrefix}说明：{message}</div>;
}

function UnavailableScoreWindowDetails() {
  const groups: WindowComparisonGroup[] = [
    {
      label: "窗口计算",
      rows: [{ label: "窗口数据", recent: "暂无按时间窗口划分的数据", historical: "暂无按时间窗口划分的数据" }],
    },
  ];
  return (
    <div className="grid gap-2">
      <div className="font-medium text-foreground">窗口明细</div>
      <WindowComparison groups={groups} historicalSubtitle="24 小时以前" />
    </div>
  );
}

function formatMass(value: number) {
  const scaled = value / 1_000_000;
  const formatted = Number.isInteger(scaled)
    ? scaled.toFixed(0)
    : scaled.toFixed(2).replace(/0+$/, "").replace(/\.$/, "");
  return `${formatted} 次`;
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

function factorSummary(
  label: string,
  factor: NonNullable<LocalRoutingCandidate["scoreDetails"]>["reliability"],
) {
  const windowDetails = factor.windowDetails;
  if (label === "可靠性" && windowDetails) {
    return `近24小时成功率 ${formatBasisPoints(windowDetails.recentScore)} · 历史成功率 ${formatBasisPoints(windowDetails.historicalScore)} · 近期/历史权重 ${formatBasisPoints(windowDetails.recentWeightBasisPoints)} / ${formatBasisPoints(windowDetails.historicalWeightBasisPoints)}`;
  }
  if (label === "响应速度" && windowDetails) {
    return `近24小时加权平均 ${formatLatency(windowDetails.recentWeightedLatencyMs)} · 历史加权平均 ${formatLatency(windowDetails.historicalWeightedLatencyMs)} · 近期/历史权重 ${formatBasisPoints(windowDetails.recentResponsivenessWeightBasisPoints)} / ${formatBasisPoints(windowDetails.historicalResponsivenessWeightBasisPoints)} · 来源权重 ${formatBasisPoints(windowDetails.responsivenessRealSourceWeightBasisPoints)} / ${formatBasisPoints(windowDetails.responsivenessMonitoringSourceWeightBasisPoints)}`;
  }
  return summaryInputs(label, factor);
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
        密钥有效倍率 {multiplier}
      </span>
    );
  }

  if (label === "偏好") {
    const priority = factor.inputs.find((input) => input.label === "候选优先级")?.value;
    return priority ? `基础优先级 ${priority}` : "暂无统计数据";
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
