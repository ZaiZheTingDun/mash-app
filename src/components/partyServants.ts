import type { SlotItem } from "./ContentGrid";
import type { Project } from "../types/project";
import type { BattleScene } from "../types/command";
import type { Servant } from "../types/servant";
import changeOrderRulesJson from "../../src-tauri/src/resources/change_order_servants.json";

type ChangeOrderRule = {
  servantId: number;
  trigger:
    | { type: "attackCard"; card: "np"; activationUseCount?: number }
    | { type: "servantSkill"; skill: string };
  effect:
    | { type: "removeSelf" }
    | { type: "removeFirstAlly" }
    | { type: "withdrawSelfToBack" };
};

const CHANGE_ORDER_RULES = changeOrderRulesJson as ChangeOrderRule[];

/**
 * Derive the front-line party (positions 1-3) from the team-builder slots.
 *
 * The display order of `slots` is authoritative — whichever cell sits in
 * the first three positions is part of the front-line, including the
 * support slot. When the support slot lands in positions 1-3 it
 * contributes the project's pinned support servant
 * (`Project.supportServantId`); the per-slot `servantId` for support is
 * always null and must not be read here.
 *
 * The previous implementation filtered the support slot out before
 * slicing, which silently shifted later slots forward and made the
 * command editor label position 3 with the 4th servant whenever the user
 * had dragged support to position 3.
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
  const supportPinned =
    activeProject?.supportServantId != null
      ? (servants.find((s) => s.variantKey === activeProject.supportServantVariantKey) ??
        servants.find((s) => s.id === activeProject.supportServantId) ??
        null)
      : null;
  return slots.map((s) => (s.type === "support" ? supportPinned : s.servant));
}

function parseServantPosition(value: string | null | undefined): number | null {
  const match = value?.match(/^servant_([1-3])(?:_|$)/);
  if (!match) return null;
  return Number(match[1]) - 1;
}

function removeAt(lineup: (Servant | null)[], index: number) {
  if (index < 0 || index >= 3 || !lineup[index]) return;
  const replacementIndex = lineup.findIndex((servant, i) => i >= 3 && servant);
  lineup[index] = replacementIndex === -1 ? null : lineup[replacementIndex];
  if (replacementIndex !== -1) lineup[replacementIndex] = null;
}

function withdrawToBack(lineup: (Servant | null)[], index: number) {
  if (index < 0 || index >= 3) return;
  const servant = lineup[index];
  if (!servant) return;
  const replacementIndex = lineup.findIndex((candidate, i) => i >= 3 && candidate);
  lineup[index] = replacementIndex === -1 ? null : lineup[replacementIndex];
  if (replacementIndex !== -1) lineup[replacementIndex] = servant;
}

function applyRule(
  lineup: (Servant | null)[],
  sourceIndex: number,
  rule: ChangeOrderRule
) {
  if (rule.effect.type === "removeSelf") {
    removeAt(lineup, sourceIndex);
    return;
  }

  if (rule.effect.type === "removeFirstAlly") {
    const targetIndex = [0, 1, 2].find((i) => i !== sourceIndex && lineup[i]);
    if (targetIndex != null) removeAt(lineup, targetIndex);
    return;
  }

  withdrawToBack(lineup, sourceIndex);
}

export function deriveScenePartyServants(
  initialLineup: (Servant | null)[],
  scenes: BattleScene[]
): (Servant | null)[][] {
  const lineup = [...initialLineup];
  const sceneParties: (Servant | null)[][] = [];
  const npUseCounts = new Map<number, number>();

  for (const scene of scenes) {
    sceneParties.push(lineup.slice(0, 3));

    for (const action of scene.preparationActions ?? scene.servantActions) {
      if (action.type !== "servant") continue;
      const sourceIndex = parseServantPosition(action.servant);
      if (sourceIndex == null || !action.skill) continue;
      const servant = lineup[sourceIndex];
      if (!servant) continue;
      const rule = CHANGE_ORDER_RULES.find(
        (r) =>
          r.servantId === servant.id &&
          r.trigger.type === "servantSkill" &&
          r.trigger.skill === action.skill
      );
      if (rule) applyRule(lineup, sourceIndex, rule);
    }

    for (const card of scene.attackPriority) {
      const sourceIndex = parseServantPosition(card.card);
      if (sourceIndex == null || !card.card?.endsWith("_np")) continue;
      const servant = lineup[sourceIndex];
      if (!servant) continue;
      const nextCount = (npUseCounts.get(servant.id) ?? 0) + 1;
      npUseCounts.set(servant.id, nextCount);
      const rule = CHANGE_ORDER_RULES.find(
        (r) =>
          r.servantId === servant.id &&
          r.trigger.type === "attackCard" &&
          r.trigger.card === "np" &&
          (r.trigger.activationUseCount == null ||
            r.trigger.activationUseCount === nextCount)
      );
      if (rule) applyRule(lineup, sourceIndex, rule);
    }
  }

  return sceneParties;
}
