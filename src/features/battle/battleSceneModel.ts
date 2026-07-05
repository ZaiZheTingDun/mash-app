import { servantLabel } from "../../components/common/battleActorLabels";
import type {
  AttackCard,
  BattleTurn,
  OrderChangeSelection,
} from "../../types/command";
import type { Servant } from "../../types/servant";

export type PrepSource = "equipment" | "commandSpell" | `servant_${1 | 2 | 3}`;
export type AttackSource = `servant_${1 | 2 | 3}`;
export type PartySlot = `servant_${1 | 2 | 3 | 4 | 5 | 6}`;
export type EnemyTarget = `enemy_${1 | 2 | 3 | 4 | 5 | 6}`;
export type PrepDraft =
  | { step: "source" }
  | { step: "option"; source: PrepSource }
  | { step: "target"; source: PrepSource; option: string; allowNoTarget?: boolean }
  | {
    step: "orderChange";
    source: "equipment";
    option: string;
    front: PartySlot | null;
  };
export type AttackDraft =
  | { step: "source"; targetIndex: number | null }
  | { step: "option"; source: AttackSource; targetIndex: number | null };

export const FIXED_ATTACK_CARD_COUNT = 3;

export const SKILLS = ["skill_1", "skill_2", "skill_3"] as const;
export const SKILL_LABELS: Record<string, string> = {
  skill_1: "技能 1",
  skill_2: "技能 2",
  skill_3: "技能 3",
};

export const COMMAND_SPELL_LABELS: Record<string, string> = {
  np_release: "宝具解放",
  restore: "灵基修复",
};

export const ATTACK_OPTIONS = [
  { value: "np", label: "宝具" },
  { value: "buster", label: "B" },
  { value: "arts", label: "A" },
  { value: "quick", label: "Q" },
  { value: "all", label: "ALL" },
] as const;

export const ENEMY_TARGETS = [
  { value: "enemy_1", label: "敌人 1", text: "1" },
  { value: "enemy_2", label: "敌人 2", text: "2" },
  { value: "enemy_3", label: "敌人 3", text: "3" },
  { value: "enemy_4", label: "敌人 4", text: "4" },
  { value: "enemy_5", label: "敌人 5", text: "5" },
  { value: "enemy_6", label: "敌人 6", text: "6" },
] as const;

export const CARD_LABELS: Record<string, string> = {
  np: "宝具",
  buster: "红卡攻击",
  arts: "蓝卡攻击",
  quick: "绿卡攻击",
  all: "任意指令卡",
};

export function createId(prefix: string): string {
  return `${prefix}_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
}

export function normalizeAttackPriority(priority: AttackCard[]): AttackCard[] {
  const next = [...priority];
  while (next.length < FIXED_ATTACK_CARD_COUNT) {
    next.push({
      id: createId(`atk_fixed_${next.length + 1}`),
      card: null,
    });
  }
  return next;
}

export function emptyLegacyFields(scene: BattleTurn): BattleTurn {
  return {
    ...scene,
    servantActions: [],
    equipmentActions: [],
    commandSpellActions: [],
  };
}

export function skillSlotIndex(skill: string | null | undefined): number {
  const match = skill?.match(/^skill_([1-3])$/);
  return match ? Number(match[1]) - 1 : -1;
}

export function servantSlotIndex(source: string | null | undefined): number | null {
  const match = source?.match(/^servant_([1-6])$/);
  return match ? Number(match[1]) - 1 : null;
}

export function sourceIndex(source: PrepSource | AttackSource): number | null {
  const index = servantSlotIndex(source);
  return index != null && index < 3 ? index : null;
}

export function orderChangeSummary(
  orderChange: OrderChangeSelection,
  partyServants: (Servant | null)[]
): string | null {
  const frontIndex = servantSlotIndex(orderChange.front);
  const backIndex = servantSlotIndex(orderChange.back);
  if (frontIndex == null || backIndex == null) return null;
  return `${servantLabel(frontIndex, partyServants[frontIndex] ?? null)} ↔ ${servantLabel(backIndex, partyServants[backIndex] ?? null)}`;
}
