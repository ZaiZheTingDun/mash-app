export type SkillSelectionType =
  | "SelectAddInfo"
  | "selectTreasureDeviceInfo"
  | "commandTypeSelfTreasureDevice";

export interface SkillSelection {
  type: SkillSelectionType;
  index: number;
  optionCount?: number | null;
  label?: string | null;
}

export interface ServantAction {
  type: "servant";
  id: string;
  servant: string | null;
  servantMemberId?: string | null;
  servantId?: number | null;
  servantIsSupport?: boolean;
  skill: string | null;
  skillSelection?: SkillSelection | null;
  target: string | null;
  targetMemberId?: string | null;
  targetServantId?: number | null;
  targetIsSupport?: boolean;
}

export interface OrderChangeSelection {
  front: string | null;
  frontMemberId?: string | null;
  frontServantId?: number | null;
  frontIsSupport?: boolean;
  back: string | null;
  backMemberId?: string | null;
  backServantId?: number | null;
  backIsSupport?: boolean;
}

export interface EquipmentAction {
  type: "equipment";
  id: string;
  skill: string | null;
  target: string | null;
  targetMemberId?: string | null;
  targetServantId?: number | null;
  targetIsSupport?: boolean;
  orderChange?: OrderChangeSelection | null;
}

export type CommandSpell = "np_release" | "restore";

export interface CommandSpellAction {
  type: "commandSpell";
  id: string;
  spell: CommandSpell | null;
  target: string | null;
  targetMemberId?: string | null;
  targetServantId?: number | null;
  targetIsSupport?: boolean;
}

export type PreparationAction =
  | ServantAction
  | EquipmentAction
  | CommandSpellAction;

export interface AttackCard {
  id: string;
  card: string | null;
  memberId?: string | null;
  servantId?: number | null;
  isSupport?: boolean;
}

export interface BattleTurn {
  id: string;
  preparationActions: PreparationAction[];
  servantActions: ServantAction[];
  equipmentActions: EquipmentAction[];
  commandSpellActions: CommandSpellAction[];
  enemyTarget?: string | null;
  attackPriority: AttackCard[];
}

export interface BattleScene {
  id: string;
  turns: BattleTurn[];
  preparationActions?: PreparationAction[];
  servantActions?: ServantAction[];
  equipmentActions?: EquipmentAction[];
  commandSpellActions?: CommandSpellAction[];
  enemyTarget?: string | null;
  attackPriority?: AttackCard[];
}

export interface AdvancedNpSlotCondition {
  servant: "servant_1" | "servant_2" | "servant_3";
  memberId?: string | null;
  servantId?: number | null;
  isSupport?: boolean;
  ready: boolean;
}

export interface AdvancedNpConditionGroup {
  id: string;
  slots: AdvancedNpSlotCondition[];
}

export interface AdvancedCommandCardCondition {
  slot: number;
  servant: "servant_1" | "servant_2" | "servant_3" | "any";
  memberId?: string | null;
  servantId?: number | null;
  isSupport?: boolean;
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
  memberId?: string | null;
  servantId?: number | null;
  isSupport?: boolean;
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
  memberId?: string | null;
  servant: "servant_1" | "servant_2" | "servant_3" | null;
  servantId?: number | null;
  isSupport?: boolean;
  outputType: AdvancedOutputType | null;
  npCard?: "auto" | "buster" | "arts" | "quick" | null;
}

export interface AdvancedBattleScene {
  id: string;
  enemyTarget?: string | null;
  mainOutput?: AdvancedMainOutput | null;
  grandAutoOrderChange?: boolean | null;
  commandConditions?: AdvancedCommandCardCondition[];
  controlActions?: PreparationAction[];
  startupActions?: PreparationAction[];
  rules: AdvancedRule[];
}
