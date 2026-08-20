import { createPortal } from "react-dom";
import { useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import {
  AlertTriangle,
  Check,
  ExternalLink,
  LoaderCircle,
  MoreHorizontal,
  Pencil,
  Plus,
  ScanSearch,
  Trash2,
  WalletCards,
  X,
} from "lucide-react";
import { Button, ConfirmDialog, Dialog, IconButton, StatusBadge } from "@/components/ui";
import { getLatestCollectorSnapshot, scanStationRecharge } from "@/lib/api/collector";
import type { CollectorRunResult, CollectorSnapshot } from "@/lib/types/collector";
import type { Station } from "@/lib/types/stations";
import { readRechargeEntries, sanitizeRechargeUrl, writeRechargeEntries } from "./rechargeEntriesStorage";

export type RechargeProvider = "liandong" | "cloudcat" | "custom";
export type RechargeEntrySource = "confirmed" | "manual";

export type RechargeEntry = {
  url: string;
  label: string;
  provider: RechargeProvider;
  paymentMethods: string[];
  source?: RechargeEntrySource;
  note?: string;
};

type RechargeDialogProps = {
  station: Station | null;
  onClose: () => void;
  onOpenUrl: (url: string) => Promise<void>;
  onAuthorize?: (station: Station) => void;
};

type EntryFormState = { label: string; url: string; note: string };

const providerLabels: Record<RechargeProvider, string> = {
  liandong: "链动小铺",
  cloudcat: "云猫",
  custom: "自定义页面",
};
const paymentLabels: Record<string, string> = {
  alipay: "支付宝",
  wechat: "微信支付",
  bank: "银行卡",
  usdt: "数字货币",
};
const inputClassName = "h-8 w-full rounded-[var(--surface-radius)] border border-border bg-surface px-3 text-sm text-foreground outline-none transition placeholder:text-muted-foreground/70 focus:border-ring focus:ring-2 focus:ring-ring/30";
const RECHARGE_SCAN_TIMEOUT_MS = 45_000;

function emptyForm(): EntryFormState { return { label: "", url: "", note: "" }; }

function displayRechargeUrl(url: string): string {
  try {
    const parsed = new URL(url);
    return parsed.pathname === "/" ? parsed.host : `${parsed.host}${parsed.pathname}`;
  } catch {
    return url;
  }
}

function scanRechargeWithTimeout(stationId: string): Promise<CollectorRunResult> {
  return new Promise((resolve, reject) => {
    const timer = window.setTimeout(() => reject(new Error("recharge_scan_timeout")), RECHARGE_SCAN_TIMEOUT_MS);
    void scanStationRecharge(stationId).then((result) => { window.clearTimeout(timer); resolve(result); }, (error: unknown) => { window.clearTimeout(timer); reject(error); });
  });
}

export function RechargeDialog({ station, onClose, onOpenUrl, onAuthorize }: RechargeDialogProps) {
  const [savedEntries, setSavedEntries] = useState<RechargeEntry[]>([]);
  const [scanResults, setScanResults] = useState<RechargeEntry[]>([]);
  const [isLoadingSaved, setIsLoadingSaved] = useState(false);
  const [isScanning, setIsScanning] = useState(false);
  const [hasScanned, setHasScanned] = useState(false);
  const [scanError, setScanError] = useState<string | null>(null);
  const [scanRequiresLogin, setScanRequiresLogin] = useState(false);
  const [isAddingEntry, setIsAddingEntry] = useState(false);
  const [editingUrl, setEditingUrl] = useState<string | null>(null);
  const [entryForm, setEntryForm] = useState<EntryFormState>(emptyForm);
  const [entryFormError, setEntryFormError] = useState<string | null>(null);
  const [openMenuUrl, setOpenMenuUrl] = useState<string | null>(null);
  const [pendingRemoval, setPendingRemoval] = useState<RechargeEntry | null>(null);
  const [openingUrl, setOpeningUrl] = useState<string | null>(null);

  useEffect(() => {
    if (!station) return;
    let cancelled = false;
    const stationId = station.id;
    const stored = readRechargeEntries(stationId);
    setSavedEntries(stored);
    setScanResults([]);
    setHasScanned(false);
    setScanError(null);
    setScanRequiresLogin(false);
    setIsAddingEntry(false);
    setEditingUrl(null);
    setEntryForm(emptyForm());
    setEntryFormError(null);
    setOpenMenuUrl(null);
    setIsLoadingSaved(stored.length === 0);

    if (stored.length > 0) {
      return () => { cancelled = true; };
    }

    // Read the latest local snapshot only to migrate legacy confirmed entries.
    // Opening this dialog never starts a browser scan.
    void getLatestCollectorSnapshot(stationId)
      .then((snapshot) => {
        if (cancelled || stored.length > 0 || !snapshot || snapshot.status !== "success") return;
        const migrated = parseRechargeSnapshot(snapshot).entries.filter((entry) => !isStationShellUrl(entry.url, station.websiteUrl));
        if (migrated.length === 0) return;
        writeRechargeEntries(stationId, migrated);
        setSavedEntries(migrated);
      })
      .catch(() => undefined)
      .finally(() => { if (!cancelled) setIsLoadingSaved(false); });
    return () => { cancelled = true; };
  }, [station?.id]);

  const savedUrlSet = useMemo(() => new Set(savedEntries.map((entry) => entry.url)), [savedEntries]);
  if (!station) return null;
  const activeStation = station;

  function persistEntries(next: RechargeEntry[]) {
    setSavedEntries(next);
    writeRechargeEntries(activeStation.id, next);
  }

  async function runScan() {
    if (isScanning) return;
    setIsScanning(true);
    setHasScanned(true);
    setScanError(null);
    setScanRequiresLogin(false);
    setScanResults([]);
    try {
      const parsed = parseRechargeRun(await scanRechargeWithTimeout(activeStation.id));
      if (parsed.status === "success") setScanResults(parsed.entries);
      else {
        setScanError(parsed.message);
        setScanRequiresLogin(parsed.status === "manual_required");
      }
    } catch (error) {
      setScanError(error instanceof Error && error.message === "recharge_scan_timeout" ? "扫描超时，无法读取登录页面。" : "扫描失败，无法读取登录页面。请稍后重试。");
    } finally {
      setIsScanning(false);
    }
  }

  function openAddForm() {
    setOpenMenuUrl(null); setEditingUrl(null); setEntryForm(emptyForm()); setEntryFormError(null); setIsAddingEntry(true);
  }
  function openEditForm(entry: RechargeEntry) {
    setOpenMenuUrl(null); setEditingUrl(entry.url); setEntryForm({ label: entry.label, url: entry.url, note: entry.note ?? "" }); setEntryFormError(null); setIsAddingEntry(false);
  }
  function closeEntryForm() {
    setIsAddingEntry(false); setEditingUrl(null); setEntryForm(emptyForm()); setEntryFormError(null);
  }
  function submitEntryForm(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const label = entryForm.label.trim();
    const url = normalizeManualUrl(entryForm.url);
    const note = entryForm.note.trim();
    if (!label) return setEntryFormError("请填写入口名称。");
    if (!url) return setEntryFormError("请输入有效的 http(s) 充值地址。");
    if (savedEntries.some((entry) => entry.url === url && entry.url !== editingUrl)) return setEntryFormError("这个地址已经在入口列表中。");
    if (editingUrl) {
      persistEntries(savedEntries.map((entry) => entry.url === editingUrl ? { ...entry, label, url, note: note || undefined } : entry));
    } else {
      persistEntries([...savedEntries, { url, label, note: note || undefined, provider: "custom", paymentMethods: [], source: "manual" }]);
    }
    closeEntryForm();
  }
  function confirmCandidate(entry: RechargeEntry) {
    if (!savedUrlSet.has(entry.url)) persistEntries([...savedEntries, { ...entry, source: "confirmed" }]);
  }
  function openEntry(entry: RechargeEntry) {
    setOpeningUrl(entry.url);
    void onOpenUrl(entry.url).finally(() => setOpeningUrl(null));
  }

  const formOpen = isAddingEntry || editingUrl !== null;

  return (
    <>
      <Dialog
        open
        title={`充值入口 · ${station.name}`}
        onClose={onClose}
        className="max-w-[640px]"
        footer={
          <div className="flex justify-end">
            <Button variant="outline" size="sm" onClick={onClose}>完成</Button>
          </div>
        }
      >
        <div className="space-y-5 p-5">
          {scanError ? <div className="flex flex-wrap items-center gap-2 rounded-[8px] border border-warning-border bg-warning-surface px-3 py-2 text-xs text-warning-foreground"><AlertTriangle className="h-3.5 w-3.5 shrink-0" /><span>{scanError}</span>{scanRequiresLogin && onAuthorize ? <Button variant="ghost" size="sm" onClick={() => onAuthorize(station)}>浏览器授权</Button> : null}{!isScanning ? <Button variant="ghost" size="sm" onClick={() => void runScan()}>重试</Button> : null}</div> : null}
          {formOpen ? <EntryForm editing={editingUrl !== null} form={entryForm} error={entryFormError} onChange={(next) => { setEntryForm(next); setEntryFormError(null); }} onCancel={closeEntryForm} onSubmit={submitEntryForm} /> : null}

          <section className="space-y-2">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <h3 className="text-sm font-semibold text-foreground">已确认入口（{savedEntries.length}）</h3>
              <div className="flex flex-wrap items-center gap-2">
                <Button size="sm" disabled={isScanning || formOpen} onClick={() => void runScan()}>{isScanning ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <ScanSearch className="h-3.5 w-3.5" />}{isScanning ? "正在扫描…" : hasScanned ? "重新扫描" : "扫描登录页"}</Button>
                <Button variant="outline" size="sm" disabled={isScanning || formOpen} onClick={openAddForm}><Plus className="h-3.5 w-3.5" />手动添加入口</Button>
              </div>
            </div>
            {isLoadingSaved ? <div className="flex items-center gap-2 rounded-[8px] border border-border bg-surface-subtle px-3 py-4 text-sm text-muted-foreground"><LoaderCircle className="h-4 w-4 animate-spin" />正在读取已保存入口…</div> : savedEntries.length === 0 ? <div className="rounded-[10px] border border-border bg-surface-subtle px-4 py-4 text-center"><div className="text-sm font-medium text-foreground">还没有充值入口</div><p className="mx-auto mt-1 max-w-md text-xs leading-5 text-muted-foreground">你可以扫描站点登录页自动发现，<br />也可以直接手动添加入口。</p></div> : <div className="space-y-2">{savedEntries.map((entry) => <EntryCard key={entry.url} entry={entry} menuOpen={openMenuUrl === entry.url} opening={openingUrl === entry.url} onOpen={() => openEntry(entry)} onMenu={() => setOpenMenuUrl(openMenuUrl === entry.url ? null : entry.url)} onEdit={() => openEditForm(entry)} onRemove={() => { setOpenMenuUrl(null); setPendingRemoval(entry); }} />)}</div>}
          </section>

          {hasScanned && !isScanning ? <section className="space-y-2 border-t border-border pt-5"><div className="flex items-center justify-between gap-3"><h3 className="text-sm font-semibold text-foreground">扫描结果（{scanResults.length}）</h3>{scanResults.length > 0 ? <Button variant="ghost" size="sm" onClick={() => setScanResults([])}>清除结果</Button> : null}</div>{scanResults.length === 0 ? <div className="rounded-[8px] border border-border bg-surface-subtle px-3 py-3 text-sm text-muted-foreground">{scanError ? "本次没有可供确认的候选入口。" : "本次没有发现新的充值入口。"}</div> : <div className="space-y-2">{scanResults.map((entry) => <ScanCandidateCard key={entry.url} entry={entry} existing={savedUrlSet.has(entry.url)} onConfirm={() => confirmCandidate(entry)} onIgnore={() => setScanResults((current) => current.filter((item) => item.url !== entry.url))} />)}</div>}</section> : null}
        </div>
      </Dialog>
      <ConfirmDialog open={pendingRemoval !== null} title="移除充值入口" description={`确定移除“${pendingRemoval?.label ?? "充值入口"}”吗？这不会影响之后的扫描结果。`} confirmLabel="移除" onCancel={() => setPendingRemoval(null)} onConfirm={() => { if (pendingRemoval) persistEntries(savedEntries.filter((entry) => entry.url !== pendingRemoval.url)); setPendingRemoval(null); }} />
    </>
  );
}

function EntryCard({ entry, menuOpen, opening, onOpen, onMenu, onEdit, onRemove }: { entry: RechargeEntry; menuOpen: boolean; opening: boolean; onOpen: () => void; onMenu: () => void; onEdit: () => void; onRemove: () => void }) {
  const menuAnchorRef = useRef<HTMLSpanElement | null>(null);
  const menuRef = useRef<HTMLDivElement | null>(null);
  const [menuPosition, setMenuPosition] = useState<{ top: number; left: number } | null>(null);

  useEffect(() => {
    if (!menuOpen) {
      setMenuPosition(null);
      return;
    }

    const updatePosition = () => {
      const rect = menuAnchorRef.current?.getBoundingClientRect();
      if (!rect) return;
      const width = 112;
      const height = 80;
      const gap = 4;
      const canOpenBelow = window.innerHeight - rect.bottom >= height + gap + 8;
      setMenuPosition({
        top: canOpenBelow ? rect.bottom + gap : Math.max(8, rect.top - height - gap),
        left: Math.max(8, Math.min(rect.right - width, window.innerWidth - width - 8)),
      });
    };
    const closeOnOutside = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!menuAnchorRef.current?.contains(target) && !menuRef.current?.contains(target)) onMenu();
    };

    updatePosition();
    document.addEventListener("pointerdown", closeOnOutside);
    window.addEventListener("resize", updatePosition);
    window.addEventListener("scroll", updatePosition, true);
    return () => {
      document.removeEventListener("pointerdown", closeOnOutside);
      window.removeEventListener("resize", updatePosition);
      window.removeEventListener("scroll", updatePosition, true);
    };
  }, [menuOpen, onMenu]);

  return <div className="flex min-w-0 items-center justify-between gap-3 rounded-[10px] border border-border bg-surface px-3 py-3"><div className="flex min-w-0 items-start gap-2.5"><span className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-[8px] bg-primary/10 text-primary"><WalletCards className="h-4 w-4" /></span><div className="min-w-0"><div className="flex flex-wrap items-center gap-1.5 text-sm font-medium text-foreground"><span className="truncate">{entry.label}</span><StatusBadge tone={entry.source === "manual" ? "disabled" : "healthy"} className="h-5 px-1.5 text-[10px]">{entry.source === "manual" ? "手动添加" : "已确认"}</StatusBadge></div><div className="mt-0.5 truncate text-xs text-muted-foreground" title={entry.url}>{entry.note || `${providerLabels[entry.provider]}${entry.paymentMethods.length ? ` · ${entry.paymentMethods.map((method) => paymentLabels[method] ?? method).join("、")}` : ""}`}</div><div className="mt-0.5 max-w-[420px] truncate text-[11px] text-muted-foreground/75" title={entry.url}>{displayRechargeUrl(entry.url)}</div></div></div><div className="flex shrink-0 items-center gap-1"><Button variant="ghost" size="sm" onClick={onOpen} disabled={opening}>{opening ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" /> : <ExternalLink className="h-3.5 w-3.5" />}打开</Button><span ref={menuAnchorRef} className="inline-flex"><IconButton label={`更多操作：${entry.label}`} onClick={onMenu}><MoreHorizontal className="h-4 w-4" /></IconButton></span>{menuOpen && menuPosition ? createPortal(<div ref={menuRef} className="fixed z-[100] min-w-[112px] rounded-[8px] border border-border bg-surface p-1 shadow-popover" style={{ top: menuPosition.top, left: menuPosition.left }}><button type="button" className="flex h-8 w-full items-center gap-2 rounded-[6px] px-2 text-left text-xs text-foreground hover:bg-hover" onClick={onEdit}><Pencil className="h-3.5 w-3.5" />编辑</button><button type="button" className="flex h-8 w-full items-center gap-2 rounded-[6px] px-2 text-left text-xs text-danger-foreground hover:bg-danger-surface" onClick={onRemove}><Trash2 className="h-3.5 w-3.5" />移除</button></div>, document.body) : null}</div></div>;
}

