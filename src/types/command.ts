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
}

export interface AttackCard {
  id: string;
  card: string | null;
}

export interface Turn {
  id: string;
  servantActions: ServantAction[];
  equipmentActions: EquipmentAction[];
  attackPriority: AttackCard[];
}
