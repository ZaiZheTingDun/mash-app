import type { CommandSpellAction, EquipmentAction, PreparationAction } from "../../types/command";
import { mysticCodeSkill, type MysticCode } from "../../types/mysticCode";
import type { Servant } from "../../types/servant";
import { memberRefAt, type PartyMember } from "../team/partyServants";
import { createId, servantSlotIndex, type PartySlot, type PrepDraft, type PrepSource } from "./battleSceneModel";
import type { ServantSkillSelectionEntry } from "./useServantSkillSelections";
import type { SkillTargetStatus } from "./useServantSkillTargeting";

export type TargetDraft = Extract<PrepDraft, { step: "target" }>;
type SelectionDraft = Extract<PrepDraft, { step: "skillSelection" }>;
type OrderChangeDraft = Extract<PrepDraft, { step: "orderChange" }>;
export type PreparationResult =
  | { type: "action"; action: PreparationAction }
  | { type: "draft"; draft: TargetDraft | SelectionDraft | OrderChangeDraft };

export interface PreparationContext {
  members: PartyMember[];
  mysticCode: MysticCode | null;
  disableAutoSkillTargetRecognition: boolean;
  skillTargetStatus: (servant: Servant | null, skill: string) => SkillTargetStatus;
  skillSelection: (servant: Servant | null, skill: string) => ServantSkillSelectionEntry | null;
}

function targetRef(members: PartyMember[], target: string | null) {
  const ref = memberRefAt(members, target);
  return { targetMemberId: ref.memberId, targetServantId: ref.servantId, targetIsSupport: ref.isSupport };
}

/** Shared by guided entry, quick entry, and advanced control actions. */
export function makePreparationAction(draft: TargetDraft, target: string | null, members: PartyMember[]): PreparationAction {
  if (draft.source === "enemyTarget") return { type: "enemyTarget", id: createId("enemy_target"), target };
  if (draft.source === "equipment") return {
    type: "equipment", id: createId("eq"), skill: draft.option, target,
    ...targetRef(members, target), orderChange: null,
  };
  if (draft.source === "commandSpell") return {
    type: "commandSpell", id: createId("cs"), spell: draft.option as CommandSpellAction["spell"],
    target, ...targetRef(members, target),
  };
  const source = memberRefAt(members, draft.source);
  return {
    type: "servant", id: createId("sa"), servant: draft.source,
    servantMemberId: source.memberId, servantId: source.servantId, servantIsSupport: source.isSupport,
    skill: draft.option, skillSelection: draft.skillSelection ?? null, target, ...targetRef(members, target),
    ...(draft.skillFormId != null ? { skillFormId: draft.skillFormId } : {}),
  };
}

export function makeOrderChangeAction(draft: OrderChangeDraft, back: PartySlot, members: PartyMember[]): EquipmentAction | null {
  if (draft.front == null) return null;
  const frontRef = memberRefAt(members, draft.front);
  const backRef = memberRefAt(members, back);
  return {
    type: "equipment", id: createId("eq"), skill: draft.option, target: null,
    orderChange: {
      front: draft.front, frontMemberId: frontRef.memberId, frontServantId: frontRef.servantId, frontIsSupport: frontRef.isSupport,
      back, backMemberId: backRef.memberId, backServantId: backRef.servantId, backIsSupport: backRef.isSupport,
    },
  };
}

function targetingMode(context: PreparationContext, source: PrepSource, skill: string) {
  if (context.disableAutoSkillTargetRecognition) return "unknown";
  if (source === "equipment") return mysticCodeSkill(context.mysticCode, skill)?.targetingMode ?? "unknown";
  const servant = context.members[servantSlotIndex(source) ?? 0]?.servant ?? null;
  return context.skillTargetStatus(servant, skill);
}

/** An omitted target keeps guided entry; null means quick entry without a selected target. */
function resolveTarget(context: PreparationContext, draft: TargetDraft, target?: PartySlot | null): PreparationResult {
  const status = draft.source === "commandSpell" ? "needsTarget" : targetingMode(context, draft.source, draft.option);
  if (status === "noTarget" || (target !== undefined && (target !== null || status !== "needsTarget"))) {
    return { type: "action", action: makePreparationAction(draft, status === "noTarget" ? null : target ?? null, context.members) };
  }
  return {
    type: "draft",
    draft: status === "needsTarget" ? { ...draft, allowNoTarget: false }
      : status === "mixed" ? { ...draft, allowNoTarget: true } : draft,
  };
}

export function startPreparationSkill(context: PreparationContext, source: PrepSource, skill: string, target?: PartySlot | null): PreparationResult {
  if (source === "equipment" && targetingMode(context, source, skill) === "orderChange") {
    return { type: "draft", draft: { step: "orderChange", source, option: skill, front: null } };
  }
  if (source !== "equipment" && source !== "commandSpell" && source !== "enemyTarget") {
    const servant = context.members[servantSlotIndex(source) ?? 0]?.servant ?? null;
    const selection = context.skillSelection(servant, skill);
    if (selection) return {
      type: "draft", draft: { step: "skillSelection", source, option: skill, selectionType: selection.selectionType, options: selection.options },
    };
  }
  return resolveTarget(context, { step: "target", source, option: skill }, target);
}

export function selectPreparationSkillOption(context: PreparationContext, draft: SelectionDraft, option: { index: number; label: string }, target?: PartySlot | null): PreparationResult {
  return resolveTarget(context, {
    step: "target", source: draft.source, option: draft.option,
    skillSelection: { type: draft.selectionType, index: option.index, optionCount: draft.options.length, label: option.label },
  }, target);
}
