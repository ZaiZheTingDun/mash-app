export type BattleStartPanel = "operationLog" | "runStatus" | "none";

export const DEFAULT_BATTLE_START_PANEL: BattleStartPanel = "operationLog";

export interface ServantPickerSettings {
  layout: "list" | "grid";
  gridSize: "small" | "medium" | "large";
}

export type ServantPickerAction = "toggleLayout" | "cycleGridSize";

export type MysticCodeGender = "female" | "male";
export const DEFAULT_MYSTIC_CODE_GENDER: MysticCodeGender = "female";

export function normalizeMysticCodeGender(value: unknown): MysticCodeGender {
  return value === "male" ? "male" : DEFAULT_MYSTIC_CODE_GENDER;
}

export function normalizeBattleStartPanel(value: unknown): BattleStartPanel {
  return value === "runStatus" || value === "none" ? value : DEFAULT_BATTLE_START_PANEL;
}