function ScanCandidateCard({ entry, existing, onConfirm, onIgnore }: { entry: RechargeEntry; existing: boolean; onConfirm: () => void; onIgnore: () => void }) {
  return <div className="flex min-w-0 items-center justify-between gap-3 rounded-[10px] border border-info-border/70 bg-surface px-3 py-3"><div className="min-w-0"><div className="flex flex-wrap items-center gap-1.5 text-sm font-medium text-foreground"><span className="truncate">{entry.label}</span><StatusBadge tone={existing ? "disabled" : "info"} className="h-5 px-1.5 text-[10px]">{existing ? "已存在" : "新发现"}</StatusBadge></div><div className="mt-0.5 text-xs text-muted-foreground">发现于：登录页面 · {providerLabels[entry.provider]}</div><div className="mt-0.5 max-w-[420px] truncate text-[11px] text-muted-foreground/75" title={entry.url}>{displayRechargeUrl(entry.url)}</div></div>{existing ? <span className="shrink-0 text-xs text-muted-foreground">已在入口列表中</span> : <div className="flex shrink-0 items-center gap-1"><Button size="sm" onClick={onConfirm}><Check className="h-3.5 w-3.5" />确认添加</Button><Button variant="ghost" size="sm" onClick={onIgnore}>忽略</Button></div>}</div>;
}

