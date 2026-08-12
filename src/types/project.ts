import type { ProjectRecognitionSettings } from "./recognition";

/**
 * One cell of the team-builder grid. Mirrors the Rust `ProjectSlot` struct.
 * For `type: "support"` slots, `servantId` is ignored on render — the
 * pinned support servant lives on `Project.supportServantId` to keep a
 * single source of truth shared with the runner.
 */
export interface ProjectSlot {
  id: string;
  type: "servant" | "support";
  servantId: number | null;
  servantVariantKey?: string | null;
  /**
   * Optional pinned craft-essence id. Persisted per-slot so each loadout
   * can carry its own equipment plan. The runner only consumes the
   * support slot's CE today (for support-row verification); party-slot
   * CEs are stored for future use.
   */
  craftEssenceId?: number | null;
  /**
   * Ordered support CE allow-list. Empty/missing falls back to
   * `craftEssenceId` for projects saved before multi-select existed.
   */
  craftEssenceIds?: number[];
  craftEssenceMultiSelect?: boolean;
  craftEssenceMlbRequired?: boolean;
}

export type BattleRepeatMode = "single" | "infinite" | "count";

export type SupportSkillLevelMins = [
  number | null,
  number | null,
  number | null,
];

export type SupportAppendSkillLevelMins = [
  number | null,
  number | null,
  number | null,
  number | null,
  number | null,
];

export type SupportGrandCraftEssenceIds = [
  number | null,
  number | null,
  number | null,
];

export type SupportGrandCraftEssenceIdLists = [number[], number[], number[]];

export type SupportGrandCraftEssenceMlbRequired = [boolean, boolean, boolean];
export type SupportGrandBondCeMode = "any" | "bond" | "bondNp";
export type GrandNpCard = "auto" | "buster" | "arts" | "quick";
export type GrandCardPriority = "damage" | "np";
export type GrandClass = string;
export type LancerGrandRole = "single" | "aoe";

export interface GrandRoleDefinition {
  role: string;
  label: string;
  required: boolean;
}

export interface GrandClassDefinition {
  id: GrandClass;
  label: string;
  servantClass: string;
  selectionGroup?: string;
  selectionGroupLabel?: string;
  selectionOptionLabel?: string;
  roles: GrandRoleDefinition[];
  cardPriorityEnabled: boolean;
  autoOrderChangeRoles: string[];
  validationMessage: string;
}
export type GrandChainPriorityItem =
  | "mainBraveChain"
  | "mainReadyNp"
  | "deputyBraveChain"
  | "mainColorChain"
  | "deputyColorChain"
  | "fallback";

export type GrandRuleKind = "any" | "command" | "np";
export type GrandRuleColor = "any" | "buster" | "arts" | "quick";

export interface GrandCardRuleSlotConfig {
  memberId?: string | null;
  slotIndex?: number | null;
  servantId: number | null;
  isSupport?: boolean;
  grandServant?: boolean;
  kind: GrandRuleKind;
  color: GrandRuleColor;
}

export interface GrandCardRuleConfig {
  id: string;
  name: string;
  slots: GrandCardRuleSlotConfig[];
}

export interface GrandCardStrategy {
  chainPriority?: GrandChainPriorityItem[];
  customRules?: GrandCardRuleConfig[];
}

export interface GrandServantConfig {
  memberId?: string | null;
  slotIndex: number;
  servantId?: number | null;
  isSupport?: boolean;
  npCard?: GrandNpCard;
  priority?: GrandCardPriority;
  role?: string | null;
  /** @deprecated Read-only compatibility for projects saved before generic roles. */
  lancerRole?: LancerGrandRole | null;
}

export type BattleApRecoveryItem =
  | "gold"
  | "silver"
  | "bronze"
  | "copper"
  | "rainbow";

export interface Project {
  id: string;
  name: string;
  advancedMode?: boolean;
  /**
   * Servant id pinned for the support-select screen. Mirrors the Rust
   * `Project::support_servant_id` field. `null`/`undefined` means the user
   * hasn't picked a support yet, in which case the runner falls back to
   * tapping the topmost row.
   */
  supportServantId?: number | null;
  supportServantVariantKey?: string | null;
  supportGrandMode?: boolean;
  supportGrandCraftEssenceIds?: SupportGrandCraftEssenceIds;
  supportGrandCraftEssenceIdLists?: SupportGrandCraftEssenceIdLists;
  supportGrandCraftEssenceMlbRequired?: SupportGrandCraftEssenceMlbRequired;
  supportGrandBondCeMode?: SupportGrandBondCeMode;
  grandClass?: GrandClass;
  grandServants?: GrandServantConfig[];
  grandCardStrategy?: GrandCardStrategy;
  supportNoblePhantasmLevelMin?: number | null;
  supportStarMapScoreMin?: number | null;
  supportGrandStarMapScoreMin?: number | null;
  supportSkillLevelMins?: SupportSkillLevelMins;
  supportAppendSkillLevelMins?: SupportAppendSkillLevelMins;
  recognitionSettings?: ProjectRecognitionSettings | null;
  disableAutoSkillTargetRecognition?: boolean;
  /**
   * Team-builder grid layout (5 servant slots + 1 support slot, in
   * arrangement order). Persisted on the backend so selections and
   * drag-and-drop layout survive across sessions and project switches.
   */
  slots: ProjectSlot[];
  /**
   * When `true`, the runner taps "Continue" on the post-battle continue
   * page so the same quest is queued again; when `false`, it taps
   * "Close" and the run terminates. Optional in the wire payload because
   * older `projects.json` rows don't carry it; the backend defaults to
   * `false`.
   */
  repeatMission?: boolean;
  repeatMode?: BattleRepeatMode | null;
  repeatCount?: number | null;
  apRecoveryItems?: BattleApRecoveryItem[];
}
