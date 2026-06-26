import { PageScaffold } from "@/components/shell/PageScaffold";
import {
  Button,
  DataTableLite,
  KeyValueRow,
  SectionCard,
  StatusBadge,
  type DataTableColumn,
  type StatusTone,
} from "@/components/ui";
import {
  collectorSourceLabels,
  mockCollectorFailure,
  mockCollectorSnapshot,
  type MockCollectorSnapshot,
} from "@/lib/mock";

const snapshots = [mockCollectorSnapshot, mockCollectorFailure];

const loginTone: Record<MockCollectorSnapshot["loginStatus"], StatusTone> = {
  "logged-in": "healthy",
  expired: "warning",
  unknown: "disabled",
};

const loginLabel: Record<MockCollectorSnapshot["loginStatus"], string> = {
  "logged-in": "已登录",
  expired: "登录过期",
  unknown: "未知",
};

const sourceTone: Record<MockCollectorSnapshot["source"], StatusTone> = {
  "frontend-api": "healthy",
  "webview-capture": "info",
  html: "warning",
  manual: "disabled",
};

const columns: DataTableColumn<MockCollectorSnapshot>[] = [
  {
    key: "station",
    header: "站点",
    render: (row) => (
      <div>
        <div className="font-medium text-slate-800">{row.stationName}</div>
        <div className="mt-0.5 text-xs text-muted-foreground">{row.stationId}</div>
      </div>
    ),
  },
  {
    key: "login",
    header: "登录",
    className: "w-24",
    render: (row) => (
      <StatusBadge tone={loginTone[row.loginStatus]}>
        {loginLabel[row.loginStatus]}
      </StatusBadge>
    ),
  },
  {
    key: "source",
    header: "来源",
    className: "w-36",
    render: (row) => (
      <StatusBadge tone={sourceTone[row.source]}>
        {collectorSourceLabels[row.source]}
      </StatusBadge>
    ),
  },
  {
    key: "fetchedAt",
    header: "最新采集",
    className: "w-28 text-muted-foreground",
    render: (row) => row.fetchedAt,
  },
  {
    key: "endpoints",
    header: "接口",
    className: "w-20 text-right",
    render: (row) => `${row.capturedEndpoints.length} 个`,
  },
];