function EntryForm({ editing, form, error, onChange, onCancel, onSubmit }: { editing: boolean; form: EntryFormState; error: string | null; onChange: (form: EntryFormState) => void; onCancel: () => void; onSubmit: (event: FormEvent<HTMLFormElement>) => void }) {
  return <form className="space-y-3 rounded-[8px] border border-border bg-surface-subtle p-4" onSubmit={onSubmit}><div className="flex items-center justify-between gap-3"><div className="text-sm font-semibold text-foreground">{editing ? "编辑充值入口" : "添加充值入口"}</div><IconButton label="取消编辑" onClick={onCancel}><X className="h-4 w-4" /></IconButton></div><div className="grid gap-3 sm:grid-cols-2"><label className="grid gap-1.5 text-xs font-medium text-muted-foreground">入口名称<input className={inputClassName} value={form.label} onChange={(event) => onChange({ ...form, label: event.target.value })} placeholder="例如：订阅购买" required /></label><label className="grid gap-1.5 text-xs font-medium text-muted-foreground">入口地址<input className={inputClassName} value={form.url} onChange={(event) => onChange({ ...form, url: event.target.value })} placeholder="https://example.com/purchase" inputMode="url" required /></label></div><label className="grid gap-1.5 text-xs font-medium text-muted-foreground">备注 / 来源（可选）<input className={inputClassName} value={form.note} onChange={(event) => onChange({ ...form, note: event.target.value })} placeholder="例如：用户中心" /></label>{error ? <div className="text-xs text-danger-foreground">{error}</div> : null}<div className="flex justify-end gap-2"><Button variant="outline" size="sm" onClick={onCancel}>取消</Button><Button type="submit" size="sm">{editing ? "保存" : "添加入口"}</Button></div></form>;
}

