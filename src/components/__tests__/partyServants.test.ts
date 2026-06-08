import { describe, it, expect } from "vitest";
import {
  deriveMembersAfterAttackCards,
  deriveMembersAfterPreparationActions,
  derivePartyMembers,
  deriveLineupAfterPreparationActions,
  derivePartyLineup,
  derivePartyServants,
  deriveScenePartyMembers,
  deriveScenePartyLineups,
  deriveScenePartyServants,
} from "../partyServants";
import type { SlotItem } from "../ContentGrid";
import { createInitialProjectSlots } from "../projectSlots";
import type { BattleScene, BattleTurn } from "../../types/command";
import type { Project } from "../../types/project";
import type { Servant } from "../../types/servant";

const MASH: Servant = {
  id: 1,
  variantKey: "1",
  name_cn: "玛修",
  name_jp: "マシュ・キリエライト",
  name_en: "Mash Kyrielight",
  class: "Shielder",
  rarity: 4,
};

const ALTRIA: Servant = {
  id: 100,
  variantKey: "100",
  name_cn: "阿尔托莉雅",
  name_jp: "アルトリア",
  name_en: "Altria",
  class: "Saber",
  rarity: 5,
};

const MERLIN: Servant = {
  id: 150,
  variantKey: "150",
  name_cn: "梅林",
  name_jp: "マーリン",
  name_en: "Merlin",
  class: "Caster",
  rarity: 5,
};

const WAVER: Servant = {
  id: 200,
  variantKey: "200",
  name_cn: "韦伯",
  name_jp: "ウェイバー",
  name_en: "Waver",
  class: "Caster",
  rarity: 5,
};

const ARASH: Servant = {
  id: 16,
  variantKey: "16",
  name_cn: "阿拉什",
  name_jp: "アーラシュ",
  name_en: "Arash",
  class: "Archer",
  rarity: 1,
};

const CHEN_GONG: Servant = {
  id: 258,
  variantKey: "258",
  name_cn: "陈宫",
  name_jp: "陳宮",
  name_en: "Chen Gong",
  class: "Caster",
  rarity: 2,
};

const HABETROT: Servant = {
  id: 315,
  variantKey: "315",
  name_cn: "哈贝特洛特",
  name_jp: "ハベトロット",
  name_en: "Habetrot",
  class: "Rider",
  rarity: 4,
};

const CHLOE: Servant = {
  id: 388,
  variantKey: "388",
  name_cn: "克洛伊",
  name_jp: "クロエ",
  name_en: "Chloe",
  class: "Archer",
  rarity: 4,
};

const ULTIMATE_ELISABETH: Servant = {
  id: 458,
  variantKey: "458",
  name_cn: "终结之伊丽莎白",
  name_jp: "終わりのエリザベート",
  name_en: "Ultimate Elisabeth",
  class: "Avenger",
  rarity: 4,
};

const SERVANTS: Servant[] = [
  MASH,
  ALTRIA,
  MERLIN,
  WAVER,
  ARASH,
  CHEN_GONG,
  HABETROT,
  CHLOE,
  ULTIMATE_ELISABETH,
];

function makeSlots(
  layout: ReadonlyArray<{ type: "servant" | "support"; servant: Servant | null }>
): SlotItem[] {
  return createInitialProjectSlots()
    .slice(0, layout.length)
    .map((s, i) => ({
      id: s.id,
      type: layout[i].type,
      servant: layout[i].servant,
      craftEssence: null,
    }));
}

function makeProject(supportServantId: number | null): Project {
  return {
    id: "p1",
    name: "Test",
    supportServantId,
    slots: createInitialProjectSlots(),
    repeatMission: false,
  };
}

function makeScene(overrides: Partial<BattleTurn> & { id?: string } = {}): BattleScene {
  const { id = "scene_1", ...turnOverrides } = overrides;
  return {
    id,
    turns: [
      {
        id: `${id}_turn_1`,
        preparationActions: [],
        servantActions: [],
        equipmentActions: [],
        commandSpellActions: [],
        attackPriority: [
          { id: "atk_0", card: null },
          { id: "atk_1", card: null },
          { id: "atk_2", card: null },
        ],
        ...turnOverrides,
      },
    ],
  };
}

