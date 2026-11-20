import { HelpTooltip } from "./HelpTooltip";

export const TURN_HELP_TEXT = "此处为同一面的不同轮次，其他面战斗请点击上方按钮";

export function TurnHelpTooltip({ advanced = false }: { advanced?: boolean }) {
  return <HelpTooltip
    ariaLabel="Turn 帮助"
    content={advanced ? "超过已配置的回合数后，会沿用最后一回合的攻击设置。" : TURN_HELP_TEXT}
  />;
}
