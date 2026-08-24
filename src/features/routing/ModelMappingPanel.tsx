import { useEffect, useMemo, useState } from "react";
import { Plus, RefreshCw, Trash2 } from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import { Button, ConfirmDialog, EmptyState, SectionCard, SelectControl, useToast } from "@/components/ui";
import { applyModelMappingDocument } from "@/lib/api/modelMapping";
import { getStationKeyCapabilities } from "@/lib/api/routing";
import { readError } from "@/lib/errors";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import { keyPoolQueryOptions } from "@/lib/query/resourceQueries";
import { loadModelMappingWorkspaceQuery, modelMappingQueryKeys } from "@/lib/queries/modelMappingQueries";
import type {
  ModelMappingDiagnosticDto,
  ModelMappingDocumentDto,
  ModelMappingRuleDto,
  ModelMappingWorkspaceDto,
} from "@/lib/types/modelMapping";

const emptyConditions = {
  endpointKinds: [],
  stream: "any" as const,
  tools: "any" as const,
  vision: "any" as const,
  reasoning: "any" as const,
};

const inputClass =
  "h-9 min-w-0 rounded-md border border-input bg-background px-2.5 text-sm text-foreground placeholder:text-muted-foreground/70 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30 disabled:cursor-not-allowed disabled:opacity-60";
const iconButtonClass = "h-9 w-9 shrink-0";

type SavingKind = "rule" | "delete" | "legacyCleanup" | "refresh" | null;
type ApplyOutcome =
  | { kind: "success"; workspace: ModelMappingWorkspaceDto }
  | { kind: "diagnostics"; diagnostics: ModelMappingDiagnosticDto[] }
  | { kind: "error"; message: string };

type SimpleModelMappingRule = ModelMappingRuleDto & {
  matcher: { kind: "exact"; model: string };
  action: { kind: "map_fixed"; target: { kind: "literal"; upstreamModel: string } };
};

type RowDraft = {
  rule: SimpleModelMappingRule;
  isNew: boolean;
  feedback: string | null;
};

