export interface ServantAction {
  type: "servant";
  id: string;
  servant: string | null;
  skill: string | null;
  target: string | null;
}

export interface OrderChangeSelection {
  front: string | null;
  back: string | null;
}

export interface EquipmentAction {
  type: "equipment";
  id: string;
  skill: string | null;
  target: string | null;
  orderChange?: OrderChangeSelection | null;
}

export type CommandSpell = "np_release" | "restore";

export interface CommandSpellAction {
  type: "commandSpell";
  id: string;
  spell: CommandSpell | null;
  target: string | null;
}

export type PreparationAction =
  | ServantAction
  | EquipmentAction
  | CommandSpellAction;

export interface AttackCard {
  id: string;
  card: string | null;
}

export interface BattleScene {
  id: string;
  preparationActions: PreparationAction[];
  servantActions: ServantAction[];
  equipmentActions: EquipmentAction[];
  commandSpellActions: CommandSpellAction[];
  attackPriority: AttackCard[];
}

export interface AdvancedNpSlotCondition {
  servant: "servant_1" | "servant_2" | "servant_3";
  ready: boolean;
}

export interface AdvancedNpConditionGroup {
  id: string;
  slots: AdvancedNpSlotCondition[];
}

export interface AdvancedCommandCardCondition {
  slot: number;
  servant: "servant_1" | "servant_2" | "servant_3" | "any";
  suit: "buster" | "arts" | "quick" | "any";
  minCritChance: number | null;
}

export interface AdvancedCommandConditionGroup {
  id: string;
  cards: AdvancedCommandCardCondition[];
}

export interface AdvancedAttackAction {
  type: "attack";
  id: string;
  card: string | null;
}

export type AdvancedAction = PreparationAction | AdvancedAttackAction;

export interface AdvancedRule {
  id: string;
  npConditionGroups: AdvancedNpConditionGroup[];
  commandConditionGroups: AdvancedCommandConditionGroup[];
  actions: AdvancedAction[];
}

export type AdvancedOutputType = "np" | "critical";

export interface AdvancedMainOutput {
  servant: "servant_1" | "servant_2" | "servant_3" | null;
  outputType: AdvancedOutputType | null;
  npCard?: "auto" | "buster" | "arts" | "quick" | null;
}

export interface AdvancedBattleScene {
  id: string;
  mainOutput?: AdvancedMainOutput | null;
  commandConditions?: AdvancedCommandCardCondition[];
  controlActions?: PreparationAction[];
  startupActions?: PreparationAction[];
  rules: AdvancedRule[];
}
