import { describe, it, expect, vi } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithTheme } from "../../test/renderWithTheme";
import { BattleSceneBlock } from "../BattleSceneBlock";
import type { BattleScene } from "../../types/command";
import type { Servant } from "../../types/servant";

function makeServant(id: number, name_cn: string): Servant {
  return {
    id,
    name_cn,
    name_jp: name_cn,
    name_en: name_cn,
    class: "saber",
    rarity: 5,
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

const PARTY: (Servant | null)[] = [
  makeServant(1, "甲"),
  makeServant(2, "乙"),
  makeServant(3, "丙"),
];

describe("BattleSceneBlock command-spell row", () => {
  it("appends a default commandSpell action when the 令咒 header button is clicked", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    const scene = makeScene();
    renderWithTheme(
      <BattleSceneBlock
        scene={scene}
        index={0}
        partyServants={PARTY}
        onChange={onChange}
        onDelete={vi.fn()}
        canDelete={false}
      />
    );

    await user.click(screen.getByRole("button", { name: /令咒/ }));

    // The header button issues exactly one onChange with the new
    // commandSpellActions list shape (camelCase wire field, type tag
    // 'commandSpell', spell + target null until the user selects them).
    expect(onChange).toHaveBeenCalledTimes(1);
    const next = onChange.mock.calls[0][0] as BattleScene;
    expect(next.commandSpellActions).toHaveLength(1);
    expect(next.commandSpellActions[0]).toMatchObject({
      type: "commandSpell",
      spell: null,
      target: null,
    });
    expect(next.commandSpellActions[0].id).toMatch(/^cs_/);
  });

  it("emits an updated commandSpell action when the spell select changes", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    const scene = makeScene({
      commandSpellActions: [
        { type: "commandSpell", id: "cs_1", spell: null, target: null },
      ],
    });
    renderWithTheme(
      <BattleSceneBlock
        scene={scene}
        index={0}
        partyServants={PARTY}
        onChange={onChange}
        onDelete={vi.fn()}
        canDelete={false}
      />
    );

    // Two `-- 令咒 --` placeholders won't exist, but the spell select
    // is the one whose options include "宝具解放" — find it by the
    // option text and grab its parent select.
    const npOption = screen.getByRole("option", { name: "宝具解放" });
    const spellSelect = npOption.closest("select");
    expect(spellSelect).not.toBeNull();
    await user.selectOptions(spellSelect!, "np_release");

    expect(onChange).toHaveBeenCalledTimes(1);
    const next = onChange.mock.calls[0][0] as BattleScene;
    expect(next.commandSpellActions[0]).toMatchObject({
      type: "commandSpell",
      id: "cs_1",
      spell: "np_release",
      target: null,
    });
  });

  it("emits an updated commandSpell action when the target select changes", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    const scene = makeScene({
      commandSpellActions: [
        { type: "commandSpell", id: "cs_1", spell: "np_release", target: null },
      ],
    });
    renderWithTheme(
      <BattleSceneBlock
        scene={scene}
        index={0}
        partyServants={PARTY}
        onChange={onChange}
        onDelete={vi.fn()}
        canDelete={false}
      />
    );

    // The command-spell row's target select is the one whose options
    // are the party servant names; grab it via the servant_2 option.
    const targetOption = screen.getAllByRole("option", { name: "乙" })[0];
    const targetSelect = targetOption.closest("select");
    expect(targetSelect).not.toBeNull();
    await user.selectOptions(targetSelect!, "servant_2");

    expect(onChange).toHaveBeenCalledTimes(1);
    const next = onChange.mock.calls[0][0] as BattleScene;
    expect(next.commandSpellActions[0]).toMatchObject({
      type: "commandSpell",
      id: "cs_1",
      spell: "np_release",
      target: "servant_2",
    });
  });
});