export function ModelMappingPanel() {
  const toast = useToast();
  const queryClient = useQueryClient();
  const workspaceQuery = useActivityQuery({
    queryKey: modelMappingQueryKeys.workspace(),
    queryFn: loadModelMappingWorkspaceQuery,
    staleTime: 5_000,
  });
  const keyPoolQuery = useActivityQuery({
    ...keyPoolQueryOptions(),
    enabled: Boolean(workspaceQuery.data),
  });
  const keyIds = keyPoolQuery.data?.map((item) => item.id) ?? [];
  const keyCapabilitiesQuery = useActivityQuery({
    queryKey: ["modelMapping", "keyCapabilities", keyIds],
    enabled: keyPoolQuery.data !== undefined,
    queryFn: async () => Promise.all((keyPoolQuery.data ?? []).map(async (item) => {
      try {
        return await getStationKeyCapabilities(item.id);
      } catch {
        return null;
      }
    })),
    staleTime: 5_000,
  });
  const [draft, setDraft] = useState<ModelMappingDocumentDto | null>(null);
  const [savingKind, setSavingKind] = useState<SavingKind>(null);
  const [rowDrafts, setRowDrafts] = useState<Record<string, RowDraft>>({});
  const [deleteRuleId, setDeleteRuleId] = useState<string | null>(null);
  const [legacyCleanupRequested, setLegacyCleanupRequested] = useState(false);

  useEffect(() => {
    if (workspaceQuery.data && !draft) {
      setDraft(workspaceQuery.data.document);
    }
  }, [draft, workspaceQuery.data]);

  const simpleRules = useMemo(
    () => (draft?.rules ?? []).filter(isSimpleMappingRule),
    [draft?.rules],
  );
  const legacyRules = useMemo(
    () => (draft?.rules ?? []).filter((rule) => !isSimpleMappingRule(rule)),
    [draft?.rules],
  );
  const visibleRows = useMemo(() => {
    const persistedIds = new Set(simpleRules.map((rule) => rule.id));
    const rows = simpleRules.map((rule) => rowDrafts[rule.id] ?? makeRowDraft(rule, false));
    for (const row of Object.values(rowDrafts)) {
      if (!persistedIds.has(row.rule.id)) rows.push(row);
    }
    return rows;
  }, [rowDrafts, simpleRules]);
  const modelOptions = useMemo(() => {
    const values = new Set(workspaceQuery.data?.knownModelOptions ?? []);
    for (const capabilities of keyCapabilitiesQuery.data ?? []) {
      for (const model of [
        ...(capabilities?.modelAllowlist ?? []),
        ...(capabilities?.modelBlocklist ?? []),
        ...(capabilities?.preferredModels ?? []),
      ]) {
        const normalized = model.trim();
        if (normalized) values.add(normalized);
      }
    }
    for (const row of visibleRows) {
      const matcher = matcherValue(row.rule);
      if (matcher) values.add(matcher);
      if (row.rule.action.target.upstreamModel.trim()) values.add(row.rule.action.target.upstreamModel.trim());
    }
    return [...values].filter(Boolean).sort();
  }, [keyCapabilitiesQuery.data, visibleRows, workspaceQuery.data?.knownModelOptions]);

  async function persist(document: ModelMappingDocumentDto): Promise<ApplyOutcome> {
    try {
      const next = await applyModelMappingDocument({ document, source: "ui" });
      if (next.diagnostics.length > 0) return { kind: "diagnostics", diagnostics: next.diagnostics };
      setDraft(next.document);
      queryClient.setQueryData(modelMappingQueryKeys.workspace(), next);
      return { kind: "success", workspace: next };
    } catch (error) {
      return { kind: "error", message: readError(error) };
    }
  }

  async function refreshModelList() {
    if (savingKind) return;
    setSavingKind("refresh");
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: keyPoolQueryOptions().queryKey }),
      queryClient.invalidateQueries({ queryKey: ["modelMapping", "keyCapabilities"] }),
    ]);
    setSavingKind(null);
    toast.success("模型列表已刷新");
  }

  function addModel() {
    if (!draft || savingKind) return;
    const now = Date.now();
    const id = `rule-${now}`;
    const priority = (draft.rules.reduce((max, rule) => Math.max(max, rule.priority), 0) || 0) + 10;
    const rule: SimpleModelMappingRule = {
      id,
      priority,
      enabled: true,
      matcher: { kind: "exact", model: "" },
      conditions: emptyConditions,
      action: { kind: "map_fixed", target: { kind: "literal", upstreamModel: "" } },
      note: null,
      revision: 0,
      createdAtMs: now,
      updatedAtMs: now,
    };
    setRowDrafts((current) => ({ ...current, [id]: { rule, isNew: true, feedback: null } }));
  }

  function updateRow(id: string, updater: (row: RowDraft) => RowDraft) {
    setRowDrafts((current) => {
      const source = current[id] ?? makeRowDraft(simpleRules.find((rule) => rule.id === id), false);
      if (!source) return current;
      return { ...current, [id]: updater(source) };
    });
  }

  async function saveRow(id: string) {
    if (!draft || savingKind) return;
    const row = rowDrafts[id] ?? makeRowDraft(simpleRules.find((item) => item.id === id), false);
    if (!row) return;
    if (!row.rule.matcher.model.trim() || !row.rule.action.target.upstreamModel.trim()) return;
    setSavingKind("rule");
    const nextRule = { ...row.rule, enabled: true };
    const outcome = await persist({
      ...draft,
      rules: [
        ...draft.rules.filter((item) => item.id !== id).map(enableSimpleMappingRule),
        nextRule,
      ],
    });
    if (outcome.kind === "success") {
      setRowDrafts((current) => {
        const next = { ...current };
        delete next[id];
        return next;
      });
    } else if (outcome.kind === "diagnostics") {
      updateRow(id, (current) => ({ ...current, feedback: formatDiagnostics(outcome.diagnostics) }));
    } else {
      updateRow(id, (current) => ({ ...current, feedback: outcome.message }));
    }
    setSavingKind(null);
  }

  function requestDeleteRow(id: string) {
    const row = rowDrafts[id];
    if (row?.isNew) {
      setRowDrafts((current) => {
        const next = { ...current };
        delete next[id];
        return next;
      });
      return;
    }
    setDeleteRuleId(id);
  }

  async function confirmDeleteRule() {
    if (!draft || !deleteRuleId || savingKind) return;
    setSavingKind("delete");
    const outcome = await persist({
      ...draft,
      rules: draft.rules.filter((rule) => rule.id !== deleteRuleId).map(enableSimpleMappingRule),
    });
    if (outcome.kind === "success") {
      setRowDrafts((current) => {
        const next = { ...current };
        delete next[deleteRuleId];
        return next;
      });
      toast.success("模型映射已删除");
    } else if (outcome.kind === "diagnostics") toast.error("映射删除未保存", formatDiagnostics(outcome.diagnostics));
    else toast.error("映射删除失败", outcome.message);
    setSavingKind(null);
    setDeleteRuleId(null);
  }

  async function confirmLegacyCleanup() {
    if (!draft || savingKind) return;
    setSavingKind("legacyCleanup");
    const outcome = await persist({ ...draft, rules: draft.rules.filter(isSimpleMappingRule).map(enableSimpleMappingRule) });
    if (outcome.kind === "success") {
      toast.success("旧版复杂规则已移除");
      setLegacyCleanupRequested(false);
    } else if (outcome.kind === "diagnostics") {
      toast.error("旧版规则未清理", formatDiagnostics(outcome.diagnostics));
    } else {
      toast.error("旧版规则清理失败", outcome.message);
    }
    setSavingKind(null);
  }

  if (workspaceQuery.isPending && !workspaceQuery.data) {
    return <SectionCard title="模型映射"><p className="text-sm text-muted-foreground" role="status">正在加载映射配置...</p></SectionCard>;
  }
  if (workspaceQuery.error && !workspaceQuery.data) {
    return <SectionCard title="模型映射"><p className="text-sm text-destructive" role="alert">{readError(workspaceQuery.error)}</p></SectionCard>;
  }
  if (!draft) {
    return <EmptyState title="暂无模型映射配置" description="后端尚未提供可编辑的映射文档。" />;
  }

  return (
    <SectionCard title="模型映射" description="把客户端请求的模型名转换为指定的上游模型名。">
      <div className="grid min-w-0 gap-4">
        <section className="grid gap-3" aria-labelledby="model-mapping-catalog-title">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div>
              <h3 id="model-mapping-catalog-title" className="text-sm font-semibold text-foreground">模型映射</h3>
            </div>
            <div className="flex flex-wrap gap-2">
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={Boolean(savingKind)}
                onClick={() => void refreshModelList()}
              >
                <RefreshCw className={`h-4 w-4 ${savingKind === "refresh" ? "animate-spin" : ""}`} aria-hidden="true" />
                获取模型列表
              </Button>
              <Button type="button" size="sm" disabled={Boolean(savingKind)} onClick={addModel}><Plus className="h-4 w-4" aria-hidden="true" />添加模型</Button>
            </div>
          </div>

          <div className="overflow-hidden">
            {legacyRules.length > 0 ? (
              <div className="mb-3 flex flex-wrap items-center justify-between gap-3 border border-border bg-muted/20 px-3 py-2.5 text-xs text-muted-foreground">
                <span>检测到 {legacyRules.length} 条旧版复杂规则。它们无法用一对一模型映射表示，因此会继续兼容执行，直到你确认清理。</span>
                <Button type="button" variant="outline" size="sm" disabled={Boolean(savingKind)} onClick={() => setLegacyCleanupRequested(true)}>清理旧规则</Button>
              </div>
            ) : null}
            {visibleRows.length > 0 ? (
              <div className="hidden px-0 py-2 text-xs font-medium text-muted-foreground md:grid md:grid-cols-[minmax(11rem,1.1fr)_minmax(11rem,1.1fr)_auto] md:items-center md:gap-2">
                <span>实际请求模型</span>
                <span>上游目标模型</span>
                <span className="sr-only">操作</span>
              </div>
            ) : null}
            {visibleRows.length === 0 ? (
              <div className="grid justify-items-center gap-2 px-4 py-9 text-center">
                <p className="text-sm font-medium text-foreground">还没有模型映射</p>
                <p className="text-xs text-muted-foreground">添加一行即可开始填写。</p>
              </div>
            ) : visibleRows.map((row) => (
              <MappingRow
                key={row.rule.id}
                row={row}
                modelOptions={modelOptions}
                disabled={Boolean(savingKind)}
                onChange={(next) => updateRow(row.rule.id, () => ({ ...next, feedback: null }))}
                onCommit={() => void saveRow(row.rule.id)}
                onDelete={() => requestDeleteRow(row.rule.id)}
              />
            ))}
          </div>
        </section>

      </div>
      <ConfirmDialog
        open={deleteRuleId !== null}
        title="删除模型映射"
        description="确认删除这条模型映射吗？删除后会立即保存。"
        confirmLabel="删除映射"
        confirming={savingKind === "delete"}
        onCancel={() => setDeleteRuleId(null)}
        onConfirm={() => void confirmDeleteRule()}
      />
      <ConfirmDialog
        open={legacyCleanupRequested}
        title="清理旧版复杂规则"
        description="这些规则包含回退、拒绝、通配符或请求条件，无法等价转换为一对一模型映射。清理后将不再参与路由。"
        confirmLabel="清理规则"
        confirming={savingKind === "legacyCleanup"}
        onCancel={() => setLegacyCleanupRequested(false)}
        onConfirm={() => void confirmLegacyCleanup()}
      />
    </SectionCard>
  );
}