describe("derivePartyServants", () => {
  it("uses the pinned support servant when support sits at position 3", () => {
    // Default layout: [servant, servant, support, servant, servant, servant]
    const slots = makeSlots([
      { type: "servant", servant: ALTRIA },
      { type: "servant", servant: MERLIN },
      { type: "support", servant: null },
      { type: "servant", servant: WAVER },
      { type: "servant", servant: null },
      { type: "servant", servant: null },
    ]);
    const project = makeProject(MASH.id);

    const party = derivePartyServants(slots, project, SERVANTS);

    // Position 3 should resolve to the pinned support (Mash), NOT to the
    // 4th slot's servant (Waver) — that was the regression.
    expect(party).toEqual([ALTRIA, MERLIN, MASH]);
  });

  it("returns null at the support position when no support is pinned", () => {
    const slots = makeSlots([
      { type: "servant", servant: ALTRIA },
      { type: "servant", servant: MERLIN },
      { type: "support", servant: null },
      { type: "servant", servant: WAVER },
    ]);
    const project = makeProject(null);

    expect(derivePartyServants(slots, project, SERVANTS)).toEqual([
      ALTRIA,
      MERLIN,
      null,
    ]);
  });

  it("honours support placed at position 1", () => {
    const slots = makeSlots([
      { type: "support", servant: null },
      { type: "servant", servant: ALTRIA },
      { type: "servant", servant: MERLIN },
      { type: "servant", servant: WAVER },
    ]);
    const project = makeProject(MASH.id);

    expect(derivePartyServants(slots, project, SERVANTS)).toEqual([
      MASH,
      ALTRIA,
      MERLIN,
    ]);
  });

  it("ignores the support servant when support sits beyond position 3", () => {
    // Support dragged to position 5 — front-line is just the first 3
    // party slots and the pinned support never enters the team.
    const slots = makeSlots([
      { type: "servant", servant: ALTRIA },
      { type: "servant", servant: MERLIN },
      { type: "servant", servant: WAVER },
      { type: "servant", servant: null },
      { type: "support", servant: null },
      { type: "servant", servant: null },
    ]);
    const project = makeProject(MASH.id);

    expect(derivePartyServants(slots, project, SERVANTS)).toEqual([
      ALTRIA,
      MERLIN,
      WAVER,
    ]);
  });

  it("returns nulls when no project is active", () => {
    const slots = makeSlots([
      { type: "servant", servant: null },
      { type: "servant", servant: null },
      { type: "support", servant: null },
    ]);

    expect(derivePartyServants(slots, null, SERVANTS)).toEqual([
      null,
      null,
      null,
    ]);
  });
});

