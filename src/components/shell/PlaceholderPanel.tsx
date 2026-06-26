type PlaceholderPanelProps = {
  title: string;
  items: string[];
};

export function PlaceholderPanel({ title, items }: PlaceholderPanelProps) {
  return (
    <div className="rounded-lg border border-border bg-[#111821]">
      <div className="border-b border-border px-4 py-3 text-sm font-medium">
        {title}
      </div>
      <div className="grid gap-2 p-4 md:grid-cols-2">
        {items.map((item) => (
          <div
            key={item}
            className="rounded-md border border-border bg-background/35 px-3 py-2 text-sm text-muted-foreground"
          >
            {item}
          </div>
        ))}
      </div>
    </div>
  );
}
