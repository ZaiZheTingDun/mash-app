import type { SlotItem } from "./ContentGrid";
import type { Project } from "../types/project";
import type { BattleScene, PreparationAction } from "../types/command";
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
  timing?: "immediate" | "endOfTurn";
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
  const match = value?.match(/^servant_([1-6])(?:_|$)/);
  if (!match) return null;
  return Number(match[1]) - 1;
}

function compactBackline(lineup: (Servant | null)[]) {
  const backline = lineup.slice(3).filter((servant): servant is Servant =>
    Boolean(servant)
  );
  for (let i = 3; i < lineup.length; i += 1) {
    lineup[i] = backline[i - 3] ?? null;
  }
}

function removeAt(lineup: (Servant | null)[], index: number) {
  if (index < 0 || index >= 3 || !lineup[index]) return;
  const replacementIndex = lineup.findIndex((servant, i) => i >= 3 && servant);
  lineup[index] = replacementIndex === -1 ? null : lineup[replacementIndex];
  if (replacementIndex !== -1) lineup[replacementIndex] = null;
  compactBackline(lineup);
}

function withdrawToBack(lineup: (Servant | null)[], index: number) {
  if (index < 0 || index >= 3) return;
  const servant = lineup[index];
  if (!servant) return;
  compactBackline(lineup);
  const replacementIndex = lineup.findIndex((candidate, i) => i >= 3 && candidate);
  if (replacementIndex === -1) return;
  lineup[index] = lineup[replacementIndex];
  lineup[replacementIndex] = servant;
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

function applyPreparationAction(
  lineup: (Servant | null)[],
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
      lineup[frontIndex] &&
      lineup[backIndex]
    ) {
      [lineup[frontIndex], lineup[backIndex]] = [
        lineup[backIndex] ?? null,
        lineup[frontIndex] ?? null,
      ];
    }
    return;
  }

  if (action.type !== "servant") return;
  const sourceIndex = parseServantPosition(action.servant);
  if (sourceIndex == null || !action.skill) return;
  const servant = lineup[sourceIndex];
  if (!servant) return;
  const rule = CHANGE_ORDER_RULES.find(
    (r) =>
      r.servantId === servant.id &&
      r.trigger.type === "servantSkill" &&
      r.trigger.skill === action.skill &&
      (r.timing ?? "immediate") === timing
  );
  if (rule) applyRule(lineup, sourceIndex, rule);
}

function applyAttackCard(
  lineup: (Servant | null)[],
  card: string | null | undefined,
  npUseCounts: Map<number, number>
) {
  const sourceIndex = parseServantPosition(card);
  if (sourceIndex == null || !card?.endsWith("_np")) return;
  const servant = lineup[sourceIndex];
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
  if (rule) applyRule(lineup, sourceIndex, rule);
}

export function deriveLineupAfterPreparationActions(
  initialLineup: (Servant | null)[],
  actions: PreparationAction[]
): (Servant | null)[] {
  const lineup = [...initialLineup];
  for (const action of actions) {
    applyPreparationAction(lineup, action);
  }
  return lineup;
}

export function deriveLineupAfterAttackCards(
  initialLineup: (Servant | null)[],
  cards: ReadonlyArray<{ card: string | null | undefined }>
): (Servant | null)[] {
  const lineup = [...initialLineup];
  const npUseCounts = new Map<number, number>();
  for (const card of cards) {
    applyAttackCard(lineup, card.card, npUseCounts);
  }
  return lineup;
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
  const lineup = [...initialLineup];
  const sceneLineups: (Servant | null)[][] = [];
  const npUseCounts = new Map<number, number>();

  for (const scene of scenes) {
    sceneLineups.push([...lineup]);

    for (const action of scene.preparationActions ?? scene.servantActions) {
      applyPreparationAction(lineup, action, "immediate");
    }

    for (const card of scene.attackPriority) {
      applyAttackCard(lineup, card.card, npUseCounts);
    }

    for (const action of scene.preparationActions ?? scene.servantActions) {
      applyPreparationAction(lineup, action, "endOfTurn");
    }
  }

  return sceneLineups;
}
