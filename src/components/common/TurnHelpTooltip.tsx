import { IconButton, Tooltip } from "@radix-ui/themes";
import { QuestionMarkCircledIcon } from "@radix-ui/react-icons";

export const TURN_HELP_TEXT = "此处为同一面的不同轮次，其他面战斗请点击上方按钮";

export function TurnHelpTooltip() {
  return (
    <Tooltip content={TURN_HELP_TEXT}>
      <IconButton
        type="button"
        size="1"
        variant="ghost"
        color="gray"
        aria-label="Turn 帮助"
      >
        <QuestionMarkCircledIcon width={14} height={14} />
      </IconButton>
    </Tooltip>
  );
}
