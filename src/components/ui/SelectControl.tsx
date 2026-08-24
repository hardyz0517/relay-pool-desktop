import {
  Fragment,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { Check, ChevronDown, Search } from "lucide-react";
import { useInteractionActivity } from "@/components/ui/InteractionActivity";
import { cn } from "@/lib/utils";

export type SelectOption<T extends string = string> = {
  value: T;
  label: ReactNode;
  triggerLabel?: ReactNode;
  description?: ReactNode;
  descriptionPlacement?: "below" | "end";
  leadingIcon?: ReactNode;
  sectionLabel?: ReactNode;
  disabled?: boolean;
};

type SelectControlProps<T extends string = string> = {
  value: T;
  options: SelectOption<T>[];
  onChange: (value: T) => void;
  ariaLabel?: string;
  title?: string;
  placeholder?: ReactNode;
  searchable?: boolean;
  searchPlaceholder?: string;
  emptyLabel?: ReactNode;
  disabled?: boolean;
  className?: string;
  menuClassName?: string;
  menuAlign?: "start" | "end";
  menuMinWidth?: number;
};

type MenuPosition = {
  bottom: number | null;
  left: number;
  top: number | null;
  width: number;
  maxHeight: number;
};

const MIN_MENU_WIDTH = 160;
const MAX_MENU_HEIGHT = 320;

export function SelectControl<T extends string>({
  value,
  options,
  onChange,
  ariaLabel,
  title,
  placeholder = "请选择",
  searchable = false,
  searchPlaceholder = "搜索...",
  emptyLabel = "无匹配项",
  disabled = false,
  className,
  menuClassName,
  menuAlign = "start",
  menuMinWidth = MIN_MENU_WIDTH,
}: SelectControlProps<T>) {
  const interactionActive = useInteractionActivity();
  const id = useId();
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const menuRef = useRef<HTMLDivElement | null>(null);
  const searchInputRef = useRef<HTMLInputElement | null>(null);
  const optionRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const [open, setOpen] = useState(false);
  const [activeIndex, setActiveIndex] = useState(0);
  const [searchQuery, setSearchQuery] = useState("");
  const [position, setPosition] = useState<MenuPosition | null>(null);

  const filteredOptions = useMemo(() => {
    if (!searchable || !searchQuery.trim()) {
      return options;
    }
    const query = searchQuery.trim().toLocaleLowerCase();
    return options.filter((option) => {
      const label = typeof option.label === "string" || typeof option.label === "number"
        ? String(option.label)
        : "";
      return option.value.toLocaleLowerCase().includes(query) || label.toLocaleLowerCase().includes(query);
    });
  }, [options, searchable, searchQuery]);

  const selectedIndex = useMemo(
    () => options.findIndex((option) => option.value === value),
    [options, value],
  );
  const selectedOption = selectedIndex >= 0 ? options[selectedIndex] : null;

  useLayoutEffect(() => {
    if (interactionActive) {
      return;
    }
    setOpen(false);
    setPosition(null);
  }, [interactionActive]);

  useLayoutEffect(() => {
    if (!open) {
      return;
    }
    updatePosition();
  }, [filteredOptions.length, menuAlign, menuMinWidth, open, searchable]);

  useEffect(() => {
    if (!open) {
      return;
    }
    setSearchQuery("");
    if (searchable) {
      searchInputRef.current?.focus();
    }
  }, [open, searchable]);

  useEffect(() => {
    if (!open) {
      return;
    }
    const selectedFilteredIndex = filteredOptions.findIndex((option) => option.value === value);
    const initialIndex = selectedFilteredIndex >= 0 ? selectedFilteredIndex : firstEnabledIndex(filteredOptions);
    setActiveIndex(initialIndex);
  }, [filteredOptions, open, value]);

  useEffect(() => {
    if (!open) {
      return;
    }
    setActiveIndex((current) => Math.min(current, Math.max(0, filteredOptions.length - 1)));
  }, [filteredOptions.length, open]);

  useEffect(() => {
    if (!open) {
      return;
    }

    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (triggerRef.current?.contains(target) || menuRef.current?.contains(target)) {
        return;
      }
      setOpen(false);
    };
    const handleViewportResize = () => updatePosition();
    const handleViewportScroll = (event: Event) => {
      const target = event.target;
      if (target instanceof Node && menuRef.current?.contains(target)) {
        return;
      }
      setOpen(false);
    };

    document.addEventListener("pointerdown", handlePointerDown);
    window.addEventListener("resize", handleViewportResize);
    window.addEventListener("scroll", handleViewportScroll, true);
    window.addEventListener("wheel", handleViewportScroll, { capture: true, passive: true });
    window.addEventListener("touchmove", handleViewportScroll, { capture: true, passive: true });
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown);
      window.removeEventListener("resize", handleViewportResize);
      window.removeEventListener("scroll", handleViewportScroll, true);
      window.removeEventListener("wheel", handleViewportScroll, true);
      window.removeEventListener("touchmove", handleViewportScroll, true);
    };
  }, [open]);

  useEffect(() => {
    if (!open) {
      return;
    }
    const activeOption = optionRefs.current[activeIndex];
    if (activeOption && typeof activeOption.scrollIntoView === "function") {
      activeOption.scrollIntoView({ block: "nearest" });
    }
  }, [activeIndex, open]);

  function updatePosition() {
    const rect = triggerRef.current?.getBoundingClientRect();
    if (!rect) {
      return;
    }
    const gap = 6;
    const viewportPadding = 10;
    const spaceBelow = Math.max(0, window.innerHeight - rect.bottom - viewportPadding - gap);
    const spaceAbove = Math.max(0, rect.top - viewportPadding - gap);
    const desiredMenuHeight = estimateMenuHeight(filteredOptions, MAX_MENU_HEIGHT, searchable);
    const openAbove = spaceBelow < desiredMenuHeight && spaceAbove > spaceBelow;
    const maxHeight = Math.min(MAX_MENU_HEIGHT, openAbove ? spaceAbove : spaceBelow);
    // A caller may request a wider menu for rich option labels, but it must remain inside
    // the viewport so the portal does not create an unreachable horizontal overflow.
    const menuWidth = Math.min(
      Math.max(rect.width, menuMinWidth),
      Math.max(0, window.innerWidth - viewportPadding * 2),
    );
    const preferredLeft = menuAlign === "end" ? rect.right - menuWidth : rect.left;

    setPosition({
      bottom: openAbove ? window.innerHeight - rect.top + gap : null,
      left: Math.max(
        viewportPadding,
        Math.min(preferredLeft, window.innerWidth - menuWidth - viewportPadding),
      ),
      top: openAbove ? null : rect.bottom + gap,
      width: menuWidth,
      maxHeight,
    });
  }

  function handleTriggerKeyDown(event: KeyboardEvent<HTMLButtonElement>) {
    if (disabled) {
      return;
    }
    if (open && event.key === "Escape") {
      event.preventDefault();
      setOpen(false);
      return;
    }
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      setOpen(true);
      setActiveIndex((current) => nextEnabledIndex(filteredOptions, current, event.key === "ArrowDown" ? 1 : -1));
      return;
    }
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      if (open) {
        chooseOption(filteredOptions[activeIndex]);
        return;
      }
      setOpen((current) => !current);
    }
  }

  function handleMenuKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (event.key === "Escape") {
      event.preventDefault();
      setOpen(false);
      triggerRef.current?.focus();
      return;
    }
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      setActiveIndex((current) => nextEnabledIndex(filteredOptions, current, event.key === "ArrowDown" ? 1 : -1));
      return;
    }
    if (event.key === "Home") {
      event.preventDefault();
      setActiveIndex(firstEnabledIndex(filteredOptions));
      return;
    }
    if (event.key === "End") {
      event.preventDefault();
      setActiveIndex(lastEnabledIndex(filteredOptions));
      return;
    }
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      chooseOption(filteredOptions[activeIndex]);
    }
  }

  function chooseOption(option: SelectOption<T> | undefined) {
    if (!option || option.disabled) {
      return;
    }
    onChange(option.value);
    setOpen(false);
    triggerRef.current?.focus();
  }

  const listboxId = `${id}-listbox`;
  const activeId = `${id}-option-${activeIndex}`;

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        aria-activedescendant={open && filteredOptions.length > 0 ? activeId : undefined}
        aria-controls={open ? listboxId : undefined}
        aria-expanded={open}
        aria-haspopup="listbox"
        aria-label={ariaLabel}
        title={title}
        disabled={disabled}
        onClick={() => !disabled && setOpen((current) => !current)}
        onKeyDown={handleTriggerKeyDown}
        className={cn(
          "inline-flex h-8 min-w-[132px] cursor-pointer items-center justify-between gap-2 rounded-[var(--surface-radius)] border border-border bg-surface px-3 text-left text-sm text-foreground shadow-surface outline-none transition duration-150 hover:border-ring/30 hover:bg-hover focus:border-ring/40 focus:ring-2 focus:ring-ring/20 disabled:cursor-not-allowed disabled:opacity-60",
          open && "border-ring/40 bg-surface ring-2 ring-ring/20",
          className,
        )}
      >
        <span className="flex min-w-0 items-center gap-1.5">
          {selectedOption?.leadingIcon ? (
            <span className="shrink-0 text-muted-foreground">{selectedOption.leadingIcon}</span>
          ) : null}
          <span className="min-w-0 truncate">
            {selectedOption?.triggerLabel ?? selectedOption?.label ?? placeholder}
          </span>
        </span>
        <ChevronDown
          className={cn(
            "h-4 w-4 shrink-0 text-muted-foreground transition-transform duration-150",
            open && "rotate-180 text-foreground",
          )}
        />
      </button>

      {open && position && createPortal(
        <div
          ref={menuRef}
          id={listboxId}
          role="listbox"
          tabIndex={-1}
          aria-label={ariaLabel}
          onKeyDown={handleMenuKeyDown}
          className={cn(
            "fixed z-[80] overflow-auto rounded-[var(--surface-radius)] border border-border bg-popover p-1 text-sm text-foreground shadow-popover outline-none [scrollbar-width:none] motion-safe:animate-[selectMenuIn_140ms_ease-out] [&::-webkit-scrollbar]:hidden",
            menuClassName,
          )}
          style={{
            bottom: position.bottom ?? undefined,
            left: position.left,
            top: position.top ?? undefined,
            width: position.width,
            maxHeight: position.maxHeight,
          }}
        >
          {searchable ? (
            <div className="sticky top-0 z-[1] border-b border-border bg-popover pb-1">
              <div className="relative">
                <Search className="pointer-events-none absolute left-2.5 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
                <input
                  ref={searchInputRef}
                  type="search"
                  value={searchQuery}
                  aria-label={`${ariaLabel ?? "选项"} 搜索`}
                  placeholder={searchPlaceholder}
                  onChange={(event) => setSearchQuery(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === " ") {
                      event.stopPropagation();
                    }
                  }}
                  className="h-8 w-full rounded-[calc(var(--surface-radius)-3px)] border border-input bg-background px-2.5 pl-8 text-sm text-foreground outline-none placeholder:text-muted-foreground/70 focus:border-input focus:ring-0"
                />
              </div>
            </div>
          ) : null}
          {filteredOptions.length === 0 ? (
            <div className="px-2.5 py-3 text-center text-xs text-muted-foreground" role="status">
              {emptyLabel}
            </div>
          ) : null}
          {filteredOptions.map((option, index) => {
            const selected = option.value === value;
            const active = index === activeIndex;
            return (
              <Fragment key={option.value}>
                {option.sectionLabel ? (
                  <div
                    role="presentation"
                    className="mt-1 border-t border-border px-2.5 pb-1 pt-2 text-[10px] font-medium text-muted-foreground"
                  >
                    {option.sectionLabel}
                  </div>
                ) : null}
                <button
                  ref={(node) => {
                    optionRefs.current[index] = node;
                  }}
                  id={`${id}-option-${index}`}
                  type="button"
                  role="option"
                  aria-selected={selected}
                  disabled={option.disabled}
                  onMouseEnter={() => !option.disabled && setActiveIndex(index)}
                  onClick={() => chooseOption(option)}
                  className={cn(
                    "flex min-h-8 w-full cursor-pointer items-center justify-between gap-3 rounded-[calc(var(--surface-radius)-3px)] px-2.5 py-1.5 text-left transition-colors duration-100 disabled:cursor-not-allowed disabled:opacity-45",
                    active ? "bg-selected text-selected-foreground" : "hover:bg-hover",
                    selected && "font-medium",
                  )}
                >
                  <span className="flex min-w-0 flex-1 items-center gap-2">
                    {option.leadingIcon ? (
                      <span className="shrink-0 text-muted-foreground">{option.leadingIcon}</span>
                    ) : null}
                    <span
                      className={cn(
                        "min-w-0 flex-1",
                        option.descriptionPlacement === "end" && "flex items-center justify-between gap-3",
                      )}
                    >
                      <span className="min-w-0 truncate">{option.label}</span>
                      {option.description ? (
                        <span
                          className={cn(
                            "truncate text-xs font-normal text-muted-foreground",
                            option.descriptionPlacement === "end"
                              ? "shrink-0"
                              : "mt-0.5 block",
                          )}
                        >
                          {option.description}
                        </span>
                      ) : null}
                    </span>
                  </span>
                  {selected ? <Check className="h-4 w-4 shrink-0 text-primary" /> : null}
                </button>
              </Fragment>
            );
          })}
        </div>,
        document.body,
      )}
    </>
  );
}

function firstEnabledIndex(options: SelectOption[]) {
  const index = options.findIndex((option) => !option.disabled);
  return index >= 0 ? index : 0;
}

function lastEnabledIndex(options: SelectOption[]) {
  for (let index = options.length - 1; index >= 0; index -= 1) {
    if (!options[index].disabled) {
      return index;
    }
  }
  return 0;
}

function nextEnabledIndex(options: SelectOption[], startIndex: number, direction: 1 | -1) {
  if (options.length === 0) {
    return 0;
  }
  let index = startIndex;
  for (let step = 0; step < options.length; step += 1) {
    index = (index + direction + options.length) % options.length;
    if (!options[index].disabled) {
      return index;
    }
  }
  return startIndex;
}

function estimateMenuHeight(options: SelectOption[], maxHeight: number, searchable = false) {
  const menuPadding = 8;
  const optionHeight = 40;
  const searchHeight = searchable ? 40 : 0;
  const estimatedContentHeight = options.length * optionHeight + menuPadding + searchHeight;
  return Math.min(maxHeight, Math.max(optionHeight + menuPadding, estimatedContentHeight));
}
