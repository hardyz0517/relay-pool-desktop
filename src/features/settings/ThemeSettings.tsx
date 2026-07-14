import { Monitor, Moon, Sun, type LucideIcon } from "lucide-react";
import { SectionCard, SegmentedControl, useToast } from "@/components/ui";
import { useTheme } from "@/theme/ThemeProvider";
import type { ThemePreference } from "@/theme/theme";

const themeOptions = [
  { value: "light", label: "日间", icon: Sun },
  { value: "dark", label: "夜间", icon: Moon },
  { value: "system", label: "跟随系统", icon: Monitor },
] satisfies Array<{ value: ThemePreference; label: string; icon: LucideIcon }>;

export function ThemeSettings() {
  const toast = useToast();
  const { preference, setPreference } = useTheme();

  function handleChange(next: ThemePreference) {
    const result = setPreference(next);
    if (!result.persisted) {
      toast.error(
        "主题偏好无法保存",
        "主题已切换，但偏好无法保存；重启后可能恢复上次设置。",
      );
    }
  }

  return (
    <SectionCard contentClassName="px-5 py-4" title="外观">
      <SegmentedControl
        ariaLabel="外观模式"
        className="w-full max-w-[360px]"
        options={themeOptions}
        value={preference}
        onChange={handleChange}
      />
    </SectionCard>
  );
}
