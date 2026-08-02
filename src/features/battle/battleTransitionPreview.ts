import type {
  AdvancedBattleScene,
  BattleScene,
  BattleStateOverride,
  PreparationAction,
} from "../../types/command";
import type { BattleTransitionPreviewEvent } from "../../types/battleTransition";
import type { PartyMember } from "../team/partyServants";
import { skillSlotIndex } from "./battleSceneModel";

function memberMatches(
  member: PartyMember,
  memberId: string | null | undefined,
  servantId: number | null | undefined,
  isSupport = false
): boolean {
  if (memberId && member.memberId) return memberId === member.memberId;
  return Boolean(member.servant) && member.servant?.id === servantId && member.isSupport === isSupport;
}

function slotIndex(value: string | null | undefined): number | null {
  const match = value?.match(/^servant_([1-6])(?:_|$)/);
  return match ? Number(match[1]) - 1 : null;
}

function overrideEvents(
  member: PartyMember,
  overrides: BattleStateOverride[] | undefined
): BattleTransitionPreviewEvent[] {
  return (overrides ?? [])
    .filter((override) =>
      memberMatches(member, override.memberId, override.servantId, override.isSupport)
    )
    .map((override) => ({
      type: "override" as const,
      stateKey: override.stateKey,
      mode: override.mode,
      remainingTurns: override.remainingTurns,
      stacks: override.stacks,
    }));
}

function actionEvents(
  member: PartyMember,
  memberIndex: number,
  actions: PreparationAction[]
): BattleTransitionPreviewEvent[] {
  return actions.flatMap((action) => {
    if (
      action.type !== "servant" ||
      !(action.servantMemberId || action.servantId != null
        ? memberMatches(
            member,
            action.servantMemberId,
            action.servantId,
            action.servantIsSupport
          )
        : slotIndex(action.servant) === memberIndex)
    ) {
      return [];
    }
    const slot = skillSlotIndex(action.skill);
    return slot < 0
      ? []
      : [{
          type: "skill" as const,
          slot: slot + 1,
          selectionIndex: action.skillSelection?.index,
        }];
  });
}

/** Builds the conservative state at the start of the selected normal turn. */
export function buildNormalBattleTransitionEvents(
  member: PartyMember,
  memberIndex: number,
  scenes: BattleScene[],
  activeSceneIndex: number,
  activeTurnIndex: number
): BattleTransitionPreviewEvent[] {
  const scene = scenes[activeSceneIndex];
  if (!scene) return [];
  const events: BattleTransitionPreviewEvent[] = [];
  for (let index = 0; index <= activeTurnIndex && index < scene.turns.length; index += 1) {
    const turn = scene.turns[index];
    events.push(...overrideEvents(member, turn.battleStateOverrides));
    if (index === activeTurnIndex) break;
    const actions =
      turn.preparationActions ?? [
        ...(turn.servantActions ?? []),
        ...(turn.equipmentActions ?? []),
        ...(turn.commandSpellActions ?? []),
      ];
    events.push(...actionEvents(member, memberIndex, actions));
    for (const card of turn.attackPriority ?? []) {
      if (!card.card?.endsWith("_np")) continue;
      if (
        card.memberId || card.servantId != null
          ? memberMatches(member, card.memberId, card.servantId, card.isSupport)
          : slotIndex(card.card) === memberIndex
      ) {
        events.push({ type: "noblePhantasm" });
      }
    }
    events.push({ type: "turnEnd" });
  }
  return events;
}

/** Builds the conservative state at the start of the selected advanced turn. */
export function buildAdvancedBattleTransitionEvents(
  member: PartyMember,
  memberIndex: number,
  scene: AdvancedBattleScene,
  activeTurnIndex: number
): BattleTransitionPreviewEvent[] {
  const turns = scene.turns ?? [];
  const events: BattleTransitionPreviewEvent[] = [];
  events.push(...actionEvents(member, memberIndex, scene.controlActions ?? []));
  for (let index = 0; index <= activeTurnIndex && index < turns.length; index += 1) {
    const turn = turns[index];
    events.push(...overrideEvents(member, turn.battleStateOverrides));
    if (index === activeTurnIndex) break;
    events.push(...actionEvents(member, memberIndex, turn.actions));
    const output = scene.mainOutput;
    if (output?.outputType === "np") {
      if (
        output.memberId || output.servantId != null
          ? memberMatches(member, output.memberId, output.servantId, output.isSupport)
          : slotIndex(output.servant) === memberIndex
      ) {
        events.push({ type: "noblePhantasm" });
      } else if (!output.memberId && output.servantId == null && output.servant == null) {
        events.push({ type: "uncertain" });
      }
    }
    events.push({ type: "turnEnd" });
  }
  return events;
}
