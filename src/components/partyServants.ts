import type { SlotItem } from "./ContentGrid";
import type { Project } from "../types/project";
import type { BattleScene, BattleTurn, PreparationAction } from "../types/command";
import type { Servant } from "../types/servant";
import changeOrderRulesJson from "../../src-tauri/src/resources/change_order_servants.json";

export interface PartyMember {
  servant: Servant | null;
  isSupport: boolean;
}

type ChangeOrderRule = {
  servantId: number;
  trigger:
    | { type: "attackCard"; card: "np"; activationUseCount?: number }
    | { type: "servantSkill"; skill: string };
  effect:
    | { type: "removeSelf" }
    | { type: "removeFirstAlly" }
    | { type: "withdrawSelfToBack" };
  timing?: "immediate" | "endOfTurn";
};

const CHANGE_ORDER_RULES = changeOrderRulesJson as ChangeOrderRule[];

export function toPartyMembers(lineup: (Servant | null)[]): PartyMember[] {
  return lineup.map((servant) => ({ servant, isSupport: false }));
}

export function partyMembersToServants(members: PartyMember[]): (Servant | null)[] {
  return members.map((member) => member.servant);
}

/**
 * Derive the full party from the team-builder slots.
 *
 * The display order of `slots` is authoritative, including the support
 * slot. The returned `PartyMember` keeps the support-slot identity with
 * that party entry so UI badges can distinguish a borrowed servant from
 * an owned copy of the same servant.
 */
export function derivePartyMembers(
  slots: SlotItem[],
  activeProject: Project | null,
  servants: Servant[]
): PartyMember[] {
  const supportPinned =
    activeProject?.supportServantId != null
      ? (servants.find((s) => s.variantKey === activeProject.supportServantVariantKey) ??
        servants.find((s) => s.id === activeProject.supportServantId) ??
        null)
      : null;
  return slots.map((slot) => ({
    servant: slot.type === "support" ? supportPinned : slot.servant,
    isSupport: slot.type === "support",
  }));
}

/**
 * Derive the front-line party (positions 1-3) from the team-builder slots.
 *
 * The display order of `slots` is authoritative — whichever cell sits in
 * the first three positions is part of the front-line, including the
 * support slot. When the support slot lands in positions 1-3 it
 * contributes the project's pinned support servant
 * (`Project.supportServantId`); the per-slot `servantId` for support is
 * always null and must not be read here.
 */
export function derivePartyServants(
  slots: SlotItem[],
  activeProject: Project | null,
  servants: Servant[]
): (Servant | null)[] {
  return derivePartyLineup(slots, activeProject, servants).slice(0, 3);
}

export function derivePartyLineup(
  slots: SlotItem[],
  activeProject: Project | null,
  servants: Servant[]
): (Servant | null)[] {
  return partyMembersToServants(derivePartyMembers(slots, activeProject, servants));
}

function parseServantPosition(value: string | null | undefined): number | null {
  const match = value?.match(/^servant_([1-6])(?:_|$)/);
  if (!match) return null;
  return Number(match[1]) - 1;
}

function compactBackline(members: PartyMember[]) {
  const backline = members
    .slice(3)
    .filter((member) => Boolean(member.servant));
  for (let i = 3; i < members.length; i += 1) {
    members[i] = backline[i - 3] ?? { servant: null, isSupport: false };
  }
}

function removeAt(members: PartyMember[], index: number) {
  if (index < 0 || index >= 3 || !members[index]?.servant) return;
  const replacementIndex = members.findIndex((member, i) => i >= 3 && member.servant);
  members[index] =
    replacementIndex === -1
      ? { servant: null, isSupport: false }
      : { ...members[replacementIndex] };
  if (replacementIndex !== -1) {
    members[replacementIndex] = { servant: null, isSupport: false };
  }
  compactBackline(members);
}

function withdrawToBack(members: PartyMember[], index: number) {
  if (index < 0 || index >= 3) return;
  const member = members[index];
  if (!member?.servant) return;
  compactBackline(members);
  const replacementIndex = members.findIndex((candidate, i) => i >= 3 && candidate.servant);
  if (replacementIndex === -1) return;
  members[index] = { ...members[replacementIndex] };
  members[replacementIndex] = { ...member };
}

function applyRule(
  members: PartyMember[],
  sourceIndex: number,
  rule: ChangeOrderRule
) {
  if (rule.effect.type === "removeSelf") {
    removeAt(members, sourceIndex);
    return;
  }

  if (rule.effect.type === "removeFirstAlly") {
    const targetIndex = [0, 1, 2].find((i) => i !== sourceIndex && members[i]?.servant);
    if (targetIndex != null) removeAt(members, targetIndex);
    return;
  }

  withdrawToBack(members, sourceIndex);
}

function applyPreparationAction(
  members: PartyMember[],
  action: PreparationAction,
  timing: ChangeOrderRule["timing"] = "immediate"
) {
  if (timing === "endOfTurn" && action.type !== "servant") return;

  if (action.type === "equipment" && action.orderChange) {
    const frontIndex = parseServantPosition(action.orderChange.front);
    const backIndex = parseServantPosition(action.orderChange.back);
    if (
      frontIndex != null &&
      backIndex != null &&
      frontIndex < 3 &&
      backIndex >= 3 &&
      members[frontIndex]?.servant &&
      members[backIndex]?.servant
    ) {
      [members[frontIndex], members[backIndex]] = [
        { ...members[backIndex] },
        { ...members[frontIndex] },
      ];
    }
    return;
  }

  if (action.type !== "servant") return;
  const sourceIndex = parseServantPosition(action.servant);
  if (sourceIndex == null || !action.skill) return;
  const servant = members[sourceIndex]?.servant;
  if (!servant) return;
  const rule = CHANGE_ORDER_RULES.find(
    (r) =>
      r.servantId === servant.id &&
      r.trigger.type === "servantSkill" &&
      r.trigger.skill === action.skill &&
      (r.timing ?? "immediate") === timing
  );
  if (rule) applyRule(members, sourceIndex, rule);
}

