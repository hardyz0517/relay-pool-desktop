import { useEffect, useMemo, useState } from "react";
import { Eye, MoreHorizontal, Plus, Save, Trash2 } from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import { Button, ConfirmDialog, EmptyState, SectionCard, useToast } from "@/components/ui";
import { applyModelMappingDocument, simulateModelMapping } from "@/lib/api/modelMapping";
import { getStationKeyCapabilities } from "@/lib/api/routing";
import { readError } from "@/lib/errors";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import { keyPoolQueryOptions } from "@/lib/query/resourceQueries";
import { loadModelMappingWorkspaceQuery, modelMappingQueryKeys } from "@/lib/queries/modelMappingQueries";
import type {
  ModelMappingActionDto,
  ModelMappingDiagnosticDto,
  ModelMappingDocumentDto,
  ModelMappingProfileDto,
  ModelMappingRuleDto,
  ModelMappingSimulationResultDto,
  ModelMappingTargetRefDto,
  ModelMappingWorkspaceDto,
} from "@/lib/types/modelMapping";
import { FallbackChainEditor } from "./FallbackChainEditor";

const DEFAULT_TARGET_MODEL = "deepseek-v4-flash";
const emptyConditions = {
  endpointKinds: [],
  stream: "any" as const,
  tools: "any" as const,
  vision: "any" as const,
  reasoning: "any" as const,
};

const inputClass =
  "h-8 min-w-0 rounded border border-input bg-background px-2 text-sm text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30 disabled:cursor-not-allowed disabled:opacity-60";

