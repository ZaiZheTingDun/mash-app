export interface ServantAction {
  type: "servant";
  id: string;
  servant: string | null;
  skill: string | null;
  target: string | null;
}

export interface EquipmentAction {
  type: "equipment";
  id: string;
  skill: string | null;
  target: string | null;
}

export type CommandSpell = "np_release" | "restore";

export interface CommandSpellAction {
  type: "commandSpell";
  id: string;
  spell: CommandSpell | null;
  target: string | null;
}

export interface AttackCard {
  id: string;
  card: string | null;
}

export interface BattleScene {
  id: string;
  servantActions: ServantAction[];
  equipmentActions: EquipmentAction[];
  commandSpellActions: CommandSpellAction[];
  attackPriority: AttackCard[];
}
