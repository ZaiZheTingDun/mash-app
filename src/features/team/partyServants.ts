import type { SlotItem } from "./ContentGrid";
import type { Project } from "../../types/project";
import type {
  AdvancedBattleScene,
  AdvancedAttackAction,
  AdvancedCommandCardCondition,
  AdvancedNpSlotCondition,
  AttackCard,
  BattleScene,
  BattleTurn,
  PreparationAction,
} from "../../types/command";
import type { Servant } from "../../types/servant";
import changeOrderRulesJson from "../../../src-tauri/src/resources/change_order_servants.json";

export interface PartyMember {
  memberId?: string | null;
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
  return lineup.map((servant) => ({ memberId: null, servant, isSupport: false }));
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
    memberId: slot.id,
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

export function memberRefAt(
  members: PartyMember[],
  value: string | null | undefined
): { memberId: string | null; servantId: number | null; isSupport: boolean } {
  const index = parseServantPosition(value);
  const member = index == null ? null : members[index] ?? null;
  return {
    memberId: member?.memberId ?? null,
    servantId: member?.servant?.id ?? null,
    isSupport: member?.isSupport === true,
  };
}

export function resolveMemberRefIndex(
  members: PartyMember[],
  fallbackValue: string | null | undefined,
  memberId: string | null | undefined,
  servantId: number | null | undefined,
  isSupport: boolean | null | undefined
): number | null {
  if (memberId) {
    const index = members.findIndex((member) => member.memberId === memberId);
    if (index >= 0) return index;
  }
  if (servantId != null) {
    const support = isSupport === true;
    const index = members.findIndex(
      (member) => member.servant?.id === servantId && member.isSupport === support
    );
    if (index >= 0) return index;
  }
  return parseServantPosition(fallbackValue);
}

function resolveActionSlot(
  members: PartyMember[],
  fallbackValue: string | null | undefined,
  memberId: string | null | undefined,
  servantId: number | null | undefined,
  isSupport: boolean | null | undefined
): string | null {
  const index = resolveMemberRefIndex(members, fallbackValue, memberId, servantId, isSupport);
  return index == null ? fallbackValue ?? null : `servant_${index + 1}`;
}

function slotStringForMemberRef(
  previousMembers: PartyMember[],
  nextMembers: PartyMember[],
  fallbackValue: string | null | undefined,
  memberId: string | null | undefined,
  servantId: number | null | undefined,
  isSupport: boolean | null | undefined,
  maxIndex: number
): { value: string | null; memberId: string | null; servantId: number | null; isSupport: boolean } {
  const fallbackRef = memberRefAt(previousMembers, fallbackValue);
  const resolvedMemberId = memberId ?? fallbackRef.memberId;
  const resolvedServantId = servantId ?? fallbackRef.servantId;
  const resolvedIsSupport = isSupport ?? fallbackRef.isSupport;
  const index = resolveMemberRefIndex(
    nextMembers,
    fallbackValue,
    resolvedMemberId,
    resolvedServantId,
    resolvedIsSupport
  );
  return {
    value: index != null && index < maxIndex ? `servant_${index + 1}` : fallbackValue ?? null,
    memberId: resolvedMemberId,
    servantId: resolvedServantId,
    isSupport: resolvedIsSupport,
  };
}

export function resolvePreparationAction(action: PreparationAction, members: PartyMember[]): PreparationAction {
  if (action.type === "servant") {
    return {
      ...action,
      servant: resolveActionSlot(
        members,
        action.servant,
        action.servantMemberId,
        action.servantId,
        action.servantIsSupport
      ),
      target: resolveActionSlot(
        members,
        action.target,
        action.targetMemberId,
        action.targetServantId,
        action.targetIsSupport
      ),
    };
  }
  if (action.type === "equipment") {
    return {
      ...action,
      target: resolveActionSlot(
        members,
        action.target,
        action.targetMemberId,
        action.targetServantId,
        action.targetIsSupport
      ),
      orderChange: action.orderChange
        ? {
            ...action.orderChange,
            front: resolveActionSlot(
              members,
              action.orderChange.front,
              action.orderChange.frontMemberId,
              action.orderChange.frontServantId,
              action.orderChange.frontIsSupport
            ),
            back: resolveActionSlot(
              members,
              action.orderChange.back,
              action.orderChange.backMemberId,
              action.orderChange.backServantId,
              action.orderChange.backIsSupport
            ),
          }
        : action.orderChange,
    };
  }
  if (action.type === "enemyTarget") {
    return action;
  }
  return {
    ...action,
    target: resolveActionSlot(
      members,
      action.target,
      action.targetMemberId,
      action.targetServantId,
      action.targetIsSupport
    ),
  };
}

function relocateMemberRef(
  previousMembers: PartyMember[],
  nextMembers: PartyMember[],
  fallbackValue: string | null | undefined,
  memberId: string | null | undefined,
  servantId: number | null | undefined,
  isSupport: boolean | null | undefined
): {
  value: string | null;
  memberId: string | null;
  servantId: number | null;
  isSupport: boolean;
} {
  const fallbackRef = memberRefAt(previousMembers, fallbackValue);
  const resolvedMemberId = memberId ?? fallbackRef.memberId;
  const resolvedServantId = servantId ?? fallbackRef.servantId;
  const resolvedIsSupport = isSupport ?? fallbackRef.isSupport;
  const index = resolveMemberRefIndex(
    nextMembers,
    fallbackValue,
    resolvedMemberId,
    resolvedServantId,
    resolvedIsSupport
  );
  const nextMember = index == null ? null : nextMembers[index] ?? null;

  return {
    value: index == null ? fallbackValue ?? null : `servant_${index + 1}`,
    // A stable slot/member can keep its memberId while its servant changes.
    // Refresh all identity fields from the next lineup so logs and later
    // runtime resolution do not retain the replaced servant's id.
    memberId: nextMember?.memberId ?? resolvedMemberId,
    servantId: nextMember?.servant?.id ?? resolvedServantId,
    isSupport: nextMember?.isSupport ?? resolvedIsSupport,
  };
}

export function relocatePreparationActionMembers(
  action: PreparationAction,
  previousMembers: PartyMember[],
  nextMembers: PartyMember[]
): PreparationAction {
  if (action.type === "servant") {
    const sourceRef = relocateMemberRef(
      previousMembers,
      nextMembers,
      action.servant,
      action.servantMemberId,
      action.servantId,
      action.servantIsSupport
    );
    const targetRef = relocateMemberRef(
      previousMembers,
      nextMembers,
      action.target,
      action.targetMemberId,
      action.targetServantId,
      action.targetIsSupport
    );
    return {
      ...action,
      servant: sourceRef.value,
      servantMemberId: sourceRef.memberId,
      servantId: sourceRef.servantId,
      servantIsSupport: sourceRef.isSupport,
      target: targetRef.value,
      targetMemberId: targetRef.memberId,
      targetServantId: targetRef.servantId,
      targetIsSupport: targetRef.isSupport,
    };
  }

  if (action.type === "equipment") {
    const targetRef = relocateMemberRef(
      previousMembers,
      nextMembers,
      action.target,
      action.targetMemberId,
      action.targetServantId,
      action.targetIsSupport
    );
    const orderChange = action.orderChange
      ? (() => {
          const frontRef = relocateMemberRef(
            previousMembers,
            nextMembers,
            action.orderChange?.front,
            action.orderChange?.frontMemberId,
            action.orderChange?.frontServantId,
            action.orderChange?.frontIsSupport
          );
          const backRef = relocateMemberRef(
            previousMembers,
            nextMembers,
            action.orderChange?.back,
            action.orderChange?.backMemberId,
            action.orderChange?.backServantId,
            action.orderChange?.backIsSupport
          );
          return {
            ...action.orderChange,
            front: frontRef.value,
            frontMemberId: frontRef.memberId,
            frontServantId: frontRef.servantId,
            frontIsSupport: frontRef.isSupport,
            back: backRef.value,
            backMemberId: backRef.memberId,
            backServantId: backRef.servantId,
            backIsSupport: backRef.isSupport,
          };
        })()
      : action.orderChange;
    return {
      ...action,
      target: targetRef.value,
      targetMemberId: targetRef.memberId,
      targetServantId: targetRef.servantId,
      targetIsSupport: targetRef.isSupport,
      orderChange,
    };
  }

  if (action.type === "enemyTarget") return action;

  const targetRef = relocateMemberRef(
    previousMembers,
    nextMembers,
    action.target,
    action.targetMemberId,
    action.targetServantId,
    action.targetIsSupport
  );
  return {
    ...action,
    target: targetRef.value,
    targetMemberId: targetRef.memberId,
    targetServantId: targetRef.servantId,
    targetIsSupport: targetRef.isSupport,
  };
}

export function relocateAdvancedBattleSceneMembers(
  scene: AdvancedBattleScene,
  previousMembers: PartyMember[],
  nextMembers: PartyMember[]
): AdvancedBattleScene {
  const mainOutput = scene.mainOutput
    ? (() => {
        const ref = slotStringForMemberRef(
          previousMembers,
          nextMembers,
          scene.mainOutput?.servant,
          scene.mainOutput?.memberId,
          scene.mainOutput?.servantId,
          scene.mainOutput?.isSupport,
          3
        );
        return {
          ...scene.mainOutput,
          servant: ref.value as AdvancedBattleScene["mainOutput"] extends infer T
            ? T extends { servant: infer S }
              ? S
              : never
            : never,
          memberId: ref.memberId,
          servantId: ref.servantId,
          isSupport: ref.isSupport,
        };
      })()
    : scene.mainOutput;
  return {
    ...scene,
    mainOutput,
    commandConditions: scene.commandConditions?.map((condition) =>
      relocateAdvancedCommandCardCondition(condition, previousMembers, nextMembers)
    ),
    controlActions: scene.controlActions?.map((action) =>
      relocatePreparationActionMembers(action, previousMembers, nextMembers)
    ),
    turns: scene.turns?.map((turn) => ({
      ...turn,
      actions: turn.actions.map((action) =>
        relocatePreparationActionMembers(action, previousMembers, nextMembers)
      ),
    })),
    startupActions: scene.startupActions?.map((action) =>
      relocatePreparationActionMembers(action, previousMembers, nextMembers)
    ),
    rules: scene.rules.map((rule) => ({
      ...rule,
      npConditionGroups: rule.npConditionGroups.map((group) => ({
        ...group,
        slots: group.slots.map((slot) =>
          relocateAdvancedNpSlotCondition(slot, previousMembers, nextMembers)
        ),
      })),
      commandConditionGroups: rule.commandConditionGroups.map((group) => ({
        ...group,
        cards: group.cards.map((condition) =>
          relocateAdvancedCommandCardCondition(condition, previousMembers, nextMembers)
        ),
      })),
      actions: rule.actions.map((action) =>
        action.type === "attack"
          ? relocateAdvancedAttackActionMembers(action, previousMembers, nextMembers)
          : relocatePreparationActionMembers(action, previousMembers, nextMembers)
      ),
    })),
  };
}

function relocateAdvancedAttackActionMembers(
  action: AdvancedAttackAction,
  previousMembers: PartyMember[],
  nextMembers: PartyMember[]
): AdvancedAttackAction {
  return relocateAttackCardMembers(action, previousMembers, nextMembers) as AdvancedAttackAction;
}

function relocateAdvancedNpSlotCondition(
  condition: AdvancedNpSlotCondition,
  previousMembers: PartyMember[],
  nextMembers: PartyMember[]
): AdvancedNpSlotCondition {
  const ref = slotStringForMemberRef(
    previousMembers,
    nextMembers,
    condition.servant,
    condition.memberId,
    condition.servantId,
    condition.isSupport,
    3
  );
  return {
    ...condition,
    servant: (ref.value ?? condition.servant) as AdvancedNpSlotCondition["servant"],
    memberId: ref.memberId,
    servantId: ref.servantId,
    isSupport: ref.isSupport,
  };
}

function relocateAdvancedCommandCardCondition(
  condition: AdvancedCommandCardCondition,
  previousMembers: PartyMember[],
  nextMembers: PartyMember[]
): AdvancedCommandCardCondition {
  if (condition.servant === "any") return condition;
  const ref = slotStringForMemberRef(
    previousMembers,
    nextMembers,
    condition.servant,
    condition.memberId,
    condition.servantId,
    condition.isSupport,
    3
  );
  return {
    ...condition,
    servant: (ref.value ?? condition.servant) as AdvancedCommandCardCondition["servant"],
    memberId: ref.memberId,
    servantId: ref.servantId,
    isSupport: ref.isSupport,
  };
}

function relocateAttackCardMembers(
  card: AttackCard,
  previousMembers: PartyMember[],
  nextMembers: PartyMember[]
): AttackCard {
  const match = card.card?.match(/^(servant_[1-6])_(.+)$/);
  if (!match) return card;
  const ref = slotStringForMemberRef(
    previousMembers,
    nextMembers,
    match[1],
    card.memberId,
    card.servantId,
    card.isSupport,
    3
  );
  return {
    ...card,
    card: ref.value ? `${ref.value}_${match[2]}` : card.card,
    memberId: ref.memberId,
    servantId: ref.servantId,
    isSupport: ref.isSupport,
  };
}

function relocateTurnMemberRef<T extends {
  memberId?: string | null;
  slotIndex?: number | null;
  servantId?: number | null;
  isSupport?: boolean;
}>(
  ref: T,
  previousMembers: PartyMember[],
  nextMembers: PartyMember[]
): T {
  const previous = ref.slotIndex == null ? null : previousMembers[ref.slotIndex] ?? null;
  const memberId = ref.memberId ?? previous?.memberId ?? null;
  const servantId = ref.servantId ?? previous?.servant?.id ?? null;
  const isSupport = ref.isSupport ?? previous?.isSupport ?? false;
  const nextIndex = resolveMemberRefIndex(
    nextMembers,
    ref.slotIndex == null ? null : `servant_${ref.slotIndex + 1}`,
    memberId,
    servantId,
    isSupport
  );
  const next = nextIndex == null ? null : nextMembers[nextIndex] ?? null;
  return {
    ...ref,
    memberId: next?.memberId ?? memberId,
    slotIndex: nextIndex ?? ref.slotIndex,
    servantId: next?.servant?.id ?? servantId,
    isSupport: next?.isSupport ?? isSupport,
  };
}

function relocateCriticalMemberPriority(
  priority: NonNullable<BattleTurn["criticalStrategy"]>["memberPriority"],
  previousMembers: PartyMember[],
  nextMembers: PartyMember[]
): NonNullable<BattleTurn["criticalStrategy"]>["memberPriority"] {
  const relocated = priority.flatMap((item) => {
    const previous = previousMembers[item.slotIndex] ?? null;
    const memberId = item.memberId ?? previous?.memberId ?? null;
    const servantId = item.servantId ?? previous?.servant?.id ?? null;
    const isSupport = item.isSupport ?? previous?.isSupport ?? false;
    const nextIndex = memberId
      ? nextMembers.findIndex((member) => member.memberId === memberId)
      : nextMembers.findIndex(
          (member) => member.servant?.id === servantId && member.isSupport === isSupport
        );
    if (nextIndex < 0 || !nextMembers[nextIndex]?.servant) return [];
    const next = nextMembers[nextIndex];
    return [{
      memberId: next.memberId ?? memberId,
      slotIndex: nextIndex,
      servantId: next.servant?.id ?? servantId,
      isSupport: next.isSupport,
    }];
  });
  for (const [slotIndex, member] of nextMembers.entries()) {
    if (!member.servant) continue;
    const alreadyIncluded = relocated.some((item) =>
      member.memberId
        ? item.memberId === member.memberId
        : item.servantId === member.servant?.id && item.isSupport === member.isSupport
    );
    if (!alreadyIncluded) {
      relocated.push({
        memberId: member.memberId ?? null,
        slotIndex,
        servantId: member.servant.id,
        isSupport: member.isSupport,
      });
    }
  }
  return relocated;
}

function relocateBattleTurnMembers(
  turn: BattleTurn,
  previousMembers: PartyMember[],
  nextMembers: PartyMember[]
): BattleTurn {
  return {
    ...turn,
    preparationActions: turn.preparationActions.map((action) =>
      relocatePreparationActionMembers(action, previousMembers, nextMembers)
    ),
    servantActions: turn.servantActions?.map((action) =>
      relocatePreparationActionMembers(action, previousMembers, nextMembers) as typeof action
    ),
    equipmentActions: turn.equipmentActions?.map((action) =>
      relocatePreparationActionMembers(action, previousMembers, nextMembers) as typeof action
    ),
    commandSpellActions: turn.commandSpellActions?.map((action) =>
      relocatePreparationActionMembers(action, previousMembers, nextMembers) as typeof action
    ),
    attackPriority: turn.attackPriority.map((card) =>
      relocateAttackCardMembers(card, previousMembers, nextMembers)
    ),
    criticalStrategy: turn.criticalStrategy
      ? {
          ...turn.criticalStrategy,
          memberPriority: relocateCriticalMemberPriority(
            turn.criticalStrategy.memberPriority,
            previousMembers,
            nextMembers
          ),
        }
      : turn.criticalStrategy,
    advancedCardStrategy: turn.advancedCardStrategy
      ? {
          customRules: turn.advancedCardStrategy.customRules.map((rule) => ({
            ...rule,
            slots: rule.slots.map((slot) =>
              slot.grandServant
                ? slot
                : relocateTurnMemberRef(slot, previousMembers, nextMembers)
            ),
          })),
        }
      : turn.advancedCardStrategy,
  };
}

export function relocateBattleSceneMembers(
  scene: BattleScene,
  previousMembers: PartyMember[],
  nextMembers: PartyMember[]
): BattleScene {
  return {
    ...scene,
    turns: scene.turns.map((turn) =>
      relocateBattleTurnMembers(turn, previousMembers, nextMembers)
    ),
    preparationActions: scene.preparationActions?.map((action) =>
      relocatePreparationActionMembers(action, previousMembers, nextMembers)
    ),
    servantActions: scene.servantActions?.map((action) =>
      relocatePreparationActionMembers(action, previousMembers, nextMembers) as typeof action
    ),
    equipmentActions: scene.equipmentActions?.map((action) =>
      relocatePreparationActionMembers(action, previousMembers, nextMembers) as typeof action
    ),
    commandSpellActions: scene.commandSpellActions?.map((action) =>
      relocatePreparationActionMembers(action, previousMembers, nextMembers) as typeof action
    ),
    attackPriority: scene.attackPriority?.map((card) =>
      relocateAttackCardMembers(card, previousMembers, nextMembers)
    ),
  };
}

function compactBackline(members: PartyMember[]) {
  const backline = members
    .slice(3)
    .filter((member) => Boolean(member.servant));
  for (let i = 3; i < members.length; i += 1) {
    members[i] = backline[i - 3] ?? { memberId: null, servant: null, isSupport: false };
  }
}

function removeAt(members: PartyMember[], index: number) {
  if (index < 0 || index >= 3 || !members[index]?.servant) return;
  const replacementIndex = members.findIndex((member, i) => i >= 3 && member.servant);
  members[index] =
    replacementIndex === -1
      ? { memberId: null, servant: null, isSupport: false }
      : { ...members[replacementIndex] };
  if (replacementIndex !== -1) {
    members[replacementIndex] = { memberId: null, servant: null, isSupport: false };
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
  const resolvedAction = resolvePreparationAction(action, members);
  if (timing === "endOfTurn" && action.type !== "servant") return;

  if (resolvedAction.type === "equipment" && resolvedAction.orderChange) {
    const frontIndex = parseServantPosition(resolvedAction.orderChange.front);
    const backIndex = parseServantPosition(resolvedAction.orderChange.back);
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

  if (resolvedAction.type !== "servant") return;
  const sourceIndex = parseServantPosition(resolvedAction.servant);
  if (sourceIndex == null || !resolvedAction.skill) return;
  const servant = members[sourceIndex]?.servant;
  if (!servant) return;
  const rule = CHANGE_ORDER_RULES.find(
    (r) =>
      r.servantId === servant.id &&
      r.trigger.type === "servantSkill" &&
      r.trigger.skill === resolvedAction.skill &&
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
