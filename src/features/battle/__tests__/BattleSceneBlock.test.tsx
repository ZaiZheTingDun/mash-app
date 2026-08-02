import { afterEach, describe, it, expect, vi } from "vitest";
import { cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { BattleSceneBlock } from "../BattleSceneBlock";
import type { BattleTurn } from "../../../types/command";
import type { Servant } from "../../../types/servant";
import type { PartyMember } from "../../team/partyServants";

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

function makeScene(overrides: Partial<BattleTurn> = {}): BattleTurn {
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
const ARASH = makeServant(16, "阿拉什");
const HABETROT = makeServant(315, "哈贝特洛特");
const TYPHON = makeServant(441, "堤丰·厄斐墨洛斯");
const MERLIN = makeServant(150, "梅林");
const PHANTASMOON = makeServant(431, "Phantasmoon");
const WAVER = makeServant(37, "诸葛孔明〔埃尔梅罗Ⅱ世〕");

const TYPHON_WAVER_MEMBERS: PartyMember[] = [
  { memberId: "slot-typhon", servant: TYPHON, isSupport: true },
  { memberId: "slot-merlin", servant: MERLIN, isSupport: false },
  { memberId: "slot-phantasmoon", servant: PHANTASMOON, isSupport: false },
  { memberId: "slot-waver", servant: WAVER, isSupport: false },
];

describe("BattleSceneBlock staged action editor", () => {
  afterEach(() => {
    vi.mocked(invoke).mockClear();
  });

  it("shows enemy target selection as unset by default", () => {
    renderWithTheme(
      <BattleSceneBlock scene={makeScene()} partyServants={PARTY} onChange={vi.fn()} />
    );

    expect(screen.getByText("敌方目标选择")).toBeInTheDocument();
    expect(
      screen.getByRole("group", { name: "敌方目标选择" })
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "未选择" })).toBeNull();
    expect(screen.getAllByRole("button", { name: /敌人/ })).toHaveLength(6);
  });

  it("updates and toggles off the enemy target", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    const { rerender } = renderWithTheme(
      <BattleSceneBlock scene={makeScene()} partyServants={PARTY} onChange={onChange} />
    );

    await user.click(screen.getByRole("button", { name: "敌人 5" }));

    expect(onChange).toHaveBeenCalledTimes(1);
    expect((onChange.mock.calls[0][0] as BattleTurn).enemyTarget).toBe(
      "enemy_5"
    );

    rerender(
      <BattleSceneBlock
        scene={makeScene({ enemyTarget: "enemy_5" })}
        partyServants={PARTY}
        onChange={onChange}
      />
    );
    await user.click(screen.getByRole("button", { name: "敌人 5" }));

    expect((onChange.mock.calls[1][0] as BattleTurn).enemyTarget).toBeNull();
  });

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
    expect(screen.getByRole("button", { name: /敌方/ })).toBeInTheDocument();
  });

  it("appends an enemy target preparation action", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    renderWithTheme(
      <BattleSceneBlock scene={makeScene()} partyServants={PARTY} onChange={onChange} />
    );

    await user.click(screen.getAllByRole("button", { name: /添加一项新的行动/ })[0]);
    await user.click(screen.getByRole("button", { name: /敌方/ }));
    await user.click(within(screen.getByRole("group", { name: "选择敌方目标" })).getByRole("button", { name: "敌人 3" }));

    expect(onChange).toHaveBeenCalledTimes(1);
    expect((onChange.mock.calls[0][0] as BattleTurn).preparationActions[0]).toMatchObject({
      type: "enemyTarget",
      target: "enemy_3",
    });
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
    const next = onChange.mock.calls[0][0] as BattleTurn;
    expect(next.preparationActions).toHaveLength(1);
    expect(next.preparationActions[0]).toMatchObject({
      type: "servant",
      servant: "servant_1",
      skill: "skill_3",
      target: "servant_2",
    });
    expect(next.servantActions).toEqual([]);
  });

  it("offers both a servant target and no target for a mixed-targeting skill", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    vi.mocked(invoke).mockImplementation(async (cmd, args) => {
      if (cmd === "get_servant_skill_targeting") {
        const invokeArgs = args as { servantId?: number } | undefined;
        if (invokeArgs?.servantId !== 1) return [];
        return [
          {
            servantCollectionNo: 1,
            skillId: 2477450,
            skillNum: 2,
            funcTargetTypes: ["ptOne", "ptOneOther"],
            targetingMode: "mixed",
          },
        ];
      }
      if (cmd === "get_skill_icon_paths") {
        return [
          { path: null, name: "技能 1" },
          { path: null, name: "技能 2" },
          { path: null, name: "技能 3" },
        ];
      }
      if (cmd === "get_servant_skill_selection") return [];
      return null;
    });
    renderWithTheme(
      <BattleSceneBlock scene={makeScene()} partyServants={PARTY} onChange={onChange} />
    );

    await waitFor(() =>
      expect(vi.mocked(invoke)).toHaveBeenCalledWith("get_servant_skill_targeting", {
        servantId: 1,
        variantKey: "1",
      })
    );
    await user.click(screen.getAllByRole("button", { name: /添加一项新的行动/ })[0]);
    await user.click(screen.getByRole("button", { name: "甲" }));
    await user.click(screen.getByRole("button", { name: "技能 2" }));

    expect(screen.getByRole("button", { name: "无目标" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "乙" })).toBeInTheDocument();
    expect(screen.getByText("该技能存在可选择目标与无需选择目标两种形态")).toHaveClass(
      "battle-targeting-mode-hint"
    );

    await user.click(screen.getByRole("button", { name: "无目标" }));
    expect((onChange.mock.calls[0][0] as BattleTurn).preparationActions[0]).toMatchObject({
      servant: "servant_1",
      skill: "skill_2",
      target: null,
    });
  });

  it("asks for a servant skill selection before saving the action", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    vi.mocked(invoke).mockImplementation(async (cmd, args) => {
      if (cmd === "get_servant_skill_selection") {
        const invokeArgs = args as { servantId?: number } | undefined;
        if (invokeArgs?.servantId !== 1) return [];
        return [
          {
            servantCollectionNo: 1,
            skillId: 101,
            skillNum: 1,
            selectionType: "selectTreasureDeviceInfo",
            supplementaryTypes: ["commandTypeSelfTreasureDevice"],
            options: [
              { index: 0, label: "攻击" },
              { index: 1, label: "防御" },
            ],
          },
        ];
      }
      if (cmd === "get_servant_skill_targeting") return [];
      if (cmd === "get_skill_icon_paths") {
        return [
          { path: null, name: "技能 1" },
          { path: null, name: "技能 2" },
          { path: null, name: "技能 3" },
        ];
      }
      return null;
    });
    renderWithTheme(
      <BattleSceneBlock scene={makeScene()} partyServants={PARTY} onChange={onChange} />
    );

    await waitFor(() =>
      expect(vi.mocked(invoke)).toHaveBeenCalledWith("get_servant_skill_selection", {
        servantId: 1,
        variantKey: "1",
      })
    );
    await user.click(screen.getAllByRole("button", { name: /添加一项新的行动/ })[0]);
    await user.click(screen.getByRole("button", { name: "甲" }));
    await user.click(screen.getByRole("button", { name: "技能 1" }));

    expect(screen.getByRole("button", { name: "攻击" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "防御" }));

    await waitFor(() => expect(onChange).toHaveBeenCalledTimes(1));
    const next = onChange.mock.calls[0][0] as BattleTurn;
    expect(next.preparationActions[0]).toMatchObject({
      type: "servant",
      skill: "skill_1",
      skillSelection: {
        type: "selectTreasureDeviceInfo",
        index: 1,
        optionCount: 2,
        label: "防御",
      },
      target: null,
    });
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
    const next = onChange.mock.calls[0][0] as BattleTurn;
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

  it("marks the support servant avatar in the Order Change picker", async () => {
    const user = userEvent.setup();
    const lineupWithSupportInBackline = [
      PARTY[0],
      PARTY[1],
      PARTY[2],
      PARTY[0],
      PARTY[3],
      PARTY[4],
    ];
    const { container } = renderWithTheme(
      <BattleSceneBlock
        scene={makeScene()}
        partyServants={lineupWithSupportInBackline}
        partyMembers={lineupWithSupportInBackline.map((servant, index) => ({
          servant,
          isSupport: index === 3,
        }))}
        onChange={vi.fn()}
      />
    );

    await user.click(screen.getAllByRole("button", { name: /添加一项新的行动/ })[0]);
    await user.click(screen.getByRole("button", { name: /御主/ }));
    await user.click(screen.getByRole("button", { name: "技能 2" }));
    await user.click(screen.getByRole("button", { name: "Order Change" }));

    const altriaButtons = screen.getAllByRole("button", { name: "甲" });
    expect(altriaButtons).toHaveLength(2);
    expect(altriaButtons[0].querySelector(".battle-support-badge")).toBeNull();
    expect(altriaButtons[1].querySelector(".battle-support-badge")).not.toBeNull();
    expect(container.querySelectorAll(".battle-support-badge")).toHaveLength(1);
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

    await user.click(screen.getAllByRole("button", { name: "未设置攻击" })[0]);
    await user.click(screen.getByRole("button", { name: "丁" }));
    await user.click(screen.getByRole("button", { name: "B" }));

    const next = onChange.mock.calls[0][0] as BattleTurn;
    expect(next.attackPriority).toHaveLength(3);
    expect(next.attackPriority[0]).toMatchObject({
      card: "servant_1_buster",
    });
  });

  it("keeps end-of-turn skill exits available for attacks in the same scene", async () => {
    const user = userEvent.setup();
    renderWithTheme(
      <BattleSceneBlock
        scene={makeScene({
          preparationActions: [
            {
              type: "servant",
              id: "sa_1",
              servant: "servant_1",
              skill: "skill_3",
              target: null,
            },
          ],
        })}
        partyServants={[HABETROT, PARTY[1], PARTY[2], PARTY[3], PARTY[4], PARTY[5]]}
        onChange={vi.fn()}
      />
    );

    await user.click(screen.getAllByRole("button", { name: "未设置攻击" })[0]);

    expect(screen.getByRole("button", { name: "哈贝特洛特" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "乙" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "丙" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "丁" })).not.toBeInTheDocument();
  });

  it("shows three fixed attack chain rows for an empty scene", () => {
    renderWithTheme(
      <BattleSceneBlock scene={makeScene()} partyServants={PARTY} onChange={vi.fn()} />
    );

    expect(screen.getByText("指令卡一")).toBeInTheDocument();
    expect(screen.getByText("指令卡二")).toBeInTheDocument();
    expect(screen.getByText("指令卡三")).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "未设置攻击" })).toHaveLength(3);
    expect(screen.queryAllByRole("button", { name: "清除指令卡" })).toHaveLength(0);
  });

  it("updates a fixed attack chain row from servant and card choice", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    renderWithTheme(
      <BattleSceneBlock scene={makeScene()} partyServants={PARTY} onChange={onChange} />
    );

    await user.click(screen.getAllByRole("button", { name: "未设置攻击" })[0]);
    await user.click(screen.getByRole("button", { name: "甲" }));
    await user.click(screen.getByRole("button", { name: "B" }));

    expect(onChange).toHaveBeenCalledTimes(1);
    const next = onChange.mock.calls[0][0] as BattleTurn;
    expect(next.attackPriority).toHaveLength(3);
    expect(next.attackPriority[0]).toMatchObject({
      card: "servant_1_buster",
    });
    expect(next.attackPriority[1].card).toBeNull();
    expect(next.attackPriority[2].card).toBeNull();
  });

  it("supports setting a fixed attack chain row to any command card", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    renderWithTheme(
      <BattleSceneBlock scene={makeScene()} partyServants={PARTY} onChange={onChange} />
    );

    await user.click(screen.getAllByRole("button", { name: "未设置攻击" })[1]);

    expect(screen.getByRole("button", { name: "甲" })).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /添加一项新的行动/ })).toHaveLength(2);

    await user.click(screen.getByRole("button", { name: "甲" }));
    await user.click(screen.getByRole("button", { name: "ALL" }));

    const next = onChange.mock.calls[0][0] as BattleTurn;
    expect(next.attackPriority[1]).toMatchObject({
      card: "servant_1_all",
    });
  });

  it("uses the post-NP replacement lineup when setting later attack rows", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    renderWithTheme(
      <BattleSceneBlock
        scene={makeScene({
          attackPriority: [
            { id: "atk_1", card: "servant_1_np" },
            { id: "atk_2", card: null },
            { id: "atk_3", card: null },
          ],
        })}
        partyServants={[ARASH, PARTY[1], PARTY[2], PARTY[3], PARTY[4], PARTY[5]]}
        onChange={onChange}
      />
    );

    expect(screen.getByRole("button", { name: "阿拉什 宝具" })).toBeInTheDocument();

    await user.click(screen.getAllByRole("button", { name: "未设置攻击" })[0]);

    expect(screen.getByRole("button", { name: "丁" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "乙" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "丙" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "阿拉什" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "丁" }));
    await user.click(screen.getByRole("button", { name: "B" }));

    const next = onChange.mock.calls[0][0] as BattleTurn;
    expect(next.attackPriority[1]).toMatchObject({
      card: "servant_1_buster",
    });
  });

  it("clears a fixed attack chain row without removing it", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    renderWithTheme(
      <BattleSceneBlock
        scene={makeScene({
          attackPriority: [
            { id: "atk_1", card: "servant_1_buster" },
            { id: "atk_2", card: null },
            { id: "atk_3", card: null },
          ],
        })}
        partyServants={PARTY}
        onChange={onChange}
      />
    );

    await user.click(screen.getAllByRole("button", { name: "清除指令卡" })[0]);

    const next = onChange.mock.calls[0][0] as BattleTurn;
    expect(next.attackPriority).toHaveLength(3);
    expect(next.attackPriority[0].card).toBeNull();
  });

  it("appends and removes fallback attack priority rows after the fixed chain", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    renderWithTheme(
      <BattleSceneBlock
        scene={makeScene({
          attackPriority: [
            { id: "atk_1", card: null },
            { id: "atk_2", card: null },
            { id: "atk_3", card: null },
          ],
        })}
        partyServants={PARTY}
        onChange={onChange}
      />
    );

    await user.click(screen.getAllByRole("button", { name: /添加一项新的行动/ })[1]);
    await user.click(screen.getByRole("button", { name: "甲" }));
    await user.click(screen.getByRole("button", { name: "B" }));

    const appended = onChange.mock.calls[0][0] as BattleTurn;
    expect(appended.attackPriority).toHaveLength(4);
    expect(appended.attackPriority[3]).toMatchObject({
      card: "servant_1_buster",
    });

    onChange.mockClear();
    cleanup();
    renderWithTheme(
      <BattleSceneBlock scene={appended} partyServants={PARTY} onChange={onChange} />
    );
    await user.click(screen.getByRole("button", { name: "删除行动" }));

    const removed = onChange.mock.calls[0][0] as BattleTurn;
    expect(removed.attackPriority).toHaveLength(3);
  });

  it("pads legacy attack priority while preserving fallback rows", () => {
    renderWithTheme(
      <BattleSceneBlock
        scene={makeScene({
          attackPriority: [
            { id: "atk_1", card: "servant_1_np" },
            { id: "atk_2", card: "servant_2_buster" },
            { id: "atk_3", card: "servant_3_arts" },
            { id: "atk_4", card: "servant_1_quick" },
          ],
        })}
        partyServants={PARTY}
        onChange={vi.fn()}
      />
    );

    expect(screen.getByText("指令卡一")).toBeInTheDocument();
    expect(screen.getByText("指令卡二")).toBeInTheDocument();
    expect(screen.getByText("指令卡三")).toBeInTheDocument();
    expect(screen.queryByText("备用 1")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "甲 绿卡攻击" })).toBeInTheDocument();
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
    const next = onChange.mock.calls[0][0] as BattleTurn;
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
    expect(summary).toHaveAccessibleName("甲 释放 技能 1 to 乙");
    expect(children[2]).toHaveClass("battle-action-to");
    expect(children[3]).toHaveClass("battle-inline-face");
    expect(children[4]).toHaveTextContent("乙");
  });

  it("shows localized skill names on servant action summary icons", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_skill_icon_paths") {
        return [
          { path: "/tmp/skill-1.png", name: "缓冲技能 A" },
          { path: null, name: "" },
          { path: null, name: "" },
        ];
      }
      return null;
    });

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
    const skillIcon = summary?.querySelector(".battle-inline-skill-icon");
    expect(summary).toHaveAccessibleName("甲 释放 技能 1 to 乙");
    await waitFor(() => {
      expect(skillIcon).toHaveAttribute("title", "缓冲技能 A");
    });
  });

  it("does not display a back-line member action as the front support fallback", () => {
    const { container } = renderWithTheme(
      <BattleSceneBlock
        scene={makeScene({
          preparationActions: [
            {
              type: "servant",
              id: "sa_waver_backline",
              servant: "servant_1",
              servantMemberId: "slot-waver",
              servantId: WAVER.id,
              servantIsSupport: false,
              skill: "skill_1",
              target: null,
            },
          ],
        })}
        partyServants={TYPHON_WAVER_MEMBERS.map((member) => member.servant)}
        partyMembers={TYPHON_WAVER_MEMBERS}
        onChange={vi.fn()}
      />
    );

    const summary = container.querySelector(".battle-action-summary");
    expect(summary).toHaveAccessibleName("从者 释放 技能 1");
    expect(summary).not.toHaveTextContent("堤丰·厄斐墨洛斯");
    expect(summary).not.toHaveTextContent("诸葛孔明〔埃尔梅罗Ⅱ世〕");
  });

  it("displays Waver member actions after Order Change brings him forward", () => {
    const { container } = renderWithTheme(
      <BattleSceneBlock
        scene={makeScene({
          preparationActions: [
            {
              type: "equipment",
              id: "eq_order_change",
              skill: "skill_3",
              target: null,
              orderChange: {
                front: "servant_2",
                frontMemberId: "slot-merlin",
                frontServantId: MERLIN.id,
                frontIsSupport: false,
                back: "servant_4",
                backMemberId: "slot-waver",
                backServantId: WAVER.id,
                backIsSupport: false,
              },
            },
            {
              type: "servant",
              id: "sa_waver_1",
              servant: "servant_1",
              servantMemberId: "slot-waver",
              servantId: WAVER.id,
              servantIsSupport: false,
              skill: "skill_1",
              target: "servant_1",
              targetMemberId: "slot-typhon",
              targetServantId: TYPHON.id,
              targetIsSupport: true,
            },
            {
              type: "servant",
              id: "sa_waver_2",
              servant: "servant_1",
              servantMemberId: "slot-waver",
              servantId: WAVER.id,
              servantIsSupport: false,
              skill: "skill_2",
              target: null,
            },
            {
              type: "servant",
              id: "sa_waver_3",
              servant: "servant_1",
              servantMemberId: "slot-waver",
              servantId: WAVER.id,
              servantIsSupport: false,
              skill: "skill_3",
              target: null,
            },
          ],
        })}
        partyServants={TYPHON_WAVER_MEMBERS.map((member) => member.servant)}
        partyMembers={TYPHON_WAVER_MEMBERS}
        onChange={vi.fn()}
      />
    );

    const summaries = Array.from(container.querySelectorAll(".battle-action-summary"));
    expect(summaries[1]).toHaveTextContent("堤丰·厄斐墨洛斯");
    expect(summaries[1]).toHaveAccessibleName(
      "诸葛孔明〔埃尔梅罗Ⅱ世〕 释放 技能 1 to 堤丰·厄斐墨洛斯"
    );
    expect(summaries[2]).toHaveAccessibleName("诸葛孔明〔埃尔梅罗Ⅱ世〕 释放 技能 2");
    expect(summaries[3]).toHaveAccessibleName("诸葛孔明〔埃尔梅罗Ⅱ世〕 释放 技能 3");
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

  it("renders command spell actions with a square actor icon", () => {
    const { container } = renderWithTheme(
      <BattleSceneBlock
        scene={makeScene({
          preparationActions: [
            {
              type: "commandSpell",
              id: "cs_1",
              spell: "restore",
              target: null,
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
    expect(children[0]).toHaveAccessibleName("令咒");
    expect(children[0].querySelector(".battle-support-badge")).toBeNull();
    expect(children[1]).toHaveTextContent("令咒 灵基修复");
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
