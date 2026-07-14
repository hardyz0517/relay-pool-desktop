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
import { Check, ChevronDown } from "lucide-react";
import { useInteractionActivity } from "@/components/ui/InteractionActivity";
import { cn } from "@/lib/utils";

export type SelectOption<T extends string = string> = {
  value: T;
  label: ReactNode;
  triggerLabel?: ReactNode;
  description?: ReactNode;
  leadingIcon?: ReactNode;
  sectionLabel?: ReactNode;
  disabled?: boolean;
};

type SelectControlProps<T extends string = string> = {
  value: T;
  options: SelectOption<T>[];
  onChange: (value: T) => void;
  ariaLabel?: string;
  placeholder?: ReactNode;
  disabled?: boolean;
  className?: string;
  menuClassName?: string;
};

type MenuPosition = {
  left: number;
  top: number;
  width: number;
  maxHeight: number;
};

const MIN_MENU_WIDTH = 160;

export function SelectControl<T extends string>({
  value,
  options,
  onChange,
  ariaLabel,
  placeholder = "请选择",
  disabled = false,
  className,
  menuClassName,
}: SelectControlProps<T>) {
  const interactionActive = useInteractionActivity();
  const id = useId();
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const menuRef = useRef<HTMLDivElement | null>(null);
  const optionRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const [open, setOpen] = useState(false);
  const [activeIndex, setActiveIndex] = useState(0);
  const [position, setPosition] = useState<MenuPosition | null>(null);

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
  }, [open, options.length]);

  useEffect(() => {
    if (!open) {
      return;
    }
    const initialIndex = selectedIndex >= 0 ? selectedIndex : firstEnabledIndex(options);
    setActiveIndex(initialIndex);
  }, [open, options, selectedIndex]);

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
    optionRefs.current[activeIndex]?.scrollIntoView({ block: "nearest" });
  }, [activeIndex, open]);

  function updatePosition() {
    const rect = triggerRef.current?.getBoundingClientRect();
    if (!rect) {
      return;
    }
    const gap = 6;
    const viewportPadding = 10;
    const spaceBelow = window.innerHeight - rect.bottom - viewportPadding;
    const spaceAbove = rect.top - viewportPadding;
    const maxHeight = Math.max(160, Math.min(320, Math.max(spaceBelow, spaceAbove) - gap));
    const openAbove = spaceBelow < 180 && spaceAbove > spaceBelow;
    const menuHeight = estimateMenuHeight(options, maxHeight);
    const menuWidth = Math.max(rect.width, MIN_MENU_WIDTH);
    const top = openAbove
      ? Math.max(viewportPadding, rect.top - menuHeight - gap)
      : Math.min(window.innerHeight - viewportPadding, rect.bottom + gap);

    setPosition({
      left: Math.max(viewportPadding, Math.min(rect.left, window.innerWidth - menuWidth - viewportPadding)),
      top,
      width: Math.max(rect.width, MIN_MENU_WIDTH),
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
      setActiveIndex((current) => nextEnabledIndex(options, current, event.key === "ArrowDown" ? 1 : -1));
      return;
    }
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      if (open) {
        chooseOption(options[activeIndex]);
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
      setActiveIndex((current) => nextEnabledIndex(options, current, event.key === "ArrowDown" ? 1 : -1));
      return;
    }
    if (event.key === "Home") {
      event.preventDefault();
      setActiveIndex(firstEnabledIndex(options));
      return;
    }
    if (event.key === "End") {
      event.preventDefault();
      setActiveIndex(lastEnabledIndex(options));
      return;
    }
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      chooseOption(options[activeIndex]);
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
        aria-activedescendant={open ? activeId : undefined}
        aria-controls={open ? listboxId : undefined}
        aria-expanded={open}
        aria-haspopup="listbox"
        aria-label={ariaLabel}
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
            "fixed z-[80] overflow-auto rounded-[var(--surface-radius)] border border-border bg-popover p-1 text-sm text-foreground shadow-popover outline-none motion-safe:animate-[selectMenuIn_140ms_ease-out]",
            menuClassName,
          )}
          style={{
            left: position.left,
            top: position.top,
            width: position.width,
            maxHeight: position.maxHeight,
          }}
        >
          {options.map((option, index) => {
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
                  <span className="flex min-w-0 items-center gap-2">
                    {option.leadingIcon ? (
                      <span className="shrink-0 text-muted-foreground">{option.leadingIcon}</span>
                    ) : null}
                    <span className="min-w-0">
                      <span className="block truncate">{option.label}</span>
                      {option.description ? (
                        <span className="mt-0.5 block truncate text-xs font-normal text-muted-foreground">
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

function estimateMenuHeight(options: SelectOption[], maxHeight: number) {
  const menuPadding = 8;
  const optionHeight = 40;
  const estimatedContentHeight = options.length * optionHeight + menuPadding;
  return Math.min(maxHeight, Math.max(optionHeight + menuPadding, estimatedContentHeight));
}
