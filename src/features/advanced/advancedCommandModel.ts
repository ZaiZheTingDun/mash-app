import { servantLabel } from "../../components/common/battleActorLabels";
import type {
  AdvancedBattleScene,
  AdvancedCommandCardCondition,
  PreparationAction,
  SkillSelection,
  SkillSelectionType,
} from "../../types/command";
import type {
  GrandCardPriority,
  GrandCardRuleConfig,
  GrandCardRuleSlotConfig,
  GrandCardStrategy,
  GrandChainPriorityItem,
  GrandNpCard,
  GrandRuleColor,
  GrandRuleKind,
  GrandServantConfig,
} from "../../types/project";
import type { Servant } from "../../types/servant";
import commandBgArts from "../../../src-tauri/resources/images/command_bg/command_bg_a.png";
import commandBgBuster from "../../../src-tauri/resources/images/command_bg/command_bg_b.png";
import commandBgQuick from "../../../src-tauri/resources/images/command_bg/command_bg_q.png";

export type PartySlot = `servant_${1 | 2 | 3 | 4 | 5 | 6}`;
export type FrontServant = Extract<PartySlot, "servant_1" | "servant_2" | "servant_3">;
export type PrepSource = "equipment" | "commandSpell" | PartySlot;
export type PrepDraft =
  | { step: "source" }
  | { step: "option"; source: PrepSource }
  | {
    step: "skillSelection";
    source: PrepSource;
    option: string;
    selectionType: SkillSelectionType;
    options: { index: number; label: string }[];
  }
  | {
    step: "target";
    source: PrepSource;
    option: string;
    allowNoTarget?: boolean;
    skillSelection?: SkillSelection | null;
  }
  | { step: "orderChange"; source: "equipment"; option: string; front: PartySlot | null };

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
export const EMPTY_STARTUP_ACTIONS: PreparationAction[] = [];
export const COMMAND_BG_BY_SUIT: Record<Exclude<AdvancedCommandCardCondition["suit"], "any">, string> = {
  arts: commandBgArts,
  buster: commandBgBuster,
  quick: commandBgQuick,
};
export const COMMAND_BG_BY_RULE_COLOR: Record<Exclude<GrandRuleColor, "any">, string> = {
  arts: commandBgArts,
  buster: commandBgBuster,
  quick: commandBgQuick,
};
export const DEFAULT_GRAND_CHAIN_PRIORITY: GrandChainPriorityItem[] = [
  "mainBraveChain",
  "mainReadyNp",
  "deputyBraveChain",
  "mainColorChain",
  "deputyColorChain",
  "fallback",
];
export const RULE_KIND_LABELS: Record<GrandRuleKind, string> = {
  any: "任意",
  command: "指令卡",
  np: "宝具",
};
export const RULE_COLOR_LABELS: Record<GrandRuleColor, string> = {
  any: "任意",
  buster: "红",
  arts: "蓝",
  quick: "绿",
};
export const RULE_KIND_DIALOG_LABELS: Record<GrandRuleKind, string> = {
  any: "任意",
  command: "指令卡",
  np: "宝具",
};
export const RULE_KIND_DIALOG_DESCRIPTIONS: Record<GrandRuleKind, string> = {
  any: "自动识别任意出牌卡",
  command: "仅识别普通指令卡",
  np: "仅识别宝具卡",
};
export const RULE_COLOR_DIALOG_LABELS: Record<GrandRuleColor, string> = {
  any: "任意",
  buster: "红卡",
  arts: "蓝卡",
  quick: "绿卡",
};
export const RULE_COLOR_DIALOG_DESCRIPTIONS: Record<GrandRuleColor, string> = {
  any: "Any",
  buster: "Buster",
  arts: "Arts",
  quick: "Quick",
};
export const RULE_KIND_OPTIONS: GrandRuleKind[] = ["any", "command", "np"];
export const RULE_COLOR_OPTIONS: GrandRuleColor[] = ["any", "buster", "arts", "quick"];

let nextAdvancedSceneId = 1;