export function CollectorsPage() {
  const selectedSnapshot = mockCollectorSnapshot;

  return (
    <PageScaffold
      eyebrow="Collectors"
      title="Sub2API 采集"
      description="展示登录态、前端接口捕获、字段识别和采集快照；当前全部为本地假数据，不访问真实站点。"
    >
      <div className="grid gap-4 xl:grid-cols-[minmax(0,1.2fr)_360px]">
        <div className="flex min-w-0 flex-col gap-4">
          <SectionCard
            title="采集目标"
            description="当前选择一个 Sub2API 风格站点，后续接入 WebView 与采集服务。"
            action={
              <div className="flex items-center gap-2">
                <Button variant="outline">选择站点</Button>
                <Button>重新采集</Button>
              </div>
            }
          >
            <DataTableLite
              columns={columns}
              rows={snapshots}
              getRowKey={(row) => row.stationId}
              selectedKey={selectedSnapshot.stationId}
            />
          </SectionCard>

          <SectionCard
            title="捕获到的接口"
            description="Phase 1 仅展示捕获结果形态，不打开 WebView、不捕获真实 XHR。"
          >
            <div className="grid gap-2 sm:grid-cols-2">
              {selectedSnapshot.capturedEndpoints.map((endpoint) => (
                <div
                  key={endpoint}
                  className="rounded-md border border-border bg-slate-50 px-3 py-2 font-mono text-xs text-slate-700"
                >
                  {endpoint}
                </div>
              ))}
            </div>
          </SectionCard>

          <SectionCard
            title="字段识别结果"
            description="将站点原始 JSON 中的余额、分组和倍率字段折算为统一快照。"
          >
            <div className="grid gap-3 lg:grid-cols-3">
              <FieldGroup
                title="余额字段"
                tone="healthy"
                fields={[selectedSnapshot.detectedBalanceField]}
              />
              <FieldGroup
                title="Group 字段"
                tone="info"
                fields={selectedSnapshot.detectedGroupFields}
              />
              <FieldGroup
                title="rate_multiplier 字段"
                tone="warning"
                fields={selectedSnapshot.detectedRateFields}
              />
            </div>
          </SectionCard>
        </div>

        <div className="flex min-w-0 flex-col gap-4">
          <SectionCard title="最新快照" contentClassName="p-0">
            <dl className="px-4">
              <KeyValueRow label="站点" value={selectedSnapshot.stationName} />
              <KeyValueRow
                label="登录状态"
                value={
                  <StatusBadge tone={loginTone[selectedSnapshot.loginStatus]}>
                    {loginLabel[selectedSnapshot.loginStatus]}
                  </StatusBadge>
                }
              />
              <KeyValueRow
                label="采集来源"
                value={
                  <StatusBadge tone={sourceTone[selectedSnapshot.source]}>
                    {collectorSourceLabels[selectedSnapshot.source]}
                  </StatusBadge>
                }
              />
              <KeyValueRow label="采集时间" value={selectedSnapshot.fetchedAt} />
            </dl>
            <div className="border-t border-border p-4">
              <div className="mb-2 text-xs font-medium text-muted-foreground">
                快照摘要
              </div>
              <div className="space-y-2">
                {selectedSnapshot.snapshotSummary.map((item) => (
                  <div
                    key={item}
                    className="flex items-center justify-between rounded-md border border-border bg-slate-50 px-3 py-2 text-sm text-slate-700"
                  >
                    <span>{item}</span>
                    <span className="h-2 w-2 rounded-full bg-emerald-500" />
                  </div>
                ))}
              </div>
            </div>
          </SectionCard>

          <SectionCard
            title="失败原因"
            description="保留解析失败时的可读提示，避免用户只看到空表。"
          >
            <div className="rounded-md border border-amber-200 bg-amber-50 px-3 py-2 text-sm text-amber-800">
              {mockCollectorFailure.failureReason}
            </div>
          </SectionCard>

          <SectionCard
            title="手动校正"
            description="后续可保存字段路径与倍率修正；当前仅为入口占位。"
            action={<Button variant="outline">打开校正</Button>}
          >
            <div className="grid gap-2 text-sm text-slate-700">
              <ManualCorrectionRow label="余额路径" value="data.quota" />
              <ManualCorrectionRow label="默认分组" value="default" />
              <ManualCorrectionRow label="补全倍率" value="completion_ratio" />
            </div>
          </SectionCard>
        </div>
      </div>
    </PageScaffold>
  );
}

function FieldGroup({
  title,
  fields,
  tone,
}: {
  title: string;
  fields: string[];
  tone: StatusTone;
}) {
  return (
    <div className="rounded-lg border border-border bg-slate-50 p-3">
      <div className="mb-2 flex items-center justify-between gap-2">
        <div className="text-sm font-medium text-slate-800">{title}</div>
        <StatusBadge tone={tone}>{fields.length ? "已识别" : "未识别"}</StatusBadge>
      </div>
      <div className="space-y-1.5">
        {fields.length ? (
          fields.map((field) => (
            <div
              key={field}
              className="rounded-md bg-white px-2.5 py-1.5 font-mono text-xs text-slate-700 ring-1 ring-border"
            >
              {field}
            </div>
          ))
        ) : (
          <div className="text-xs text-muted-foreground">等待手动校正</div>
        )}
      </div>
    </div>
  );
}

function ManualCorrectionRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-3 rounded-md border border-border bg-slate-50 px-3 py-2">
      <span className="text-muted-foreground">{label}</span>
      <span className="font-mono text-xs text-slate-700">{value}</span>
    </div>
  );
}
