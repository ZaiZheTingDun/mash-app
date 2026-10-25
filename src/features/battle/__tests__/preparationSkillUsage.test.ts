import { describe, expect, it } from "vitest";
import { collectPreparationSkillUsage, equipmentSkillUsageKey, preparationSkillUsageBeforeTurn, servantSkillUsageKey } from "../preparationSkillUsage";
import type { PartyMember } from "../../team/partyServants";
import type { BattleScene, BattleTurn, PreparationAction } from "../../../types/command";

const members: PartyMember[] = Array.from({ length: 6 }, (_, index) => ({
  memberId: `member-${index}`, isSupport: index === 1,
  servant: { id: index === 1 ? 1 : index + 1, variantKey: index === 1 ? "1:1" : `${index + 1}:1`, name_cn: "", name_jp: "", name_en: "", class: "saber", rarity: 5 },
}));
const skill = (source: string, option: string): PreparationAction => ({ type: "servant", id: `${source}-${option}`, servant: source, skill: option, target: null });
function turn(id: string, actions: PreparationAction[], attackPriority: BattleTurn["attackPriority"] = []): BattleTurn {
  return { id, preparationActions: actions, servantActions: [], equipmentActions: [], commandSpellActions: [], enemyTarget: null, attackPriority };
}

describe("preparation skill usage display", () => {
  it("follows individual members through order changes and tracks equipment separately", () => {
    const actions: PreparationAction[] = [
      skill("servant_1", "skill_1"), skill("servant_2", "skill_2"),
      { type: "equipment", id: "swap", skill: "skill_3", target: null, orderChange: { front: "servant_1", back: "servant_4" } },
      skill("servant_1", "skill_3"),
    ];
    const used = collectPreparationSkillUsage(members, actions);
    expect(used.has(servantSkillUsageKey(members[0], "skill_1"))).toBe(true);
    expect(used.has(servantSkillUsageKey(members[1], "skill_2"))).toBe(true);
    expect(used.has(servantSkillUsageKey(members[0], "skill_2"))).toBe(false);
    expect(used.has(servantSkillUsageKey(members[3], "skill_3"))).toBe(true);
    expect(used.has(servantSkillUsageKey(members[0], "skill_3"))).toBe(false);
    expect(used.has(equipmentSkillUsageKey("skill_3"))).toBe(true);
  });

  it("distinguishes owned and support copies in legacy data without member IDs", () => {
    const legacy = members.map(member => ({ ...member, memberId: null }));
    const used = collectPreparationSkillUsage(legacy, [skill("servant_2", "skill_1")]);
    expect(used.has(servantSkillUsageKey(legacy[0], "skill_1"))).toBe(false);
    expect(used.has(servantSkillUsageKey(legacy[1], "skill_1"))).toBe(true);
  });

  it("carries history across turns and waves, uses the existing NP lineup replay, and excludes future skills", () => {
    const party = members.map((member, index) => index === 0 ? { ...member, servant: { ...member.servant!, id: 16, variantKey: "16:1" } } : member);
    const scenes: BattleScene[] = [
      { id: "wave-1", turns: [
        turn("first", [skill("servant_1", "skill_1")], [{ id: "np", card: "servant_1_np" }]),
        turn("second", [skill("servant_1", "skill_2")]),
      ] },
      { id: "wave-2", turns: [turn("future", [skill("servant_3", "skill_3")])] },
    ];
    expect(preparationSkillUsageBeforeTurn(party, scenes, 0, 0).size).toBe(0);
    const used = preparationSkillUsageBeforeTurn(party, scenes, 1, 0);
    expect(used.has(servantSkillUsageKey(party[0], "skill_1"))).toBe(true);
    expect(used.has(servantSkillUsageKey(party[3], "skill_2"))).toBe(true);
    expect(used.has(servantSkillUsageKey(party[0], "skill_2"))).toBe(false);
    expect(used.has(servantSkillUsageKey(party[2], "skill_3"))).toBe(false);
  });

  it("recomputes usage after deleting actions instead of retaining stale click state", () => {
    const first = skill("servant_1", "skill_1");
    const scenes: BattleScene[] = [{ id: "wave", turns: [turn("first", [first]), turn("second", [])] }];
    expect(preparationSkillUsageBeforeTurn(members, scenes, 0, 1).has(servantSkillUsageKey(members[0], "skill_1"))).toBe(true);
    scenes[0].turns[0].preparationActions = [];
    expect(preparationSkillUsageBeforeTurn(members, scenes, 0, 1).size).toBe(0);
    expect(collectPreparationSkillUsage(members, [first, first]).size).toBe(1);
  });
});