export function createId(prefix: string): string {
  return `${prefix}_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
}

export function createDefaultScene(): AdvancedBattleScene {
  return {
    id: `advanced_scene_${nextAdvancedSceneId++}_${Date.now()}`,
    mainOutput: { servant: null, outputType: null, npCard: "auto" },
    grandAutoOrderChange: null,
    commandConditions: [0, 1, 2, 3, 4].map(defaultCommandCard),
    controlActions: [],
    startupActions: [],
    rules: [],
  };
}

export function normalizeScene(scene: AdvancedBattleScene): AdvancedBattleScene {
  return {
    ...scene,
    mainOutput: scene.mainOutput
      ? { npCard: "auto", ...scene.mainOutput }
      : { servant: null, outputType: null, npCard: "auto" },
    grandAutoOrderChange: scene.grandAutoOrderChange ?? null,
    commandConditions:
      scene.commandConditions && scene.commandConditions.length === 5
        ? scene.commandConditions.map((card) => ({ ...card, minCritChance: null }))
        : [0, 1, 2, 3, 4].map(defaultCommandCard),
    controlActions: scene.controlActions ?? [],
    startupActions: scene.startupActions ?? [],
    rules: [],
  };
}

export function defaultCommandCard(slot: number): AdvancedCommandCardCondition {
  return {
    slot,
    servant: "any",
    suit: "any",
    minCritChance: null,
  };
}

export function servantSlotIndex(source: string | null | undefined): number | null {
  const match = source?.match(/^servant_([1-6])$/);
  return match ? Number(match[1]) - 1 : null;
}

export function mainGrandBackSlot(grandServants: GrandServantConfig[]): number | null {
  const slotIndex = grandServants[0]?.slotIndex;
  return Number.isInteger(slotIndex) && slotIndex >= 3 && slotIndex < 6 ? slotIndex : null;
}

export function normalizeGrandServants(values: GrandServantConfig[] | undefined): GrandServantConfig[] {
  const seen = new Set<number>();
  return (values ?? [])
    .filter((item) => Number.isInteger(item.slotIndex) && item.slotIndex >= 0 && item.slotIndex < 6)
    .filter((item) => {
      if (seen.has(item.slotIndex)) return false;
      seen.add(item.slotIndex);
      return true;
    })
    .slice(0, 2)
    .map((item) => ({
      memberId: item.memberId ?? null,
      slotIndex: item.slotIndex,
      servantId: item.servantId ?? null,
      isSupport: item.isSupport === true,
      npCard: item.npCard ?? "auto",
      priority: item.priority ?? "damage",
    }));
}

export function normalizeGrandCardStrategy(strategy: GrandCardStrategy | undefined): GrandCardStrategy {
  const configured = strategy?.chainPriority ?? [];
  const priority = configured.filter(
    (item, index): item is GrandChainPriorityItem =>
      DEFAULT_GRAND_CHAIN_PRIORITY.includes(item as GrandChainPriorityItem) &&
      configured.indexOf(item) === index
  );
  for (const item of DEFAULT_GRAND_CHAIN_PRIORITY) {
    if (!priority.includes(item)) {
      priority.push(item);
    }
  }
  return {
    chainPriority: priority,
    customRules: normalizeCustomRules(strategy?.customRules),
  };
}

export function defaultRuleSlot(): GrandCardRuleSlotConfig {
  return {
    slotIndex: null,
    servantId: null,
    isSupport: false,
    grandServant: false,
    kind: "any",
    color: "any",
  };
}

export function createDefaultCustomRule(): GrandCardRuleConfig {
  return {
    id: createId("grand_rule"),
    name: "自定义规则",
    slots: [defaultRuleSlot(), defaultRuleSlot(), defaultRuleSlot()],
  };
}

export function normalizeRuleSlot(slot: Partial<GrandCardRuleSlotConfig> | undefined): GrandCardRuleSlotConfig {
  const kinds: GrandRuleKind[] = ["any", "command", "np"];
  const colors: GrandRuleColor[] = ["any", "buster", "arts", "quick"];
  const kind = slot?.kind;
  const color = slot?.color;
  return {
    memberId: slot?.memberId ?? null,
    slotIndex:
      typeof slot?.slotIndex === "number" && Number.isInteger(slot.slotIndex)
        ? slot.slotIndex
        : null,
    servantId: typeof slot?.servantId === "number" ? slot.servantId : null,
    isSupport: slot?.isSupport === true,
    grandServant: slot?.grandServant === true,
    kind: kinds.includes(kind as GrandRuleKind) ? (kind as GrandRuleKind) : "any",
    color: colors.includes(color as GrandRuleColor) ? (color as GrandRuleColor) : "any",
  };
}

export function normalizeCustomRule(rule: Partial<GrandCardRuleConfig> | undefined, index: number): GrandCardRuleConfig {
  const slots = [0, 1, 2].map((slotIndex) => normalizeRuleSlot(rule?.slots?.[slotIndex]));
  return {
    id: rule?.id || createId("grand_rule"),
    name: rule?.name ?? `自定义规则 ${index + 1}`,
    slots,
  };
}

export function normalizeCustomRules(rules: GrandCardRuleConfig[] | undefined): GrandCardRuleConfig[] {
  return (rules ?? []).map((rule, index) => normalizeCustomRule(rule, index));
}

export function cardColorLabel(card: Servant["noblePhantasmCard"] | undefined): string | null {
  switch (card) {
    case "buster":
      return "红";
    case "arts":
      return "蓝";
    case "quick":
      return "绿";
    default:
      return null;
  }
}

export function npCardLabel(card: GrandNpCard | undefined, inferredCard?: Servant["noblePhantasmCard"]): string {
  switch (card) {
    case "buster":
      return "红";
    case "arts":
      return "蓝";
    case "quick":
      return "绿";
    default:
      return cardColorLabel(inferredCard) ? `自动${cardColorLabel(inferredCard)}` : "自动";
  }
}

export function autoNpOptionLabel(servant: Servant | null): string {
  const label = cardColorLabel(servant?.noblePhantasmCard);
  return label ? `自动读取（${label}）` : "自动读取";
}

export function priorityLabel(priority: GrandCardPriority | undefined): string {
  return priority === "np" ? "NP" : "伤害";
}

export function prepSummary(action: PreparationAction, partyLineup: (Servant | null)[]): string {
  if (action.type === "commandSpell") {
    const target = servantSlotIndex(action.target);
    const suffix = target == null ? "" : ` to ${servantLabel(target, partyLineup[target] ?? null)}`;
    return `令咒 ${COMMAND_SPELL_LABELS[action.spell ?? ""] ?? "行动"}${suffix}`;
  }
  if (action.type === "equipment") {
    if (action.orderChange) {
      const front = servantSlotIndex(action.orderChange.front);
      const back = servantSlotIndex(action.orderChange.back);
      const swap =
        front == null || back == null
          ? ""
          : ` ${servantLabel(front, partyLineup[front] ?? null)} ↔ ${servantLabel(back, partyLineup[back] ?? null)}`;
      return `御主礼装 ${SKILL_LABELS[action.skill ?? ""] ?? "技能"} Order Change${swap}`;
    }
    const target = servantSlotIndex(action.target);
    const suffix = target == null ? "" : ` to ${servantLabel(target, partyLineup[target] ?? null)}`;
    return `御主礼装 ${SKILL_LABELS[action.skill ?? ""] ?? "技能"}${suffix}`;
  }
  const source = servantSlotIndex(action.servant) ?? 0;
  const target = servantSlotIndex(action.target);
  const suffix = target == null ? "" : ` to ${servantLabel(target, partyLineup[target] ?? null)}`;
  const selection = action.skillSelection
    ? `并选择 ${action.skillSelection.label ?? `选项 ${action.skillSelection.index + 1}`}`
    : "";
  return `${servantLabel(source, partyLineup[source] ?? null)} ${SKILL_LABELS[action.skill ?? ""] ?? "技能"}${selection}${suffix}`;
}

export function commandCardSummary(card: AdvancedCommandCardCondition): string {
  const servant = card.servant === "any" ? "ANY" : `S${card.servant.slice(-1)}`;
  const suit = card.suit === "any" ? "ANY" : card.suit[0].toUpperCase();
  return `${servant}${suit}`;
}

export function commandCardAria(card: AdvancedCommandCardCondition): string {
  return `设置指令卡 ${card.slot + 1}，${commandCardSummary(card)}`;
}
