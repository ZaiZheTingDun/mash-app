import { describe, expect, it } from "vitest";
import { relocateGrandCardStrategySlots, relocateGrandServants } from "../grandRuleSlots";
import type { SlotItem } from "../contentGridTypes";
import type { GrandCardStrategy } from "../../types/project";
import type { Servant } from "../../types/servant";

const ALTRIA: Servant = {
  id: 10,
  variantKey: "10",
  name_cn: "阿尔托莉雅",
  name_jp: "アルトリア",
  name_en: "Altria",
  class: "Saber",
  rarity: 5,
  noblePhantasmCard: "buster",
};

function slot(id: string, type: SlotItem["type"], servant: Servant | null): SlotItem {
  return {
    id,
    type,
    servant,
    craftEssence: null,
    craftEssenceMlbRequired: true,
  };
}

describe("relocateGrandCardStrategySlots", () => {
  it("keeps a duplicate support servant rule bound to the support slot after party reorder", () => {
    const strategy: GrandCardStrategy = {
      customRules: [
        {
          id: "rule_1",
          name: "指定助战",
          slots: [
            {
              slotIndex: 2,
              servantId: ALTRIA.id,
              isSupport: true,
              grandServant: false,
              kind: "np",
              color: "any",
            },
            {
              slotIndex: 0,
              servantId: ALTRIA.id,
              isSupport: false,
              grandServant: false,
              kind: "np",
              color: "any",
            },
            {
              slotIndex: null,
              servantId: null,
              isSupport: false,
              grandServant: true,
              kind: "any",
              color: "any",
            },
          ],
        },
      ],
    };
    const reordered = [
      slot("slot-2", "support", null),
      slot("slot-0", "servant", ALTRIA),
      slot("slot-1", "servant", null),
    ];

    const relocated = relocateGrandCardStrategySlots(strategy, reordered, ALTRIA.id);
    const [supportRule, ownedRule] = relocated?.customRules?.[0].slots ?? [];

    expect(supportRule).toMatchObject({
      slotIndex: 0,
      servantId: ALTRIA.id,
      isSupport: true,
    });
    expect(ownedRule).toMatchObject({
      slotIndex: 1,
      servantId: ALTRIA.id,
      isSupport: false,
    });
  });
});

describe("relocateGrandServants", () => {
  it("keeps grand servant settings bound to the same member after party reorder", () => {
    const reordered = [
      slot("slot-2", "support", null),
      slot("slot-0", "servant", ALTRIA),
      slot("slot-1", "servant", null),
    ];

    const relocated = relocateGrandServants(
      [
        {
          memberId: "slot-0",
          slotIndex: 0,
          servantId: ALTRIA.id,
          isSupport: false,
          npCard: "auto",
          priority: "damage",
        },
        {
          memberId: "slot-2",
          slotIndex: 2,
          servantId: ALTRIA.id,
          isSupport: true,
          npCard: "buster",
          priority: "np",
        },
      ],
      reordered,
      ALTRIA.id
    );

    expect(relocated).toEqual([
      {
        memberId: "slot-0",
        slotIndex: 1,
        servantId: ALTRIA.id,
        isSupport: false,
        npCard: "auto",
        priority: "damage",
      },
      {
        memberId: "slot-2",
        slotIndex: 0,
        servantId: ALTRIA.id,
        isSupport: true,
        npCard: "buster",
        priority: "np",
      },
    ]);
  });
});
