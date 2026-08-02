import { describe, expect, it } from "vitest";
import type { BattleScene, AdvancedBattleScene } from "../../../types/command";
import type { Servant } from "../../../types/servant";
import type { PartyMember } from "../../team/partyServants";
import {
  buildAdvancedBattleTransitionEvents,
  buildNormalBattleTransitionEvents,
} from "../battleTransitionPreview";

const mash: Servant = {
  id: 1,
  variantKey: "1:3",
  name_cn: "玛修",
  name_jp: "マシュ",
  name_en: "Mash",
  class: "Shielder",
  rarity: 4,
};
const member: PartyMember = { memberId: "slot-a", servant: mash, isSupport: false };

describe("battle transition preview", () => {
  it("applies explicit actions from previous normal turns and current-turn overrides", () => {
    const scenes: BattleScene[] = [
      {
        id: "scene",
        turns: [
          {
            id: "turn-1",
            preparationActions: [
              {
                type: "servant",
                id: "skill",
                servant: "servant_1",
                servantMemberId: "slot-a",
                servantId: 1,
                skill: "skill_2",
                target: null,
              },
            ],
            servantActions: [],
            equipmentActions: [],
            commandSpellActions: [],
            attackPriority: [
              { id: "np", card: "servant_1_np", memberId: "slot-a", servantId: 1 },
            ],
          },
          {
            id: "turn-2",
            battleStateOverrides: [
              {
                memberId: "slot-a",
                servantId: 1,
                stateKey: "manual",
                mode: "set",
                remainingTurns: 2,
              },
            ],
            preparationActions: [],
            servantActions: [],
            equipmentActions: [],
            commandSpellActions: [],
            attackPriority: [],
          },
        ],
      },
    ];

    expect(buildNormalBattleTransitionEvents(member, 0, scenes, 0, 1)).toEqual([
      { type: "skill", slot: 2, selectionIndex: undefined },
      { type: "noblePhantasm" },
      { type: "turnEnd" },
      {
        type: "override",
        stateKey: "manual",
        mode: "set",
        remainingTurns: 2,
        stacks: undefined,
      },
    ]);
  });

  it("does not speculate about an unbound advanced NP output", () => {
    const scene: AdvancedBattleScene = {
      id: "advanced",
      mainOutput: { servant: null, outputType: "np", npCard: "auto" },
      turns: [
        { id: "turn-1", actions: [] },
        { id: "turn-2", actions: [] },
      ],
      rules: [],
    };
    expect(buildAdvancedBattleTransitionEvents(member, 0, scene, 1)).toEqual([
      { type: "uncertain" },
      { type: "turnEnd" },
    ]);
  });

  it("keeps old slot-only actions compatible", () => {
    const scene: AdvancedBattleScene = {
      id: "advanced",
      turns: [
        {
          id: "turn-1",
          actions: [
            {
              type: "servant",
              id: "legacy",
              servant: "servant_1",
              skill: "skill_1",
              target: null,
            },
          ],
        },
        { id: "turn-2", actions: [] },
      ],
      rules: [],
    };
    expect(buildAdvancedBattleTransitionEvents(member, 0, scene, 1)).toContainEqual({
      type: "skill",
      slot: 1,
      selectionIndex: undefined,
    });
  });
});
