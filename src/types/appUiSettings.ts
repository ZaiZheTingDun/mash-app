export type BattleStartPanel = "operationLog" | "runStatus" | "none";

export const DEFAULT_BATTLE_START_PANEL: BattleStartPanel = "operationLog";

export function normalizeBattleStartPanel(value: unknown): BattleStartPanel {
  return value === "runStatus" || value === "none" ? value : DEFAULT_BATTLE_START_PANEL;
}