export function detectProvider(station: Pick<Station, "name" | "websiteUrl">): RechargeProvider {
  const identity = `${station.name} ${station.websiteUrl}`.toLowerCase();
  if (identity.includes("链动") || identity.includes("liandong") || identity.includes("chain")) return "liandong";
  if (identity.includes("云猫") || identity.includes("yuncat") || identity.includes("cloudcat")) return "cloudcat";
  return "custom";
}

export function parseRechargeRun(result: CollectorRunResult): { status: RechargeScanStatus; entries: RechargeEntry[]; message: string } { return parseRechargeSnapshot(result.snapshot); }

type RechargeScanStatus = "success" | "manual_required" | "no_match" | "not_found" | "error";

export function parseRechargeSnapshot(snapshot: CollectorSnapshot): { status: RechargeScanStatus; entries: RechargeEntry[]; message: string } {
  const summary = isRecord(snapshot.summaryJson) ? snapshot.summaryJson : {};
  const normalized = isRecord(snapshot.normalizedJson) ? snapshot.normalizedJson : {};
  const rawStatus = typeof summary.status === "string" ? summary.status : snapshot.status;
  const status: RechargeScanStatus = rawStatus === "login_required" || snapshot.status === "manual_required" ? "manual_required" : rawStatus === "success" ? "success" : rawStatus === "not_found" ? "not_found" : rawStatus === "no_match" || snapshot.status === "partial" ? "no_match" : snapshot.status === "failed" ? "error" : "no_match";
  const entries = Array.isArray(normalized.entries) ? normalized.entries.flatMap((value) => parseEntry(value, summary)) : [];
  const message = snapshot.errorMessage ?? (status === "success" ? `发现 ${entries.length} 个充值入口` : status === "manual_required" ? "请先完成站点登录或浏览器授权。" : status === "not_found" ? "页面明确返回 404，未生成充值入口。" : "已读取登录页面，但未发现可确认的充值入口。");
  return { status, entries, message };
}