function MappingRow({
  row,
  modelOptions,
  disabled,
  onChange,
  onCommit,
  onDelete,
}: {
  row: RowDraft;
  modelOptions: string[];
  disabled: boolean;
  onChange: (row: RowDraft) => void;
  onCommit: () => void;
  onDelete: () => void;
}) {
  const matcher = matcherValue(row.rule);
  const updateRule = (rule: SimpleModelMappingRule) => onChange({ ...row, rule, feedback: null });
  return (
    <div
      className="border-b border-border last:border-b-0"
      aria-label={`模型映射行 ${row.rule.id}`}
      onBlur={(event) => {
        const nextTarget = event.relatedTarget;
        const isListboxTarget = nextTarget instanceof Element && nextTarget.closest('[role="listbox"]');
        if (nextTarget && (event.currentTarget.contains(nextTarget) || isListboxTarget)) return;
        onCommit();
      }}
    >
      <div className="grid gap-2 px-0 py-3 md:grid-cols-[minmax(11rem,1.1fr)_minmax(11rem,1.1fr)_auto] md:items-center">
        <label className="grid gap-1 text-xs text-muted-foreground">
          <span className="md:sr-only">实际请求模型</span>
          <ModelPicker
            ariaLabel={`实际请求模型 ${row.rule.id}`}
            value={matcher}
            options={modelOptions}
            disabled={disabled}
            placeholder="例如：gpt-4o-mini"
            onChange={(value) => updateRule({ ...row.rule, matcher: { kind: "exact", model: value } })}
          />
        </label>
        <label className="grid gap-1 text-xs text-muted-foreground">
          <span className="md:sr-only">上游目标模型</span>
          <ModelPicker
            ariaLabel={`上游目标模型 ${row.rule.id}`}
            value={row.rule.action.target.upstreamModel}
            options={modelOptions}
            disabled={disabled}
            placeholder="例如：deepseek-v4-flash"
            onChange={(value) => updateRule({ ...row.rule, action: { kind: "map_fixed", target: { kind: "literal", upstreamModel: value } } })}
          />
        </label>
        <div className="flex items-center justify-end gap-1">
          <Button type="button" variant="ghost" size="icon" className={`${iconButtonClass} text-danger-foreground hover:text-danger-foreground`} aria-label={`删除模型映射 ${row.rule.id}`} title="删除" disabled={disabled} onClick={onDelete}>
            <Trash2 className="h-4 w-4" aria-hidden="true" />
          </Button>
        </div>
      </div>
      {row.feedback ? <p className="px-3 pb-3 text-xs text-danger-foreground" role="alert">{row.feedback}</p> : null}
    </div>
  );
}

