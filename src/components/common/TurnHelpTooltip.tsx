import { HelpTooltip } from "./HelpTooltip";

export const TURN_HELP_TEXT = "此处为同一面的不同轮次，其他面战斗请点击上方按钮";

export function TurnHelpTooltip() {
  return <HelpTooltip ariaLabel="Turn 帮助" content={TURN_HELP_TEXT} />;
}
