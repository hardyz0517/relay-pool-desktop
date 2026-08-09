import { ArrowLeft } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { AlertingSettings } from "./AlertingSettings";
import { IconButton } from "@/components/ui";

type ChangeCenterSettingsPageProps = {
  onBack: () => void;
};

/** Alert lifecycle and notification configuration owned by Change Center. */
export function ChangeCenterSettingsPage({ onBack }: ChangeCenterSettingsPageProps) {
  return (
    <PageScaffold
      title="变更中心设置"
      width="settings"
      stickyHeader
      backAction={
        <IconButton label="返回变更中心" onClick={onBack}>
          <ArrowLeft className="h-4 w-4" />
        </IconButton>
      }
    >
      <AlertingSettings />
    </PageScaffold>
  );
}
