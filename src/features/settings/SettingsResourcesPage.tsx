import { ResourceManagementPanel } from "../setup/SetupPage";

interface SettingsResourcesPageProps {
  onExitBlockedChange?: (blocked: boolean) => void;
}

export function SettingsResourcesPage({ onExitBlockedChange }: SettingsResourcesPageProps) {
  return (
    <ResourceManagementPanel
      mode="manage"
      embedded
      onExitBlockedChange={onExitBlockedChange}
    />
  );
}
