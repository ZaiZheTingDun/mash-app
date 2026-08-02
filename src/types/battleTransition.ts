import type { SkillSelectionType } from "./command";

export type BattleTransitionPreviewEvent =
  | { type: "skill"; slot: number; selectionIndex?: number | null }
  | { type: "noblePhantasm" }
  | { type: "turnEnd" }
  | {
      type: "override";
      stateKey: string;
      mode: "set" | "clear";
      remainingTurns?: number | null;
      stacks?: number | null;
    }
  | { type: "uncertain" };

export interface ResolvedSkillForm {
  id: number;
  slot: number;
  name: string;
  icon: string | null;
  targetTypes: string[];
  selection?: {
    type: SkillSelectionType;
    options: { index: number; label: string }[];
  } | null;
}

export interface ResolvedNoblePhantasmForm {
  id: number;
  name: string;
  card: "buster" | "arts" | "quick" | null;
}

export interface AvailableBattleCondition {
  stateKey: string;
  name: string;
  durationTurns: number | null;
  maxStacks: number | null;
  external: boolean;
}

export interface ActiveBattleCondition {
  stateKey: string;
  name: string;
  remainingTurns: number | null;
  stacks: number;
  external: boolean;
}

export interface ResolvedBattleMetadata {
  skills: [ResolvedSkillForm | null, ResolvedSkillForm | null, ResolvedSkillForm | null];
  noblePhantasm: ResolvedNoblePhantasmForm | null;
  availableConditions: AvailableBattleCondition[];
  activeConditions: ActiveBattleCondition[];
  cooldownAdjustments: Record<string, number>;
  previewUncertain: boolean;
}
