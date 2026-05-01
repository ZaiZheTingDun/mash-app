import { describe, it, expect } from "vitest";
import {
  derivePartyLineup,
  derivePartyServants,
  deriveScenePartyServants,
} from "../partyServants";
import { createInitialProjectSlots, type SlotItem } from "../ContentGrid";
import type { BattleScene } from "../../types/command";
import type { Project } from "../../types/project";
import type { Servant } from "../../types/servant";

const MASH: Servant = {
  id: 1,
  name_cn: "玛修",
  name_jp: "マシュ・キリエライト",
  name_en: "Mash Kyrielight",
  class: "Shielder",
  rarity: 4,
};

const ALTRIA: Servant = {
  id: 100,
  name_cn: "阿尔托莉雅",
  name_jp: "アルトリア",
  name_en: "Altria",
  class: "Saber",
  rarity: 5,
};

const MERLIN: Servant = {
  id: 150,
  name_cn: "梅林",
  name_jp: "マーリン",
  name_en: "Merlin",
  class: "Caster",
  rarity: 5,
};

const WAVER: Servant = {
  id: 200,
  name_cn: "韦伯",
  name_jp: "ウェイバー",
  name_en: "Waver",
  class: "Caster",
  rarity: 5,
};

const ARASH: Servant = {
  id: 16,
  name_cn: "阿拉什",
  name_jp: "アーラシュ",
  name_en: "Arash",
  class: "Archer",
  rarity: 1,
};

const CHEN_GONG: Servant = {
  id: 258,
  name_cn: "陈宫",
  name_jp: "陳宮",
  name_en: "Chen Gong",
  class: "Caster",
  rarity: 2,
};

const SERVANTS: Servant[] = [MASH, ALTRIA, MERLIN, WAVER, ARASH, CHEN_GONG];

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

function makeScene(overrides: Partial<BattleScene> = {}): BattleScene {
  return {
    id: "scene_1",
    servantActions: [],
    equipmentActions: [],
    commandSpellActions: [],
    attackPriority: [
      { id: "atk_0", card: null },
      { id: "atk_1", card: null },
      { id: "atk_2", card: null },
    ],
    ...overrides,
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

describe("deriveScenePartyServants", () => {
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
});
