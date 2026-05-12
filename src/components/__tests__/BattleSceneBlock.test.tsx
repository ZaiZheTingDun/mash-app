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
    variantKey: String(id),
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
    preparationActions: [],
    servantActions: [],
    equipmentActions: [],
    commandSpellActions: [],
    attackPriority: [],
    ...overrides,
  };
}

const PARTY: (Servant | null)[] = [
  makeServant(1, "甲"),
  makeServant(2, "乙"),
  makeServant(3, "丙"),
  makeServant(4, "丁"),
  makeServant(5, "戊"),
  makeServant(6, "己"),
];

describe("BattleSceneBlock staged action editor", () => {
  it("shows the preparation source picker when the add row is clicked", async () => {
    const user = userEvent.setup();
    renderWithTheme(
      <BattleSceneBlock scene={makeScene()} partyServants={PARTY} onChange={vi.fn()} />
    );

    await user.click(screen.getAllByRole("button", { name: /添加一项新的行动/ })[0]);

    expect(screen.getByRole("button", { name: "甲" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "乙" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "丙" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /御主/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "令咒" })).toBeInTheDocument();
  });

  it("appends an ordered servant preparation action after source, skill, and target are selected", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    renderWithTheme(
      <BattleSceneBlock scene={makeScene()} partyServants={PARTY} onChange={onChange} />
    );

    await user.click(screen.getAllByRole("button", { name: /添加一项新的行动/ })[0]);
    await user.click(screen.getByRole("button", { name: "甲" }));
    await user.click(screen.getByRole("button", { name: "技能 3" }));
    await user.click(screen.getByRole("button", { name: "乙" }));

    expect(onChange).toHaveBeenCalledTimes(1);
    const next = onChange.mock.calls[0][0] as BattleScene;
    expect(next.preparationActions).toHaveLength(1);
    expect(next.preparationActions[0]).toMatchObject({
      type: "servant",
      servant: "servant_1",
      skill: "skill_3",
      target: "servant_2",
    });
    expect(next.servantActions).toEqual([]);
  });

  it("adds an equipment Order Change action from one front slot and one back slot", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    renderWithTheme(
      <BattleSceneBlock scene={makeScene()} partyServants={PARTY} onChange={onChange} />
    );

    await user.click(screen.getAllByRole("button", { name: /添加一项新的行动/ })[0]);
    await user.click(screen.getByRole("button", { name: /御主/ }));
    await user.click(screen.getByRole("button", { name: "技能 2" }));
    await user.click(screen.getByRole("button", { name: "Order Change" }));

    expect(screen.getByRole("button", { name: "丁" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "乙" }));
    await user.click(screen.getByRole("button", { name: "戊" }));

    expect(onChange).toHaveBeenCalledTimes(1);
    const next = onChange.mock.calls[0][0] as BattleScene;
    expect(next.preparationActions[0]).toMatchObject({
      type: "equipment",
      skill: "skill_2",
      target: null,
      orderChange: {
        front: "servant_2",
        back: "servant_5",
      },
    });
  });

  it("does not allow empty Order Change back slots to be selected", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    renderWithTheme(
      <BattleSceneBlock
        scene={makeScene()}
        partyServants={[PARTY[0], PARTY[1], PARTY[2], PARTY[3], null, null]}
        onChange={onChange}
      />
    );

    await user.click(screen.getAllByRole("button", { name: /添加一项新的行动/ })[0]);
    await user.click(screen.getByRole("button", { name: /御主/ }));
    await user.click(screen.getByRole("button", { name: "技能 2" }));
    await user.click(screen.getByRole("button", { name: "Order Change" }));
    await user.click(screen.getByRole("button", { name: "甲" }));

    expect(screen.getByRole("button", { name: "从者 5" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "从者 5" }));

    expect(onChange).not.toHaveBeenCalled();
  });

  it("uses the post-Order Change front line when adding later actions in the same scene", async () => {
    const user = userEvent.setup();
    renderWithTheme(
      <BattleSceneBlock
        scene={makeScene({
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
        })}
        partyServants={PARTY}
        onChange={vi.fn()}
      />
    );

    await user.click(screen.getAllByRole("button", { name: /添加一项新的行动/ })[0]);

    expect(screen.getByRole("button", { name: "丁" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "乙" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "丙" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "甲" })).not.toBeInTheDocument();
  });

  it("uses the post-Order Change front line for attacks in the same scene", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    renderWithTheme(
      <BattleSceneBlock
        scene={makeScene({
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
        })}
        partyServants={PARTY}
        onChange={onChange}
      />
    );

    await user.click(screen.getAllByRole("button", { name: /添加一项新的行动/ })[1]);
    await user.click(screen.getByRole("button", { name: "丁" }));
    await user.click(screen.getByRole("button", { name: "B" }));

    const next = onChange.mock.calls[0][0] as BattleScene;
    expect(next.attackPriority[0]).toMatchObject({
      card: "servant_1_buster",
    });
  });

  it("appends an attack priority row from servant and card choice", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    renderWithTheme(
      <BattleSceneBlock scene={makeScene()} partyServants={PARTY} onChange={onChange} />
    );

    await user.click(screen.getAllByRole("button", { name: /添加一项新的行动/ })[1]);
    await user.click(screen.getByRole("button", { name: "甲" }));
    await user.click(screen.getByRole("button", { name: "B" }));

    expect(onChange).toHaveBeenCalledTimes(1);
    const next = onChange.mock.calls[0][0] as BattleScene;
    expect(next.attackPriority).toHaveLength(1);
    expect(next.attackPriority[0]).toMatchObject({
      card: "servant_1_buster",
    });
  });

  it("removes an existing preparation row through its hover delete button", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    renderWithTheme(
      <BattleSceneBlock
        scene={makeScene({
          preparationActions: [
            {
              type: "equipment",
              id: "eq_1",
              skill: "skill_2",
              target: null,
            },
          ],
        })}
        partyServants={PARTY}
        onChange={onChange}
      />
    );

    await user.click(screen.getByRole("button", { name: "删除行动" }));

    expect(onChange).toHaveBeenCalledTimes(1);
    const next = onChange.mock.calls[0][0] as BattleScene;
    expect(next.preparationActions).toEqual([]);
  });

  it("renders targeted servant actions with the target face after to", () => {
    const { container } = renderWithTheme(
      <BattleSceneBlock
        scene={makeScene({
          preparationActions: [
            {
              type: "servant",
              id: "sa_1",
              servant: "servant_1",
              skill: "skill_1",
              target: "servant_2",
            },
          ],
        })}
        partyServants={PARTY}
        onChange={vi.fn()}
      />
    );

    const summary = container.querySelector(".battle-action-summary");
    const children = Array.from(summary?.children ?? []);
    expect(children[0]).toHaveClass("battle-inline-face");
    expect(children[1]).toHaveTextContent("甲 释放 技能 1");
    expect(children[2]).toHaveClass("battle-action-to");
    expect(children[3]).toHaveClass("battle-inline-face");
    expect(children[4]).toHaveTextContent("乙");
  });

  it("renders Order Change actions with both servant faces", () => {
    const { container } = renderWithTheme(
      <BattleSceneBlock
        scene={makeScene({
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
        })}
        partyServants={PARTY}
        onChange={vi.fn()}
      />
    );

    const summary = container.querySelector(".battle-action-summary");
    const children = Array.from(summary?.children ?? []);
    expect(children[0]).toHaveClass("battle-inline-square");
    expect(children[1]).toHaveTextContent("御主礼装 释放 技能 3");
    expect(children[2]).toHaveTextContent("Order Change");
    expect(children[3]).toHaveClass("battle-inline-face");
    expect(children[4]).toHaveTextContent("甲");
    expect(children[5]).toHaveTextContent("↔");
    expect(children[6]).toHaveClass("battle-inline-face");
    expect(children[7]).toHaveTextContent("丁");
  });

  it("cancels an in-progress preparation action from the left-side delete control", async () => {
    const user = userEvent.setup();
    renderWithTheme(
      <BattleSceneBlock scene={makeScene()} partyServants={PARTY} onChange={vi.fn()} />
    );

    await user.click(screen.getAllByRole("button", { name: /添加一项新的行动/ })[0]);
    await user.click(screen.getByRole("button", { name: "撤销添加行动" }));

    expect(screen.queryByRole("button", { name: "甲" })).not.toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /添加一项新的行动/ })[0]).toBeInTheDocument();
  });
});
