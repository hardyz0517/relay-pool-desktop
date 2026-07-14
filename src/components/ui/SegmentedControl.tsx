import { useEffect, useMemo, useState, type KeyboardEvent } from "react";
import type { LucideIcon } from "lucide-react";
import { cn } from "@/lib/utils";

type SegmentedControlOption<T extends string> = {
  value: T;
  label: string;
  icon?: LucideIcon;
  disabled?: boolean;
};

type SegmentedControlProps<T extends string> = {
  value?: T;
  defaultValue?: T;
  options: SegmentedControlOption<T>[];
  onChange?: (value: T) => void;
  ariaLabel?: string;
  disabled?: boolean;
  className?: string;
};

export function SegmentedControl<T extends string>({
  value,
  defaultValue,
  options,
  onChange,
  ariaLabel = "分段切换",
  disabled = false,
  className,
}: SegmentedControlProps<T>) {
  const firstEnabledValue = options.find((option) => !option.disabled)?.value;
  const initialValue = value ?? defaultValue ?? firstEnabledValue;
  const [internalValue, setInternalValue] = useState<T | undefined>(initialValue);
  const selectedValue = onChange ? value : internalValue;
  const selectedIndex = Math.max(0, options.findIndex((option) => option.value === selectedValue));
  const canUse = !disabled && options.length > 0;

  useEffect(() => {
    if (!onChange) {
      setInternalValue(value ?? defaultValue ?? firstEnabledValue);
    }
  }, [defaultValue, firstEnabledValue, onChange, value]);

  const activeStyle = useMemo(
    () => ({
      width: `calc((100% - 4px) / ${Math.max(options.length, 1)})`,
      transform: `translateX(${selectedIndex * 100}%)`,
    }),
    [options.length, selectedIndex],
  );

  function selectOption(nextValue: T) {
    if (!canUse) {
      return;
    }
    const option = options.find((item) => item.value === nextValue);
    if (!option || option.disabled || nextValue === selectedValue) {
      return;
    }
    if (!onChange) {
      setInternalValue(nextValue);
    }
    onChange?.(nextValue);
  }

  function moveSelection(direction: 1 | -1) {
    if (!canUse) {
      return;
    }
    const enabledOptions = options.filter((option) => !option.disabled);
    const currentIndex = Math.max(0, enabledOptions.findIndex((option) => option.value === selectedValue));
    const nextIndex = (currentIndex + direction + enabledOptions.length) % enabledOptions.length;
    const nextOption = enabledOptions[nextIndex];
    if (nextOption) {
      selectOption(nextOption.value);
    }
  }

  function handleKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (event.key === "ArrowRight" || event.key === "ArrowDown") {
      event.preventDefault();
      moveSelection(1);
    }
    if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
      event.preventDefault();
      moveSelection(-1);
    }
  }

  return (
    <div
      role="radiogroup"
      aria-label={ariaLabel}
      aria-disabled={!canUse}
      onKeyDown={handleKeyDown}
      className={cn(
        "relative inline-grid h-8 min-w-0 items-center overflow-hidden rounded-[var(--surface-radius)] border border-border bg-muted p-0.5",
        className,
      )}
      style={{ gridTemplateColumns: `repeat(${Math.max(options.length, 1)}, minmax(0, 1fr))` }}
    >
      <span
        aria-hidden="true"
        className="pointer-events-none absolute left-0.5 top-0.5 h-[calc(100%-4px)] rounded-[calc(var(--surface-radius)-3px)] bg-control-thumb shadow-surface transition-transform duration-200 ease-out"
        style={activeStyle}
      />
      {options.map((option) => {
        const selected = option.value === selectedValue;
        const optionDisabled = disabled || option.disabled;
        const Icon = option.icon;

        return (
          <button
            key={option.value}
            type="button"
            role="radio"
            aria-checked={selected}
            disabled={optionDisabled}
            onClick={() => selectOption(option.value)}
            className={cn(
              "relative z-10 h-7 min-w-0 cursor-pointer rounded-[calc(var(--surface-radius)-3px)] px-3 text-xs font-medium leading-7 text-muted-foreground transition-colors duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30 disabled:cursor-default disabled:opacity-45",
              selected ? "text-foreground" : "hover:text-foreground",
            )}
          >
            <span className="flex min-w-0 items-center justify-center gap-1.5">
              {Icon ? <Icon aria-hidden="true" className="h-3.5 w-3.5 shrink-0" /> : null}
              <span className="truncate">{option.label}</span>
            </span>
          </button>
        );
      })}
    </div>
  );
}
