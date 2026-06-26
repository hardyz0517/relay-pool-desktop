import { AlertTriangle, Database, FileSearch, Radar, ShieldCheck } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import {
  Button,
  DataTableLite,
  InspectorPanel,
  SectionCard,
  StatusBadge,
  Toolbar,
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
type EndpointRow = {
  id: string;
  endpoint: string;
  status: string;
  matched: string;
};

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

const endpointColumns: DataTableColumn<EndpointRow>[] = [
  {
    key: "method",
    header: "Method",
    className: "w-20",
    render: (row) => row.endpoint.split(" ")[0],
  },
  {
    key: "path",
    header: "Path",
    render: (endpoint) => (
      <code className="text-xs text-slate-700">
        {endpoint.endpoint.replace(/^\\w+\\s/, "")}
      </code>
    ),
  },
  {
    key: "status",
    header: "Status",
    className: "w-20",
    render: (row) => row.status,
  },
  {
    key: "matched",
    header: "Matched",
    className: "w-28",
    render: (row) => row.matched,
  },
];

export function CollectorsPage() {
  const selected = mockCollectorSnapshot;
  const endpointRows = selected.capturedEndpoints.map((endpoint, index) => ({
    id: endpoint,
    endpoint,
    status: index === 1 ? "206" : "200",
    matched: index === 0 ? "balance" : index === 3 ? "ratio" : "model",
  }));
  const detectedCount =
    1 + selected.detectedGroupFields.length + selected.detectedRateFields.length;

  return (
    <PageScaffold
      title="Sub2API 采集"
      description="采集诊断控制台；当前只展示本地 mock，不打开 WebView 或捕获真实 XHR。"
      actions={<Button>开始采集</Button>}
    >
      <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
        <DiagMetric icon={ShieldCheck} label="登录态" value={loginLabel[selected.loginStatus]} tone={loginTone[selected.loginStatus]} />
        <DiagMetric icon={Radar} label="捕获接口" value={`${selected.capturedEndpoints.length} 个`} />
        <DiagMetric icon={Database} label="识别字段" value={`${detectedCount} 个`} />
        <DiagMetric icon={AlertTriangle} label="最近错误" value={mockCollectorFailure.failureReason ? "1 条" : "无"} tone="warning" />
      </div>

      <div className="grid gap-3 xl:grid-cols-[minmax(0,1fr)_380px]">
        <div className="min-w-0 overflow-hidden rounded-2xl border border-white/70 bg-white/90 shadow-[0_12px_30px_rgba(33,79,88,0.07)]">
          <Toolbar>
            <div>
              <div className="text-[13px] font-semibold text-slate-800">采集目标</div>
              <div className="text-xs text-muted-foreground">
                {selected.stationName} · {collectorSourceLabels[selected.source]} · {selected.fetchedAt}
              </div>
            </div>
            <div className="flex items-center gap-2">
              <StatusBadge tone={loginTone[selected.loginStatus]}>
                {loginLabel[selected.loginStatus]}
              </StatusBadge>
              <Button variant="secondary">选择站点</Button>
            </div>
          </Toolbar>

          <div className="grid gap-3 p-3 lg:grid-cols-[minmax(0,1.1fr)_360px]">
            <SectionCard
              title="捕获接口"
              description="类似 Network 面板的采集结果视图。"
              contentClassName="p-0"
            >
              <DataTableLite
                columns={endpointColumns}
                rows={endpointRows}
                getRowKey={(row) => row.id}
                className="rounded-none border-0 shadow-none"
              />
            </SectionCard>

            <SectionCard title="字段识别" description="核心字段匹配结果。">
              <div className="grid gap-2">
                <FieldMatch label="balance" tone="healthy" values={[selected.detectedBalanceField]} />
                <FieldMatch label="group" tone="info" values={selected.detectedGroupFields} />
                <FieldMatch label="rate_multiplier" tone="warning" values={selected.detectedRateFields} />
                <FieldMatch label="key / token" tone="disabled" values={[]} />
              </div>
            </SectionCard>
          </div>
        </div>

        <InspectorPanel title="采集快照" description="摘要、失败原因和手动校正入口。">
          <div className="space-y-3 p-4">
            <div className="grid gap-2">
              {selected.snapshotSummary.map((item) => (
                <div
                  key={item}
                  className="flex items-center gap-2 rounded-2xl border border-cyan-100 bg-cyan-50/60 px-3 py-2 text-xs text-slate-700"
                >
                  <FileSearch className="h-4 w-4 text-teal-600" />
                  {item}
                </div>
              ))}
            </div>
            <div className="rounded-2xl border border-amber-200 bg-amber-50/80 p-3 text-xs leading-5 text-amber-800">
              {mockCollectorFailure.failureReason}
            </div>
            <Button variant="secondary">打开手动校正</Button>
          </div>
        </InspectorPanel>
      </div>
    </PageScaffold>
  );
}

function DiagMetric({
  icon: Icon,
  label,
  value,
  tone = "info",
}: {
  icon: typeof ShieldCheck;
  label: string;
  value: string;
  tone?: StatusTone;
}) {
  const iconClassName =
    tone === "healthy"
      ? "bg-emerald-100 text-emerald-700"
      : tone === "warning"
        ? "bg-amber-100 text-amber-700"
        : "bg-cyan-100 text-cyan-700";

  return (
    <div className="rounded-2xl border border-white/70 bg-white/95 p-4 shadow-[0_12px_30px_rgba(33,79,88,0.07)]">
      <div className={`flex h-9 w-9 items-center justify-center rounded-2xl ${iconClassName}`}>
        <Icon className="h-4 w-4" />
      </div>
      <div className="mt-3 text-xs text-muted-foreground">{label}</div>
      <div className="mt-0.5 text-xl font-semibold text-slate-800">{value}</div>
    </div>
  );
}

function FieldMatch({
  label,
  tone,
  values,
}: {
  label: string;
  tone: StatusTone;
  values: string[];
}) {
  return (
    <div className="rounded-2xl border border-cyan-100 bg-cyan-50/45 p-3">
      <div className="flex items-center justify-between gap-2">
        <div className="font-mono text-xs font-semibold text-slate-700">{label}</div>
        <StatusBadge tone={values.length ? tone : "disabled"}>
          {values.length ? "matched" : "missing"}
        </StatusBadge>
      </div>
      <div className="mt-2 flex flex-wrap gap-1.5">
        {values.length ? (
          values.map((value) => (
            <code
              key={value}
              className="rounded-full border border-white/80 bg-white px-2 py-1 text-[11px] text-slate-700"
            >
              {value}
            </code>
          ))
        ) : (
          <span className="text-xs text-muted-foreground">等待手动校正</span>
        )}
      </div>
    </div>
  );
}
