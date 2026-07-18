import { ChevronLeft, ChevronRight } from "lucide-react";

export type PaginationItem = number | "ellipsis";

type PaginationProps = {
  ariaLabel: string;
  page: number;
  totalPages: number;
  disabled?: boolean;
  onPageChange: (page: number) => void;
};

export function buildPaginationItems(page: number, totalPages: number): PaginationItem[] {
  const safeTotalPages = Math.max(1, Math.floor(totalPages));
  const safePage = Math.min(Math.max(1, Math.floor(page)), safeTotalPages);
  const visiblePages = new Set<number>([1, safeTotalPages]);

  for (let candidate = safePage - 2; candidate <= safePage + 2; candidate += 1) {
    if (candidate >= 1 && candidate <= safeTotalPages) {
      visiblePages.add(candidate);
    }
  }

  const pages = [...visiblePages].sort((left, right) => left - right);
  const items: PaginationItem[] = [];

  pages.forEach((visiblePage, index) => {
    const previousPage = pages[index - 1];
    if (previousPage !== undefined && visiblePage - previousPage > 1) {
      if (visiblePage - previousPage === 2) {
        items.push(previousPage + 1);
      } else {
        items.push("ellipsis");
      }
    }
    items.push(visiblePage);
  });

  return items;
}

export function Pagination({ ariaLabel, page, totalPages, disabled = false, onPageChange }: PaginationProps) {
  const safeTotalPages = Math.max(1, Math.floor(totalPages));
  const safePage = Math.min(Math.max(1, Math.floor(page)), safeTotalPages);
  const items = buildPaginationItems(safePage, safeTotalPages);

  return (
    <nav className="flex items-center" aria-label={ariaLabel}>
      <button
        type="button"
        aria-label="上一页"
        title="上一页"
        disabled={disabled || safePage <= 1}
        onClick={() => onPageChange(safePage - 1)}
        className="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-l-[4px] border border-border bg-surface text-muted-foreground transition-colors hover:bg-surface-subtle hover:text-foreground focus-visible:z-10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30 disabled:cursor-not-allowed disabled:text-muted-foreground/45"
      >
        <ChevronLeft className="h-4 w-4" aria-hidden="true" />
      </button>

      {items.map((item, index) => item === "ellipsis" ? (
        <span
          key={`ellipsis-${index}`}
          aria-hidden="true"
          className="-ml-px inline-flex h-8 min-w-9 items-center justify-center border border-border bg-surface px-2 text-muted-foreground"
        >
          ...
        </span>
      ) : (
        <button
          key={item}
          type="button"
          aria-label={`第 ${item} 页`}
          aria-current={item === safePage ? "page" : undefined}
          disabled={disabled}
          onClick={() => onPageChange(item)}
          className={`-ml-px inline-flex h-8 min-w-9 items-center justify-center border px-2 text-sm transition-colors focus-visible:z-20 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30 disabled:cursor-not-allowed ${
            item === safePage
              ? "z-10 border-primary bg-info-surface font-medium text-info-foreground"
              : "border-border bg-surface text-foreground hover:bg-surface-subtle"
          }`}
        >
          {item}
        </button>
      ))}

      <button
        type="button"
        aria-label="下一页"
        title="下一页"
        disabled={disabled || safePage >= safeTotalPages}
        onClick={() => onPageChange(safePage + 1)}
        className="-ml-px inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-r-[4px] border border-border bg-surface text-muted-foreground transition-colors hover:bg-surface-subtle hover:text-foreground focus-visible:z-10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30 disabled:cursor-not-allowed disabled:text-muted-foreground/45"
      >
        <ChevronRight className="h-4 w-4" aria-hidden="true" />
      </button>
    </nav>
  );
}
