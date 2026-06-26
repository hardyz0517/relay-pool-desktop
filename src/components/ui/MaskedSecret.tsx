type MaskedSecretProps = {
  value: string;
};

export function MaskedSecret({ value }: MaskedSecretProps) {
  return (
    <code className="rounded border border-border bg-slate-50 px-1.5 py-0.5 text-xs text-slate-700">
      {value}
    </code>
  );
}
