import type { ReactNode } from "react";
import { IconButton, Tooltip } from "@radix-ui/themes";
import { QuestionMarkCircledIcon } from "@radix-ui/react-icons";

interface HelpTooltipProps {
  ariaLabel: string;
  content: ReactNode;
}

export function HelpTooltip({ ariaLabel, content }: HelpTooltipProps) {
  return (
    <Tooltip content={content}>
      <IconButton
        type="button"
        size="1"
        variant="ghost"
        color="gray"
        aria-label={ariaLabel}
      >
        <QuestionMarkCircledIcon width={14} height={14} />
      </IconButton>
    </Tooltip>
  );
}
