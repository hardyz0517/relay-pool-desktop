import { StationGroupOptionLabel, StationGroupTriggerLabel } from "@/components/group/StationGroupChip";
import { SelectControl } from "@/components/ui";
import type { StationGroupOption } from "@/lib/types/groupFacts";
import { noGroupOptionValue, stationGroupSelectValue } from "@/lib/groupOptionViewModels";
import { cn } from "@/lib/utils";

type StationGroupSelectControlProps = {
  ariaLabel: string;
  groups: StationGroupOption[];
  value: string;
  onChange: (value: string) => void;
  noGroupLabel: string;
  noGroupDescription: string;
  className?: string;
  disabled?: boolean;
  menuClassName?: string;
};

export function StationGroupSelectControl({
  ariaLabel,
  groups,
  value,
  onChange,
  noGroupLabel,
  noGroupDescription,
  className,
  disabled,
  menuClassName,
}: StationGroupSelectControlProps) {
  const options = [
    {
      value: noGroupOptionValue,
      label: noGroupLabel,
      description: noGroupDescription,
      descriptionPlacement: "end" as const,
    },
    ...groups.map((group) => ({
      value: stationGroupSelectValue(group),
      label: <StationGroupOptionLabel option={group} />,
      triggerLabel: <StationGroupTriggerLabel option={group} />,
    })),
  ];

  return (
    <SelectControl
      ariaLabel={ariaLabel}
      className={className}
      disabled={disabled}
      menuClassName={cn(
        "text-xs [scrollbar-width:none] [&::-webkit-scrollbar]:hidden",
        menuClassName,
      )}
      menuMinWidth={420}
      options={options}
      value={value}
      onChange={onChange}
    />
  );
}