function applyAttackCard(
  members: PartyMember[],
  card: string | null | undefined,
  npUseCounts: Map<number, number>
) {
  const sourceIndex = parseServantPosition(card);
  if (sourceIndex == null || !card?.endsWith("_np")) return;
  const servant = members[sourceIndex]?.servant;
  if (!servant) return;
  const nextCount = (npUseCounts.get(servant.id) ?? 0) + 1;
  npUseCounts.set(servant.id, nextCount);
  const rule = CHANGE_ORDER_RULES.find(
    (r) =>
      r.servantId === servant.id &&
      r.trigger.type === "attackCard" &&
      r.trigger.card === "np" &&
      (r.timing ?? "immediate") === "immediate" &&
      (r.trigger.activationUseCount == null ||
        r.trigger.activationUseCount === nextCount)
  );
  if (rule) applyRule(members, sourceIndex, rule);
}

export function deriveMembersAfterPreparationActions(
  initialMembers: PartyMember[],
  actions: PreparationAction[]
): PartyMember[] {
  const members = initialMembers.map((member) => ({ ...member }));
  for (const action of actions) {
    applyPreparationAction(members, action);
  }
  return members;
}

export function deriveMembersAfterAttackCards(
  initialMembers: PartyMember[],
  cards: ReadonlyArray<{ card: string | null | undefined }>
): PartyMember[] {
  const members = initialMembers.map((member) => ({ ...member }));
  const npUseCounts = new Map<number, number>();
  for (const card of cards) {
    applyAttackCard(members, card.card, npUseCounts);
  }
  return members;
}

export function deriveLineupAfterPreparationActions(
  initialLineup: (Servant | null)[],
  actions: PreparationAction[]
): (Servant | null)[] {
  return partyMembersToServants(
    deriveMembersAfterPreparationActions(toPartyMembers(initialLineup), actions)
  );
}

export function deriveLineupAfterAttackCards(
  initialLineup: (Servant | null)[],
  cards: ReadonlyArray<{ card: string | null | undefined }>
): (Servant | null)[] {
  return partyMembersToServants(
    deriveMembersAfterAttackCards(toPartyMembers(initialLineup), cards)
  );
}

export function deriveScenePartyServants(
  initialLineup: (Servant | null)[],
  scenes: BattleScene[]
): (Servant | null)[][] {
  return deriveScenePartyLineups(initialLineup, scenes).map((lineup) =>
    lineup.slice(0, 3)
  );
}

export function deriveScenePartyLineups(
  initialLineup: (Servant | null)[],
  scenes: BattleScene[]
): (Servant | null)[][] {
  return deriveScenePartyMembers(toPartyMembers(initialLineup), scenes).map(
    partyMembersToServants
  );
}

export function deriveScenePartyMembers(
  initialMembers: PartyMember[],
  scenes: BattleScene[]
): PartyMember[][] {
  const members = initialMembers.map((member) => ({ ...member }));
  const sceneMembers: PartyMember[][] = [];
  const npUseCounts = new Map<number, number>();

  for (const scene of scenes) {
    sceneMembers.push(members.map((member) => ({ ...member })));

    for (const turn of sceneTurns(scene)) {
      applyTurnLineupChanges(members, turn, npUseCounts);
    }
  }

  return sceneMembers;
}

export function deriveTurnPartyMembers(
  initialMembers: PartyMember[],
  scenes: BattleScene[],
  targetSceneIndex: number,
  targetTurnIndex: number
): PartyMember[] {
  const members = initialMembers.map((member) => ({ ...member }));
  const npUseCounts = new Map<number, number>();

  for (const [sceneIndex, scene] of scenes.entries()) {
    if (sceneIndex > targetSceneIndex) break;
    const turns = sceneTurns(scene);
    for (const [turnIndex, turn] of turns.entries()) {
      if (sceneIndex === targetSceneIndex && turnIndex >= targetTurnIndex) {
        return members.map((member) => ({ ...member }));
      }
      applyTurnLineupChanges(members, turn, npUseCounts);
    }
  }

  return members.map((member) => ({ ...member }));
}

function sceneTurns(scene: BattleScene): BattleTurn[] {
  if (scene.turns?.length) return scene.turns;
  return [
    {
      id: `${scene.id}_legacy_turn_1`,
      preparationActions: scene.preparationActions ?? [],
      servantActions: scene.servantActions ?? [],
      equipmentActions: scene.equipmentActions ?? [],
      commandSpellActions: scene.commandSpellActions ?? [],
      enemyTarget: scene.enemyTarget ?? null,
      attackPriority: scene.attackPriority ?? [],
    },
  ];
}

function applyTurnLineupChanges(
  members: PartyMember[],
  turn: BattleTurn,
  npUseCounts: Map<number, number>
) {
  for (const action of turn.preparationActions ?? turn.servantActions) {
    applyPreparationAction(members, action, "immediate");
  }

  for (const card of turn.attackPriority) {
    applyAttackCard(members, card.card, npUseCounts);
  }

  for (const action of turn.preparationActions ?? turn.servantActions) {
    applyPreparationAction(members, action, "endOfTurn");
  }
}
