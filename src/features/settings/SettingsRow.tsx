import type { ReactNode } from "react";
import { Text } from "@radix-ui/themes";

export function SettingsRow({ label, description, help, children }: {
  label: string;
  description: string;
  help?: ReactNode;
  children: ReactNode;
}) {
  return <div className="basic-setting-row">
    <div className="basic-setting-copy">
      <Text size="2" weight="bold">{label}</Text>
      <Text size="1" color="gray">{description}</Text>
      {help}
    </div>
    <div className="basic-setting-control">{children}</div>
  </div>;
}
