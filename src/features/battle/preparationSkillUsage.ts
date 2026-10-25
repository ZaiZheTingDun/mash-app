import type { BattleScene, PreparationAction } from "../../types/command";
import { deriveMembersAfterPreparationActions, deriveTurnPartyMembers, resolveMemberRefIndex, type PartyMember } from "../team/partyServants";

/** Display-only history, rebuilt from the configured actions rather than stored separately. */
export type PreparationSkillUsage = ReadonlySet<string>;

export function servantSkillUsageKey(member: PartyMember, skill: string): string {
  const identity = member.memberId ?? `${member.servant?.variantKey}:${member.isSupport}`;
  return `servant:${identity}:${skill}`;
}

export function equipmentSkillUsageKey(skill: string): string {
  return `equipment:${skill}`;
}

export function collectPreparationSkillUsage(initialMembers: PartyMember[], actions: PreparationAction[], previous?: PreparationSkillUsage): PreparationSkillUsage {
  const used = new Set(previous);
  let members = initialMembers;
  for (const action of actions) {
    if (action.type === "equipment" && action.skill) used.add(equipmentSkillUsageKey(action.skill));
    if (action.type === "servant" && action.skill) {
      const index = resolveMemberRefIndex(members, action.servant, action.servantMemberId, action.servantId, action.servantIsSupport);
      const member = index == null ? null : members[index];
      if (member?.servant) used.add(servantSkillUsageKey(member, action.skill));
    }
    members = deriveMembersAfterPreparationActions(members, [action]);
  }
  return used;
}

export function preparationSkillUsageBeforeTurn(initialMembers: PartyMember[], scenes: BattleScene[], wave: number, turn: number): PreparationSkillUsage {
  let used: PreparationSkillUsage = new Set();
  for (let waveIndex = 0; waveIndex <= wave; waveIndex += 1) {
    const turns = scenes[waveIndex].turns;
    const end = waveIndex === wave ? turn : turns.length;
    for (let turnIndex = 0; turnIndex < end; turnIndex += 1) {
      const current = turns[turnIndex];
      const members = deriveTurnPartyMembers(initialMembers, scenes, waveIndex, turnIndex);
      const actions = current.preparationActions ?? [...current.servantActions, ...current.equipmentActions, ...current.commandSpellActions];
      used = collectPreparationSkillUsage(members, actions, used);
    }
  }
  return used;
}