function ModelPicker({
  ariaLabel,
  value,
  options,
  disabled,
  placeholder,
  onChange,
}: {
  ariaLabel: string;
  value: string;
  options: string[];
  disabled?: boolean;
  placeholder?: string;
  onChange: (value: string) => void;
}) {
  return (
    <div className="flex min-w-0 items-center gap-2">
      <input
        aria-label={ariaLabel}
        className={`${inputClass} min-w-0 flex-1`}
        value={value}
        disabled={disabled}
        placeholder={placeholder}
        onChange={(event) => onChange(event.target.value)}
      />
      <SelectControl
        ariaLabel={`${ariaLabel} 候选模型`}
        title="从当前 Key 的模型中选择"
        className="!h-9 !w-9 !min-w-9 !px-0 justify-center [&>span:first-child]:hidden"
        value=""
        options={options.map((option) => ({ value: option, label: option }))}
        searchable
        searchPlaceholder="搜索模型..."
        emptyLabel="没有匹配的模型"
        menuMinWidth={220}
        disabled={disabled || options.length === 0}
        onChange={(option) => onChange(option)}
      />
    </div>
  );
}

function makeRowDraft(rule: SimpleModelMappingRule | undefined, isNew: boolean): RowDraft | null {
  if (!rule) return null;
  return { rule: { ...rule, enabled: true }, isNew, feedback: null };
}

function matcherValue(rule: SimpleModelMappingRule): string {
  return rule.matcher.model;
}

function isSimpleMappingRule(rule: ModelMappingRuleDto): rule is SimpleModelMappingRule {
  return rule.matcher.kind === "exact"
    && rule.conditions.endpointKinds.length === 0
    && rule.conditions.stream === "any"
    && rule.conditions.tools === "any"
    && rule.conditions.vision === "any"
    && rule.conditions.reasoning === "any"
    && rule.action.kind === "map_fixed"
    && rule.action.target.kind === "literal";
}

function enableSimpleMappingRule(rule: ModelMappingRuleDto): ModelMappingRuleDto {
  return isSimpleMappingRule(rule) ? { ...rule, enabled: true } : rule;
}

function formatDiagnostics(diagnostics: ModelMappingDiagnosticDto[]): string {
  return diagnostics.map((diagnostic) => `${diagnostic.path}: ${diagnostic.message}`).join("；") || "配置未保存。";
}
