import { Text } from "@radix-ui/themes";
import { ChevronRightIcon } from "@radix-ui/react-icons";
import type { ReactNode } from "react";

/**
 * Collapsible group used throughout the debug UI. Built on the native
 * `<details>` element so it needs zero React state and is naturally
 * keyboard-accessible. The chevron rotates via a CSS rule on
 * `.debug-section[open] > summary > .debug-section-chevron`.
 */
export function DebugSection({
  title,
  defaultOpen = false,
  badge,
  children,
}: {
  title: string;
  defaultOpen?: boolean;
  badge?: ReactNode;
  children: ReactNode;
}) {
  return (
    <details className="debug-section" open={defaultOpen}>
      <summary className="debug-section-summary">
        <ChevronRightIcon
          className="debug-section-chevron"
          width={14}
          height={14}
        />
        <Text size="2" weight="medium">
          {title}
        </Text>
        {badge !== undefined && badge !== null && (
          <Text size="1" color="gray" className="debug-section-badge">
            {badge}
          </Text>
        )}
      </summary>
      <div className="debug-section-body">{children}</div>
    </details>
  );
}