function parseEntry(value: unknown, summary: Record<string, unknown>): RechargeEntry[] {
  if (!isRecord(value) || typeof value.url !== "string") return [];
  const url = sanitizeRechargeUrl(value.url);
  if (!url) return [];
  const provider = value.provider === "liandong" || value.provider === "cloudcat" ? value.provider : summary.provider === "liandong" || summary.provider === "cloudcat" ? summary.provider : "custom";
  const paymentMethods = Array.isArray(value.paymentMethods) ? value.paymentMethods.filter((item): item is string => typeof item === "string") : [];
  return [{ url, label: typeof value.label === "string" && value.label.trim() ? value.label : "充值入口", provider, paymentMethods, source: "confirmed" }];
}

function normalizeManualUrl(value: string): string | null {
  return sanitizeRechargeUrl(value.trim());
}

function isStationShellUrl(value: string, stationUrl: string): boolean {
  try {
    const entryUrl = new URL(value);
    const originUrl = new URL(stationUrl);
    const entryHost = entryUrl.hostname.replace(/^www\./i, "").toLowerCase();
    const originHost = originUrl.hostname.replace(/^www\./i, "").toLowerCase();
    return entryHost === originHost
      && ["", "/", "/home", "/dashboard", "/index.html"].includes(entryUrl.pathname.toLowerCase());
  } catch {
    return false;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> { return typeof value === "object" && value !== null && !Array.isArray(value); }