describe("derivePartyMembers", () => {
  it("marks only the support slot when owned and support servants match", () => {
    const slots = makeSlots([
      { type: "servant", servant: ALTRIA },
      { type: "support", servant: null },
      { type: "servant", servant: MERLIN },
    ]);
    const members = derivePartyMembers(slots, makeProject(ALTRIA.id), SERVANTS);

    expect(members.map((member) => member.servant)).toEqual([ALTRIA, ALTRIA, MERLIN]);
    expect(members.map((member) => member.isSupport)).toEqual([false, true, false]);
  });

  it("moves the support slot marker with Order Change", () => {
    const members = [
      { servant: MERLIN, isSupport: false },
      { servant: WAVER, isSupport: false },
      { servant: MASH, isSupport: false },
      { servant: ALTRIA, isSupport: true },
      { servant: CHEN_GONG, isSupport: false },
      { servant: null, isSupport: false },
    ];

    const next = deriveMembersAfterPreparationActions(members, [
      {
        type: "equipment",
        id: "eq_1",
        skill: "skill_3",
        target: null,
        orderChange: {
          front: "servant_2",
          back: "servant_4",
        },
      },
    ]);

    expect(next.map((member) => member.servant)).toEqual([
      MERLIN,
      ALTRIA,
      MASH,
      WAVER,
      CHEN_GONG,
      null,
    ]);
    expect(next.map((member) => member.isSupport)).toEqual([
      false,
      true,
      false,
      false,
      false,
      false,
    ]);
  });

  it("keeps the support marker on the substitute after a front-line exit", () => {
    const members = [
      { servant: ARASH, isSupport: false },
      { servant: MERLIN, isSupport: false },
      { servant: WAVER, isSupport: false },
      { servant: ALTRIA, isSupport: true },
      { servant: MASH, isSupport: false },
      { servant: CHEN_GONG, isSupport: false },
    ];

    const next = deriveMembersAfterAttackCards(members, [
      { card: "servant_1_np" },
    ]);

    expect(next.map((member) => member.servant)).toEqual([
      ALTRIA,
      MERLIN,
      WAVER,
      MASH,
      CHEN_GONG,
      null,
    ]);
    expect(next.map((member) => member.isSupport)).toEqual([
      true,
      false,
      false,
      false,
      false,
      false,
    ]);
  });

  it("carries the support marker across battle scene transitions", () => {
    const lineups = deriveScenePartyMembers(
      [
        { servant: ARASH, isSupport: false },
        { servant: MERLIN, isSupport: false },
        { servant: WAVER, isSupport: false },
        { servant: ALTRIA, isSupport: true },
        { servant: MASH, isSupport: false },
        { servant: CHEN_GONG, isSupport: false },
      ],
      [
        makeScene({ attackPriority: [{ id: "atk_0", card: "servant_1_np" }] }),
        makeScene({ id: "scene_2" }),
      ]
    );

    expect(lineups[1].map((member) => member.servant)).toEqual([
      ALTRIA,
      MERLIN,
      WAVER,
      MASH,
      CHEN_GONG,
      null,
    ]);
    expect(lineups[1].map((member) => member.isSupport)).toEqual([
      true,
      false,
      false,
      false,
      false,
      false,
    ]);
  });
});

