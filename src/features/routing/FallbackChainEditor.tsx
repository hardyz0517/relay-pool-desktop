import { ArrowDown, ArrowUp, Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui";
import type {
  ModelMappingActionDto,
  ModelMappingProfileDto,
  ModelMappingTargetRefDto,
} from "@/lib/types/modelMapping";

type FallbackChainEditorProps = {
  ruleId: string;
  action: Extract<ModelMappingActionDto, { kind: "map_fallback_chain" }>;
  profiles: ModelMappingProfileDto[];
  disabled?: boolean;
  onChange: (action: Extract<ModelMappingActionDto, { kind: "map_fallback_chain" }>) => void;
};

const inputClass =
  "h-8 min-w-0 rounded border border-input bg-background px-2 text-sm text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30 disabled:cursor-not-allowed disabled:opacity-60";

function literalTarget(): ModelMappingTargetRefDto {
  return { kind: "literal", upstreamModel: "" };
}

export function FallbackChainEditor({ ruleId, action, profiles, disabled = false, onChange }: FallbackChainEditorProps) {
  const updateTarget = (index: number, target: ModelMappingTargetRefDto) => {
    onChange({ ...action, targets: action.targets.map((item, itemIndex) => itemIndex === index ? target : item) });
  };

  const moveTarget = (index: number, direction: -1 | 1) => {
    const nextIndex = index + direction;
    if (nextIndex < 0 || nextIndex >= action.targets.length) return;
    const targets = [...action.targets];
    [targets[index], targets[nextIndex]] = [targets[nextIndex], targets[index]];
    onChange({ ...action, targets });
  };

  return (
    <fieldset className="mt-2 grid gap-2 border-l-2 border-info-border pl-3" disabled={disabled}>
      <legend className="sr-only">{ruleId} 回退链</legend>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="text-xs text-muted-foreground">
          <span className="font-medium text-foreground">回退目标顺序</span>
          <span className="ml-2">最多 3 个 TargetRef，由后端决定资格和切换时机。</span>
        </div>
        <Button
          type="button"
          variant="outline"
          size="sm"
          aria-label={`${ruleId} 新增回退目标`}
          disabled={disabled || action.targets.length >= 3}
          onClick={() => onChange({ ...action, targets: [...action.targets, literalTarget()] })}
        >
          <Plus className="h-4 w-4" aria-hidden="true" />
          新增目标
        </Button>
      </div>
      <label className="grid max-w-sm gap-1 text-xs text-muted-foreground">
        回退触发条件
        <select
          aria-label={`${ruleId} 回退触发条件`}
          className={inputClass}
          value={action.fallbackTrigger}
          onChange={(event) => onChange({ ...action, fallbackTrigger: event.target.value as typeof action.fallbackTrigger })}
        >
          <option value="no_eligible_target">无可用目标</option>
          <option value="retry_exhausted_before_output">输出前重试耗尽（高级）</option>
        </select>
      </label>
      {action.targets.length === 0 ? (
        <p className="border border-dashed border-border px-2 py-3 text-xs text-muted-foreground">暂无回退目标，保存会被后端拒绝。</p>
      ) : (
        <ol className="grid gap-2" aria-label={`${ruleId} 回退目标列表`}>
          {action.targets.map((target, index) => (
            <li key={`${ruleId}-target-${index}`} className="grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-2">
              <span className="grid h-6 w-6 place-items-center rounded-full bg-muted text-xs font-medium text-muted-foreground" aria-label={`第 ${index + 1} 个目标`}>{index + 1}</span>
              <div className="grid min-w-0 grid-cols-[auto_minmax(0,1fr)] gap-2">
                <select
                  aria-label={`${ruleId} 目标 ${index + 1} 类型`}
                  className={`${inputClass} w-28`}
                  value={target.kind}
                  onChange={(event) => updateTarget(index, event.target.value === "model_profile"
                    ? { kind: "model_profile", modelProfileId: profiles[0]?.id ?? "" }
                    : literalTarget())}
                >
                  <option value="literal">实际模型</option>
                  <option value="model_profile" disabled={profiles.length === 0}>Profile</option>
                </select>
                {target.kind === "literal" ? (
                  <input
                    aria-label={`${ruleId} 目标 ${index + 1} 实际模型`}
                    className={`${inputClass} w-full`}
                    value={target.upstreamModel}
                    placeholder="upstream model"
                    onChange={(event) => updateTarget(index, { ...target, upstreamModel: event.target.value })}
                  />
                ) : (
                  <select
                    aria-label={`${ruleId} 目标 ${index + 1} Profile`}
                    className={`${inputClass} w-full`}
                    value={target.modelProfileId}
                    onChange={(event) => updateTarget(index, { ...target, modelProfileId: event.target.value })}
                  >
                    <option value="">选择 Profile</option>
                    {profiles.map((profile) => <option key={profile.id} value={profile.id}>{profile.displayName || profile.canonicalModel}</option>)}
                    {!profiles.some((profile) => profile.id === target.modelProfileId) && target.modelProfileId ? <option value={target.modelProfileId}>{target.modelProfileId}（当前文档）</option> : null}
                  </select>
                )}
              </div>
              <div className="flex items-center gap-0.5">
                <Button type="button" variant="ghost" size="icon" aria-label={`上移目标 ${index + 1}`} title="上移" disabled={disabled || index === 0} onClick={() => moveTarget(index, -1)}><ArrowUp className="h-4 w-4" aria-hidden="true" /></Button>
                <Button type="button" variant="ghost" size="icon" aria-label={`下移目标 ${index + 1}`} title="下移" disabled={disabled || index === action.targets.length - 1} onClick={() => moveTarget(index, 1)}><ArrowDown className="h-4 w-4" aria-hidden="true" /></Button>
                <Button type="button" variant="ghost" size="icon" aria-label={`删除目标 ${index + 1}`} title="删除目标" disabled={disabled} onClick={() => onChange({ ...action, targets: action.targets.filter((_, itemIndex) => itemIndex !== index) })}><Trash2 className="h-4 w-4" aria-hidden="true" /></Button>
              </div>
            </li>
          ))}
        </ol>
      )}
    </fieldset>
  );
}
