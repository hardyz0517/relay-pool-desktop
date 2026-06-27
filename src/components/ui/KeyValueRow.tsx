import type { ReactNode } from "react";

type KeyValueRowProps = {
  label: string;
  value: ReactNode;
};

export function KeyValueRow({ label, value }: KeyValueRowProps) {
  return (
    <div className="grid grid-cols-[112px_minmax(0,1fr)] gap-3 border-b border-border py-2 text-[13px] last:border-b-0">
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd className="min-w-0 text-slate-700">{value}</dd>
    </div>
  );
}