type SavingKind = "default" | "rule" | "delete" | null;
type ApplyOutcome =
  | { kind: "success"; workspace: ModelMappingWorkspaceDto }
  | { kind: "diagnostics"; diagnostics: ModelMappingDiagnosticDto[] }
  | { kind: "error"; message: string };

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
  const [defaultModel, setDefaultModel] = useState("");
  const [savingKind, setSavingKind] = useState<SavingKind>(null);
  const [defaultFeedback, setDefaultFeedback] = useState<string | null>(null);
  const [editorRule, setEditorRule] = useState<ModelMappingRuleDto | null>(null);
  const [editorFeedback, setEditorFeedback] = useState<string | null>(null);
  const [previewResult, setPreviewResult] = useState<ModelMappingSimulationResultDto | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [deleteRuleId, setDeleteRuleId] = useState<string | null>(null);

  useEffect(() => {
    if (workspaceQuery.data && !draft) {
      setDraft(workspaceQuery.data.document);
      setDefaultModel(defaultModelFromDocument(workspaceQuery.data.document));
    }
  }, [draft, workspaceQuery.data]);

  const rules = draft?.rules ?? [];
  const regularRules = useMemo(
    () => rules.filter((rule) => rule.matcher.kind !== "default"),
    [rules],
  );
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
    if (defaultModel.trim()) values.add(defaultModel.trim());
    for (const rule of regularRules) {
      if (rule.matcher.kind === "exact") values.add(rule.matcher.model);
      if (rule.action.kind === "map_fixed" && rule.action.target.kind === "literal") values.add(rule.action.target.upstreamModel);
    }
    return [...values].filter(Boolean).sort();
  }, [defaultModel, keyCapabilitiesQuery.data, regularRules, workspaceQuery.data?.knownModelOptions]);

  async function persist(document: ModelMappingDocumentDto): Promise<ApplyOutcome> {
    try {
      const next = await applyModelMappingDocument({ document, source: "ui" });
      if (next.diagnostics.length > 0) return { kind: "diagnostics", diagnostics: next.diagnostics };
      setDraft(next.document);
      setDefaultModel(defaultModelFromDocument(next.document));
      queryClient.setQueryData(modelMappingQueryKeys.workspace(), next);
      return { kind: "success", workspace: next };
    } catch (error) {
      return { kind: "error", message: readError(error) };
    }
  }

  async function saveDefault() {
    if (!draft || !defaultModel.trim() || savingKind) return;
    setSavingKind("default");
    setDefaultFeedback(null);
    const nextDocument = withDefaultRule(draft, defaultModel.trim());
    const outcome = await persist(nextDocument);
    if (outcome.kind === "success") toast.success("默认值已保存");
    else if (outcome.kind === "diagnostics") setDefaultFeedback(formatDiagnostics(outcome.diagnostics));
    else setDefaultFeedback(outcome.message);
    setSavingKind(null);
  }

  function beginNewRule() {
    if (!draft || savingKind) return;
    const now = Date.now();
    const priority = (draft.rules.reduce((max, rule) => Math.max(max, rule.priority), 0) || 0) + 10;
    setEditorRule({
      id: `rule-${now}`,
      priority,
      enabled: true,
      matcher: { kind: "exact", model: "" },
      conditions: emptyConditions,
      action: { kind: "map_fixed", target: { kind: "literal", upstreamModel: defaultModel.trim() || modelOptions[0] || DEFAULT_TARGET_MODEL } },
      note: null,
      revision: 0,
      createdAtMs: now,
      updatedAtMs: now,
    });
    setEditorFeedback(null);
    setPreviewResult(null);
    setPreviewError(null);
  }

  function beginEditRule(rule: ModelMappingRuleDto) {
    if (savingKind) return;
    setEditorRule(editableRule(rule, draft?.profiles ?? []));
    setEditorFeedback(null);
    setPreviewResult(null);
    setPreviewError(null);
  }

  function cancelEdit() {
    setEditorRule(null);
    setEditorFeedback(null);
    setPreviewResult(null);
    setPreviewError(null);
  }

  async function saveRule() {
    if (!draft || !editorRule || savingKind) return;
    const validation = validateEditorRule(editorRule);
    if (validation) {
      setEditorFeedback(validation);
      return;
    }
    setSavingKind("rule");
    setEditorFeedback(null);
    const nextRules = [...draft.rules.filter((rule) => rule.id !== editorRule.id), editorRule];
    const outcome = await persist({ ...draft, rules: nextRules });
    if (outcome.kind === "success") {
      toast.success("规则已保存");
      cancelEdit();
    } else if (outcome.kind === "diagnostics") setEditorFeedback(formatDiagnostics(outcome.diagnostics));
    else setEditorFeedback(outcome.message);
    setSavingKind(null);
  }

  async function confirmDeleteRule() {
    if (!draft || !deleteRuleId || savingKind) return;
    setSavingKind("delete");
    const nextRules = draft.rules.filter((rule) => rule.id !== deleteRuleId);
    const outcome = await persist({ ...draft, rules: nextRules });
    if (outcome.kind === "success") {
      if (editorRule?.id === deleteRuleId) cancelEdit();
      toast.success("规则已删除");
    } else if (outcome.kind === "diagnostics") toast.error("规则删除未保存", formatDiagnostics(outcome.diagnostics));
    else toast.error("规则删除失败", outcome.message);
    setSavingKind(null);
    setDeleteRuleId(null);
  }

  async function previewRule(rule: ModelMappingRuleDto) {
    if (!draft || validateEditorRule(rule)) return;
    setPreviewError(null);
    setPreviewResult(null);
    try {
      const result = await simulateModelMapping({
        model: rule.matcher.kind === "exact" ? rule.matcher.model : "preview-model",
        endpoint: "responses",
        stream: false,
        usesTools: false,
        usesVision: false,
        usesReasoning: false,
        draft: { ...draft, rules: [...draft.rules.filter((item) => item.id !== rule.id), rule] },
      });
      setPreviewResult(result);
    } catch (error) {
      setPreviewError(readError(error));
    }
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

  const editorPreviewDisabled = !editorRule || Boolean(validateEditorRule(editorRule)) || Boolean(savingKind);
  return (
    <SectionCard title="模型映射" description="把客户端请求的模型名转换为指定的上游模型名。">
      <div className="grid min-w-0 gap-4">
        <section className="grid gap-3" aria-labelledby="model-mapping-default-title">
          <div>
            <h3 id="model-mapping-default-title" className="text-sm font-semibold text-foreground">默认映射</h3>
            <p className="mt-1 text-xs text-muted-foreground">当请求模型没有命中任何规则时，使用该目标模型。</p>
          </div>
          <div className="flex flex-wrap items-end gap-2">
            <label className="grid min-w-64 flex-1 gap-1 text-xs text-muted-foreground sm:max-w-md">
              <span>默认目标模型</span>
              <ModelPicker
                ariaLabel="默认目标模型"
                value={defaultModel}
                options={modelOptions}
                onChange={(value) => { setDefaultModel(value); setDefaultFeedback(null); }}
                placeholder={DEFAULT_TARGET_MODEL}
              />
            </label>
            <Button type="button" variant="outline" size="sm" disabled={!defaultModel.trim() || Boolean(savingKind)} onClick={() => void saveDefault()}>
              <Save className="h-4 w-4" aria-hidden="true" />{savingKind === "default" ? "保存中" : "保存默认值"}
            </Button>
          </div>
          {defaultFeedback ? <p className="text-xs text-danger-foreground" role="alert">{defaultFeedback}</p> : null}
        </section>

        <section className="grid gap-3 border-t border-border pt-4" aria-labelledby="model-mapping-rules-title">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div>
              <h3 id="model-mapping-rules-title" className="text-sm font-semibold text-foreground">映射规则</h3>
              <p className="mt-1 text-xs text-muted-foreground">为指定的请求模型配置单独的上游模型。</p>
            </div>
            <Button type="button" variant="outline" size="sm" disabled={Boolean(savingKind)} onClick={beginNewRule}>
              <Plus className="h-4 w-4" aria-hidden="true" />新增规则
            </Button>
          </div>

          {editorRule ? (
            <RuleEditor
              rule={editorRule}
              profiles={draft.profiles}
              modelOptions={modelOptions}
              disabled={Boolean(savingKind)}
              previewDisabled={editorPreviewDisabled}
              previewResult={previewResult}
              previewError={previewError}
              feedback={editorFeedback}
              onChange={setEditorRule}
              onPreview={() => void previewRule(editorRule)}
              onSave={() => void saveRule()}
              onCancel={cancelEdit}
            />
          ) : regularRules.length === 0 ? (
            <div className="grid justify-items-center gap-2 border border-dashed border-border px-4 py-7 text-center">
              <p className="text-sm font-medium text-foreground">还没有映射规则</p>
              <p className="text-xs text-muted-foreground">添加规则后，可以把指定的请求模型映射到其他上游模型。</p>
              <Button type="button" variant="outline" size="sm" disabled={Boolean(savingKind)} onClick={beginNewRule}>
                <Plus className="h-4 w-4" aria-hidden="true" />新增第一条规则
              </Button>
            </div>
          ) : (
            <div className="overflow-x-auto border-y border-border">
              <table className="w-full min-w-[720px] text-left text-sm">
                <thead className="border-b border-border text-xs text-muted-foreground">
                  <tr><th className="px-2 py-2">请求模型</th><th className="px-2 py-2">匹配方式</th><th className="px-2 py-2">目标模型</th><th className="px-2 py-2">状态</th><th className="px-2 py-2 text-right">操作</th></tr>
                </thead>
                <tbody>
                  {regularRules.map((rule) => (
                    <RuleListRow
                      key={rule.id}
                      rule={rule}
                      profiles={draft.profiles}
                      disabled={Boolean(savingKind)}
                      onPreview={() => { beginEditRule(rule); void previewRule(rule); }}
                      onEdit={() => beginEditRule(rule)}
                      onDelete={() => setDeleteRuleId(rule.id)}
                    />
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </section>
      </div>
      <datalist id="model-mapping-model-options">
        {modelOptions.map((model) => <option key={model} value={model} />)}
      </datalist>
      <ConfirmDialog
        open={deleteRuleId !== null}
        title="删除映射规则"
        description="确认删除这条映射规则吗？删除后会立即保存，未命中的请求将不再使用这条规则。"
        confirmLabel="删除规则"
        confirming={savingKind === "delete"}
        onCancel={() => setDeleteRuleId(null)}
        onConfirm={() => void confirmDeleteRule()}
      />
    </SectionCard>
  );
}

function RuleEditor({
  rule,
  profiles,
  modelOptions,
  disabled,
  previewDisabled,
  previewResult,
  previewError,
  feedback,
  onChange,
  onPreview,
  onSave,
  onCancel,
}: {
  rule: ModelMappingRuleDto;
  profiles: ModelMappingProfileDto[];
  modelOptions: string[];
  disabled: boolean;
  previewDisabled: boolean;
  previewResult: ModelMappingSimulationResultDto | null;
  previewError: string | null;
  feedback: string | null;
  onChange: (rule: ModelMappingRuleDto) => void;
  onPreview: () => void;
  onSave: () => void;
  onCancel: () => void;
}) {
  const matcherKind = rule.matcher.kind === "glob" ? "glob" : "exact";
  const matcherValue = rule.matcher.kind === "glob" ? rule.matcher.pattern : rule.matcher.kind === "exact" ? rule.matcher.model : "";
  const isComplete = !validateEditorRule(rule);
  return (
    <div className="grid gap-4 border border-border bg-muted/10 px-3 py-3" aria-label="规则编辑器">
      <div className="grid gap-3 sm:grid-cols-2">
        <label className="grid gap-1 text-xs text-muted-foreground">
          <span>请求模型</span>
          <ModelPicker
            ariaLabel="编辑请求模型"
            value={matcherValue}
            options={modelOptions}
            disabled={disabled}
            onChange={(value) => onChange({ ...rule, matcher: matcherKind === "glob" ? { kind: "glob", pattern: value } : { kind: "exact", model: value } })}
          />
        </label>
        <label className="grid gap-1 text-xs text-muted-foreground">
          <span>匹配方式</span>
          <select
            aria-label="匹配方式"
            className={inputClass}
            value={matcherKind}
            disabled={disabled}
            onChange={(event) => onChange({ ...rule, matcher: event.target.value === "glob" ? { kind: "glob", pattern: matcherValue } : { kind: "exact", model: matcherValue } })}
          >
            <option value="exact">精确匹配</option>
            <option value="glob">通配符匹配</option>
          </select>
        </label>
      </div>
      <div className="flex flex-wrap items-end gap-4 border-t border-border pt-3">
        <label className="grid gap-1 text-xs text-muted-foreground">
          <span>优先级</span>
          <input aria-label="规则优先级" className={`${inputClass} w-24`} type="number" min={1} value={rule.priority} disabled={disabled} onChange={(event) => onChange({ ...rule, priority: Math.max(1, Number(event.target.value) || 1) })} />
        </label>
        <label className="inline-flex items-center gap-2 pb-1 text-xs text-muted-foreground">
          <input aria-label="启用规则" type="checkbox" checked={rule.enabled} disabled={disabled} onChange={(event) => onChange({ ...rule, enabled: event.target.checked })} />
          {rule.enabled ? "启用规则" : "停用规则"}
        </label>
      </div>
      <label className="grid gap-1 text-xs text-muted-foreground sm:max-w-md">
        <span>动作</span>
        <select aria-label="动作" className={inputClass} value={rule.action.kind} disabled={disabled} onChange={(event) => onChange({ ...rule, action: actionForKind(event.target.value, rule.action) })}>
          <option value="map_fixed">映射到目标模型</option>
          <option value="preserve">保留原模型</option>
          <option value="reject">拒绝请求</option>
          <option value="map_fallback_chain">多个目标回退</option>
        </select>
      </label>
      {rule.action.kind === "map_fixed" ? <TargetEditor ruleId={rule.id} target={rule.action.target} profiles={profiles} modelOptions={modelOptions} disabled={disabled} onChange={(target) => onChange({ ...rule, action: { kind: "map_fixed", target } })} /> : null}
      {rule.action.kind === "map_fallback_chain" ? <FallbackChainEditor ruleId={rule.id} action={rule.action} profiles={profiles} disabled={disabled} onChange={(action) => onChange({ ...rule, action })} /> : null}
      {rule.action.kind === "reject" ? <p className="text-xs text-muted-foreground">{rule.action.message || "请求会被拒绝。"}</p> : null}
      {!isComplete ? <p className="text-xs text-muted-foreground">请先填写请求模型和目标模型。</p> : null}
      {previewResult || previewError ? (
        <div className="border-t border-border pt-3 text-sm" role={previewError ? "alert" : "status"}>
          <p className="font-medium text-foreground">预览结果</p>
          {previewError ? <p className="mt-1 text-danger-foreground">✕ 请求失败：{previewError}</p> : (
            <div className="mt-2 grid gap-1 text-xs">
              <span className="text-muted-foreground">请求模型 <strong className="font-medium text-foreground">{previewResult?.requestedModel}</strong></span>
              <span className="text-muted-foreground">↓ 映射为 <strong className="font-medium text-foreground">{previewResult?.upstreamModel ?? "(无目标)"}</strong></span>
            </div>
          )}
        </div>
      ) : null}
      {feedback ? <p className="text-xs text-danger-foreground" role="alert">{feedback}</p> : null}
      <div className="flex flex-wrap justify-end gap-2">
        <Button type="button" variant="outline" size="sm" disabled={disabled} onClick={onCancel}>取消</Button>
        <Button type="button" variant="outline" size="sm" disabled={previewDisabled} onClick={onPreview}><Eye className="h-4 w-4" aria-hidden="true" />预览</Button>
        <Button type="button" size="sm" disabled={!isComplete || disabled} onClick={onSave}><Save className="h-4 w-4" aria-hidden="true" />{disabled ? "保存中" : "保存规则"}</Button>
      </div>
    </div>
  );
}

function RuleListRow({ rule, profiles, disabled, onPreview, onEdit, onDelete }: { rule: ModelMappingRuleDto; profiles: ModelMappingProfileDto[]; disabled: boolean; onPreview: () => void; onEdit: () => void; onDelete: () => void }) {
  const matcherLabel = rule.matcher.kind === "glob" ? "通配符匹配" : rule.matcher.kind === "exact" ? "精确匹配" : "默认匹配";
  const modelLabel = rule.matcher.kind === "glob" ? rule.matcher.pattern : rule.matcher.kind === "exact" ? rule.matcher.model : "所有未命中请求";
  return (
    <tr className="border-b border-border/70 last:border-0">
      <td className="px-2 py-2 font-medium text-foreground">{modelLabel}</td>
      <td className="px-2 py-2 text-xs text-muted-foreground">{matcherLabel}</td>
      <td className="px-2 py-2 text-foreground">{targetSummary(rule, profiles)}</td>
      <td className="px-2 py-2 text-xs text-muted-foreground">{rule.enabled ? "启用" : "停用"}</td>
      <td className="px-2 py-2 text-right">
        <div className="flex items-center justify-end gap-1">
          <Button type="button" variant="ghost" size="sm" disabled={disabled} onClick={onPreview}>预览</Button>
          <Button type="button" variant="ghost" size="sm" disabled={disabled} onClick={onEdit}>编辑</Button>
          <details className="relative">
            <summary aria-label="更多操作" title="更多操作" className="flex h-8 w-8 cursor-pointer list-none items-center justify-center rounded border border-border text-muted-foreground outline-none hover:bg-muted [&::-webkit-details-marker]:hidden"><MoreHorizontal className="h-4 w-4" /></summary>
            <div className="absolute right-0 z-10 mt-1 w-28 rounded border border-border bg-background p-1 shadow-[var(--surface-shadow)]">
              <Button type="button" variant="ghost" size="sm" className="w-full justify-start text-danger-foreground" disabled={disabled} onClick={onDelete}><Trash2 className="h-4 w-4" aria-hidden="true" />删除</Button>
            </div>
          </details>
        </div>
      </td>
    </tr>
  );
}

function TargetEditor({ ruleId, target, profiles, modelOptions, disabled, onChange }: { ruleId: string; target: ModelMappingTargetRefDto; profiles: ModelMappingProfileDto[]; modelOptions: string[]; disabled?: boolean; onChange: (target: ModelMappingTargetRefDto) => void }) {
  return (
    <label className="grid gap-1 text-xs text-muted-foreground sm:max-w-md">
      <span>目标模型</span>
      {target.kind === "literal" ? (
        <ModelPicker ariaLabel={`${ruleId} 目标模型`} value={target.upstreamModel} options={modelOptions} disabled={disabled} onChange={(value) => onChange({ ...target, upstreamModel: value })} />
      ) : (
        <select aria-label={`${ruleId} 目标模型`} className={inputClass} value={target.modelProfileId} disabled={disabled} onChange={(event) => onChange({ ...target, modelProfileId: event.target.value })}>
          <option value="">选择统一模型</option>
          {profiles.map((profile) => <option key={profile.id} value={profile.id}>{profile.displayName || profile.canonicalModel}</option>)}
        </select>
      )}
      {target.kind === "literal" && modelOptions.length > 0 ? <span className="sr-only">可从候选模型中选择，也可以直接输入。</span> : null}
    </label>
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
    <div className="flex min-w-0">
      <input
        aria-label={ariaLabel}
        className={`${inputClass} min-w-0 flex-1 rounded-r-none`}
        list="model-mapping-model-options"
        value={value}
        disabled={disabled}
        placeholder={placeholder}
        onChange={(event) => onChange(event.target.value)}
      />
      <select
        aria-label={`${ariaLabel} 候选模型`}
        title="从当前 Key 的模型中选择"
        className={`${inputClass} w-20 rounded-l-none border-l-0 px-1`}
        value=""
        disabled={disabled || options.length === 0}
        onChange={(event) => {
          if (event.target.value) onChange(event.target.value);
        }}
      >
        <option value="">选择</option>
        {options.map((option) => <option key={option} value={option}>{option}</option>)}
      </select>
    </div>
  );
}

function defaultModelFromDocument(document: ModelMappingDocumentDto): string {
  const rule = document.rules.find((item) => item.matcher.kind === "default");
  if (!rule) return "";
  if (rule.action.kind === "map_fixed" && rule.action.target.kind === "literal") return rule.action.target.upstreamModel;
  if (rule.action.kind === "map_fallback_chain") {
    const first = rule.action.targets[0];
    return first?.kind === "literal" ? first.upstreamModel : "";
  }
  return "";
}

function withDefaultRule(document: ModelMappingDocumentDto, model: string): ModelMappingDocumentDto {
  const now = Date.now();
  const existing = document.rules.find((rule) => rule.matcher.kind === "default");
  const rule: ModelMappingRuleDto = existing ? {
    ...existing,
    enabled: true,
    action: { kind: "map_fixed", target: { kind: "literal", upstreamModel: model } },
    updatedAtMs: now,
  } : {
    id: `default-${now}`,
    priority: (document.rules.reduce((max, item) => Math.max(max, item.priority), 0) || 0) + 10,
    enabled: true,
    matcher: { kind: "default" },
    conditions: emptyConditions,
    action: { kind: "map_fixed", target: { kind: "literal", upstreamModel: model } },
    note: null,
    revision: 0,
    createdAtMs: now,
    updatedAtMs: now,
  };
  return { ...document, rules: [...document.rules.filter((item) => item.id !== rule.id), rule] };
}

function editableRule(rule: ModelMappingRuleDto, profiles: ModelMappingProfileDto[]): ModelMappingRuleDto {
  if (rule.action.kind !== "map_fixed" || rule.action.target.kind !== "model_profile") return { ...rule };
  const target = rule.action.target;
  const profile = profiles.find((item) => item.id === target.modelProfileId);
  return { ...rule, action: { kind: "map_fixed", target: { kind: "literal", upstreamModel: profile?.defaultUpstreamModel ?? "" } } };
}

function validateEditorRule(rule: ModelMappingRuleDto): string | null {
  if (rule.matcher.kind === "exact" && !rule.matcher.model.trim()) return "请填写请求模型。";
  if (rule.matcher.kind === "glob" && !rule.matcher.pattern.trim()) return "请填写匹配模式。";
  if (rule.action.kind === "map_fixed" && rule.action.target.kind === "literal" && !rule.action.target.upstreamModel.trim()) return "请填写目标模型。";
  if (rule.action.kind === "map_fixed" && rule.action.target.kind === "model_profile" && !rule.action.target.modelProfileId) return "请选择目标模型。";
  if (rule.action.kind === "map_fallback_chain" && rule.action.targets.length === 0) return "请至少填写一个目标模型。";
  return null;
}

function formatDiagnostics(diagnostics: ModelMappingDiagnosticDto[]): string {
  return diagnostics.map((diagnostic) => `${diagnostic.path}: ${diagnostic.message}`).join("；") || "配置未保存。";
}

function targetSummary(rule: ModelMappingRuleDto, profiles: ModelMappingProfileDto[]): string {
  if (rule.action.kind === "preserve") return "保留原模型";
  if (rule.action.kind === "reject") return "拒绝请求";
  const targets = rule.action.kind === "map_fixed" ? [rule.action.target] : rule.action.targets;
  return targets.map((target) => {
    if (target.kind === "literal") return target.upstreamModel || "未填写";
    const profile = profiles.find((item) => item.id === target.modelProfileId);
    return profile?.displayName || profile?.canonicalModel || target.modelProfileId;
  }).join(" → ");
}

function actionForKind(kind: string, current: ModelMappingActionDto): ModelMappingActionDto {
  switch (kind) {
    case "preserve": return { kind: "preserve" };
    case "reject": return { kind: "reject", rejectionKind: "policy", message: null };
    case "map_fallback_chain": return { kind: "map_fallback_chain", targets: current.kind === "map_fixed" ? [current.target] : [{ kind: "literal", upstreamModel: "" }], fallbackTrigger: "no_eligible_target" };
    case "map_fixed":
    default: return { kind: "map_fixed", target: current.kind === "map_fallback_chain" ? (current.targets[0] ?? { kind: "literal", upstreamModel: "" }) : { kind: "literal", upstreamModel: "" } };
  }
}