describe("deriveScenePartyServants", () => {
  it("keeps end-of-turn skill exits out of same-turn preparation lineups", () => {
    const lineup = deriveLineupAfterPreparationActions(
      [HABETROT, MERLIN, WAVER, ALTRIA, MASH, CHEN_GONG],
      [
        {
          type: "servant",
          id: "sa_1",
          servant: "servant_1",
          skill: "skill_3",
          target: null,
        },
      ]
    );

    expect(lineup).toEqual([HABETROT, MERLIN, WAVER, ALTRIA, MASH, CHEN_GONG]);
  });

  it("replaces Arash with the first back-line servant after an NP scene", () => {
    const lineups = deriveScenePartyServants(
      [ARASH, MERLIN, WAVER, ALTRIA, MASH, null],
      [
        makeScene({ attackPriority: [{ id: "atk_0", card: "servant_1_np" }] }),
        makeScene({ id: "scene_2" }),
      ]
    );

    expect(lineups[0]).toEqual([ARASH, MERLIN, WAVER]);
    expect(lineups[1]).toEqual([ALTRIA, MERLIN, WAVER]);
  });

  it("compacts the back line after a servant leaves and a substitute enters", () => {
    const lineups = deriveScenePartyLineups(
      [ARASH, MERLIN, WAVER, ALTRIA, MASH, CHEN_GONG],
      [
        makeScene({ attackPriority: [{ id: "atk_0", card: "servant_1_np" }] }),
        makeScene({ id: "scene_2" }),
      ]
    );

    expect(lineups[1]).toEqual([ALTRIA, MERLIN, WAVER, MASH, CHEN_GONG, null]);
  });

  it("sacrifices Chen Gong's first non-self front-line ally", () => {
    const lineups = deriveScenePartyServants(
      [CHEN_GONG, MERLIN, WAVER, ALTRIA, MASH, null],
      [
        makeScene({ attackPriority: [{ id: "atk_0", card: "servant_1_np" }] }),
        makeScene({ id: "scene_2" }),
      ]
    );

    expect(lineups[0]).toEqual([CHEN_GONG, MERLIN, WAVER]);
    expect(lineups[1]).toEqual([CHEN_GONG, ALTRIA, WAVER]);
  });

  it("keeps current back-line slot mapping after a servant withdraws to back", () => {
    const lineups = deriveScenePartyLineups(
      [CHLOE, MERLIN, WAVER, ALTRIA, MASH, CHEN_GONG],
      [
        makeScene({
          preparationActions: [
            {
              type: "servant",
              id: "sa_1",
              servant: "servant_1",
              skill: "skill_2",
              target: null,
            },
          ],
        }),
        makeScene({
          id: "scene_2",
          preparationActions: [
            {
              type: "equipment",
              id: "eq_1",
              skill: "skill_3",
              target: null,
              orderChange: {
                front: "servant_1",
                back: "servant_4",
              },
            },
          ],
        }),
        makeScene({ id: "scene_3" }),
      ]
    );

    expect(lineups[1]).toEqual([ALTRIA, MERLIN, WAVER, CHLOE, MASH, CHEN_GONG]);
    expect(lineups[2]).toEqual([CHLOE, MERLIN, WAVER, ALTRIA, MASH, CHEN_GONG]);
  });

  it("replaces Habetrot after using her third skill", () => {
    const lineups = deriveScenePartyLineups(
      [HABETROT, MERLIN, WAVER, ALTRIA, MASH, CHEN_GONG],
      [
        makeScene({
          preparationActions: [
            {
              type: "servant",
              id: "sa_1",
              servant: "servant_1",
              skill: "skill_3",
              target: null,
            },
          ],
        }),
        makeScene({ id: "scene_2" }),
      ]
    );

    expect(lineups[0]).toEqual([HABETROT, MERLIN, WAVER, ALTRIA, MASH, CHEN_GONG]);
    expect(lineups[1]).toEqual([ALTRIA, MERLIN, WAVER, MASH, CHEN_GONG, null]);
  });

  it("replaces Ultimate Elisabeth after using her third skill", () => {
    const lineups = deriveScenePartyLineups(
      [MERLIN, ULTIMATE_ELISABETH, WAVER, ALTRIA, MASH, CHEN_GONG],
      [
        makeScene({
          preparationActions: [
            {
              type: "servant",
              id: "sa_1",
              servant: "servant_2",
              skill: "skill_3",
              target: null,
            },
          ],
        }),
        makeScene({ id: "scene_2" }),
      ]
    );

    expect(lineups[0]).toEqual([
      MERLIN,
      ULTIMATE_ELISABETH,
      WAVER,
      ALTRIA,
      MASH,
      CHEN_GONG,
    ]);
    expect(lineups[1]).toEqual([MERLIN, ALTRIA, WAVER, MASH, CHEN_GONG, null]);
  });

  it("derives the full lineup with a pinned support before scene simulation", () => {
    const slots = makeSlots([
      { type: "servant", servant: ARASH },
      { type: "servant", servant: MERLIN },
      { type: "support", servant: null },
      { type: "servant", servant: ALTRIA },
    ]);

    expect(derivePartyLineup(slots, makeProject(MASH.id), SERVANTS)).toEqual([
      ARASH,
      MERLIN,
      MASH,
      ALTRIA,
    ]);
  });

  it("applies configured Order Change to later scene lineups", () => {
    const lineups = deriveScenePartyLineups(
      [ARASH, MERLIN, WAVER, ALTRIA, MASH, CHEN_GONG],
      [
        makeScene({
          preparationActions: [
            {
              type: "equipment",
              id: "eq_1",
              skill: "skill_3",
              target: null,
              orderChange: {
                front: "servant_2",
                back: "servant_5",
              },
            },
          ],
        }),
        makeScene({ id: "scene_2" }),
      ]
    );

    expect(lineups[0]).toEqual([ARASH, MERLIN, WAVER, ALTRIA, MASH, CHEN_GONG]);
    expect(lineups[1]).toEqual([ARASH, MASH, WAVER, ALTRIA, MERLIN, CHEN_GONG]);
  });
});
