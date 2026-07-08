import { ArrowDownUp, BadgeCheck, LockKeyhole } from "lucide-react";
import { EmptyState, SectionCard, StatusBadge } from "@/components/ui";
import type { LocalRoutingWorkspace } from "@/lib/types/localRouting";
import { LocalRoutingCandidateRow } from "./LocalRoutingCandidateRow";

type LocalRoutingEditTabProps = {
  workspace: LocalRoutingWorkspace | null;
  loading: boolean;
};

export function LocalRoutingEditTab({ workspace, loading }: LocalRoutingEditTabProps) {
  if (loading && !workspace) {
    return (
      <SectionCard title="编辑预览">
        <div className="text-sm text-muted-foreground">正在加载本地路由工作区...</div>
      </SectionCard>
    );
  }

  if (!workspace) {
    return (
      <SectionCard title="编辑预览">
        <EmptyState title="暂无可编辑数据" description="当前仅展示本地路由编辑骨架。" />
      </SectionCard>
    );
  }

  return (
    <div className="grid gap-3">
      <SectionCard title="策略草案" contentClassName="grid gap-3">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="flex min-w-0 items-center gap-3">
            <span className="flex h-10 w-10 shrink-0 items-center justify-center rounded-[10px] bg-blue-50 text-blue-700">
              <BadgeCheck className="h-5 w-5" />
            </span>
            <div className="min-w-0">
              <div className="truncate text-sm font-semibold text-slate-900">低价优先 + 稳定保持</div>
              <div className="truncate text-xs text-muted-foreground">
                预览候选顺位与启用状态，暂不写入路由行为。
              </div>
            </div>
          </div>
          <StatusBadge tone="info">预览</StatusBadge>
        </div>
        <div className="grid gap-2 text-xs text-slate-600 sm:grid-cols-2">
          <EditHint icon={<ArrowDownUp className="h-4 w-4" />} title="拖拽排序" body="后续任务接入，当前只保留布局占位。" />
          <EditHint icon={<LockKeyhole className="h-4 w-4" />} title="行为冻结" body="本页不触发保存、重排或运行时策略更新。" />
        </div>
      </SectionCard>

      <SectionCard title="候选编辑列表" contentClassName="grid gap-2">
        {workspace.candidates.length === 0 ? (
          <EmptyState title="暂无候选 Key" description="候选列表会随本地路由工作区加载后显示。" />
        ) : (
          workspace.candidates.map((candidate) => (
            <LocalRoutingCandidateRow key={candidate.stationKeyId} candidate={candidate} />
          ))
        )}
      </SectionCard>
    </div>
  );
}

function EditHint({ icon, title, body }: { icon: React.ReactNode; title: string; body: string }) {
  return (
    <div className="flex items-start gap-2 rounded-[var(--surface-radius)] border border-slate-200 bg-slate-50 px-3 py-2">
      <span className="mt-0.5 shrink-0 text-slate-500">{icon}</span>
      <span className="min-w-0">
        <span className="block text-xs font-semibold text-slate-800">{title}</span>
        <span className="block text-xs leading-5 text-muted-foreground">{body}</span>
      </span>
    </div>
  );
}
