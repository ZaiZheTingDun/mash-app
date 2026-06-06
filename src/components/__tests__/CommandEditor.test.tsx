import { describe, it, expect, vi } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../test/renderWithTheme";
import { CommandEditor } from "../CommandEditor";
import type { AdvancedBattleScene, BattleScene } from "../../types/command";
import type { Servant } from "../../types/servant";

function makeServant(
  id: number,
  name_cn: string,
  noblePhantasmCard?: Servant["noblePhantasmCard"],
): Servant {
  return {
    id,
    variantKey: String(id),
    name_cn,
    name_jp: name_cn,
    name_en: name_cn,
    class: "saber",
    rarity: 5,
    noblePhantasmCard,
  };
}

function makeScene(id: string, skill: string): BattleScene {
  return {
    id,
    preparationActions: [
      {
        type: "equipment",
        id: `${id}_eq`,
        skill,
        target: null,
      },
    ],
    servantActions: [],
    equipmentActions: [],
    commandSpellActions: [],
    attackPriority: [],
  };
}

describe("CommandEditor pagination", () => {
  it("shows one Battle at a time and keeps the editor body in a scroll region", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "load_battle_scenes") {
        return [makeScene("scene_1", "skill_1"), makeScene("scene_2", "skill_3")];
      }
      return [];
    });

    const { container } = renderWithTheme(
      <CommandEditor
        projectId="project_1"
        partyLineup={[
          makeServant(1, "甲"),
          makeServant(2, "乙"),
          makeServant(3, "丙"),
        ]}
      />
    );

    expect(await screen.findByText("Battle 1 / 2")).toBeInTheDocument();
    expect(screen.getByText("御主礼装 释放 技能 1")).toBeInTheDocument();
    expect(screen.queryByText("御主礼装 释放 技能 3")).not.toBeInTheDocument();
    expect(container.querySelector(".command-scroll-region")).not.toBeNull();

    await user.click(screen.getByRole("button", { name: "下一场战斗" }));

    expect(screen.getByText("Battle 2 / 2")).toBeInTheDocument();
    expect(screen.getByText("御主礼装 释放 技能 3")).toBeInTheDocument();
    expect(screen.queryByText("御主礼装 释放 技能 1")).not.toBeInTheDocument();
  });

  it("uses the advanced scene commands in advanced mode", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "load_advanced_battle_scenes") {
        return [];
      }
      if (cmd === "get_servant_face_path") {
        return null;
      }
      return [];
    });

    renderWithTheme(
      <CommandEditor
        projectId="project_1"
        advancedMode
        partyLineup={[
          makeServant(1, "甲"),
          makeServant(2, "乙"),
          makeServant(3, "丙"),
        ]}
      />
    );

    expect(await screen.findByText("主力输出")).toBeInTheDocument();
    expect(screen.getByText("启动条件")).toBeInTheDocument();
    expect(screen.getByText("控制栏")).toBeInTheDocument();
    expect(screen.getByText("启动阶段")).toBeInTheDocument();
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("load_advanced_battle_scenes", {
      projectId: "project_1",
    });
    expect(vi.mocked(invoke)).not.toHaveBeenCalledWith("load_battle_scenes", {
      projectId: "project_1",
    });
  });

  it("configures grand servants from the advanced main output section", async () => {
    const user = userEvent.setup();
    const onGrandServantsChange = vi.fn();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "load_advanced_battle_scenes") {
        return [];
      }
      if (cmd === "get_servant_face_path") {
        return null;
      }
      return [];
    });

    renderWithTheme(
      <CommandEditor
        projectId="project_1"
        advancedMode
        onGrandServantsChange={onGrandServantsChange}
        partyLineup={[
          makeServant(1, "甲"),
          makeServant(2, "乙"),
          makeServant(3, "丙"),
        ]}
      />
    );

    await screen.findByText("主力输出");
    await user.click(screen.getByRole("button", { name: "甲" }));

    expect(onGrandServantsChange).toHaveBeenCalledWith([
      { slotIndex: 0, npCard: "auto", priority: "damage" },
    ]);
  });

  it("marks the support servant avatar in advanced command settings", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "load_advanced_battle_scenes") {
        return [];
      }
      if (cmd === "get_servant_face_path") {
        return null;
      }
      return [];
    });

    const altria = makeServant(2, "乙");
    const partyLineup = [
      makeServant(1, "甲"),
      altria,
      makeServant(3, "丙"),
      altria,
      makeServant(5, "戊"),
      makeServant(6, "己"),
    ];
    const { container } = renderWithTheme(
      <CommandEditor
        projectId="project_1"
        advancedMode
        partyLineup={partyLineup}
        partyMembers={partyLineup.map((servant, index) => ({
          servant,
          isSupport: index === 3,
        }))}
      />
    );

    await screen.findByText("主力输出");

    const altriaButtons = screen.getAllByRole("button", { name: "乙" });
    expect(altriaButtons).toHaveLength(2);
    expect(altriaButtons[0].querySelector(".battle-support-badge")).toBeNull();
    expect(altriaButtons[1].querySelector(".battle-support-badge")).not.toBeNull();
    expect(container.querySelectorAll(".battle-support-badge")).toHaveLength(1);
  });

  it("shows inferred NP color for automatic grand servant settings", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "load_advanced_battle_scenes") {
        return [];
      }
      if (cmd === "get_servant_face_path") {
        return null;
      }
      return [];
    });

    renderWithTheme(
      <CommandEditor
        projectId="project_1"
        advancedMode
        grandServants={[{ slotIndex: 0, npCard: "auto", priority: "damage" }]}
        partyLineup={[
          makeServant(1, "甲", "buster"),
          makeServant(2, "乙"),
          makeServant(3, "丙"),
        ]}
      />
    );

    expect(await screen.findByText("自动红")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "主冠位：甲" }));

    expect(screen.getByRole("option", { name: "自动读取（红）" })).toBeInTheDocument();
  });

  it("keeps grand card strategy collapsed at the bottom and saves reordered priority", async () => {
    const user = userEvent.setup();
    const onGrandCardStrategyChange = vi.fn();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "load_advanced_battle_scenes") {
        return [];
      }
      if (cmd === "get_servant_face_path") {
        return null;
      }
      return [];
    });

    renderWithTheme(
      <CommandEditor
        projectId="project_1"
        advancedMode
        grandServants={[{ slotIndex: 0, npCard: "auto", priority: "damage" }]}
        grandCardPriorityEnabled
        onGrandCardStrategyChange={onGrandCardStrategyChange}
        partyLineup={[
          makeServant(1, "甲"),
          makeServant(2, "乙"),
          makeServant(3, "丙"),
        ]}
      />
    );

    const strategyToggle = await screen.findByRole("button", { name: /冠位出牌优先级/ });
    expect(strategyToggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByText("启动阶段").compareDocumentPosition(strategyToggle)).toBe(
      Node.DOCUMENT_POSITION_FOLLOWING
    );

    await user.click(strategyToggle);
    await user.click(screen.getByRole("button", { name: "上移主冠位宝具已就绪" }));

    expect(onGrandCardStrategyChange).toHaveBeenCalledWith({
      chainPriority: [
        "mainReadyNp",
        "mainBraveChain",
        "deputyBraveChain",
        "mainColorChain",
        "deputyColorChain",
        "fallback",
      ],
    });
  });

  it("hides grand card strategy when the feature toggle is disabled", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "load_advanced_battle_scenes") {
        return [];
      }
      if (cmd === "get_servant_face_path") {
        return null;
      }
      return [];
    });

    renderWithTheme(
      <CommandEditor
        projectId="project_1"
        advancedMode
        grandServants={[{ slotIndex: 0, npCard: "auto", priority: "damage" }]}
        grandCardPriorityEnabled={false}
        partyLineup={[
          makeServant(1, "甲"),
          makeServant(2, "乙"),
          makeServant(3, "丙"),
        ]}
      />
    );

    await screen.findByText("启动阶段");

    expect(screen.queryByRole("button", { name: /冠位出牌优先级/ })).not.toBeInTheDocument();
  });

  it("uses post-Order Change lineup in advanced startup actions", async () => {
    const user = userEvent.setup();
    const advancedScene: AdvancedBattleScene = {
      id: "advanced_scene_1",
      mainOutput: { servant: null, outputType: null },
      commandConditions: [
        { slot: 0, servant: "any", suit: "any", minCritChance: null },
        { slot: 1, servant: "any", suit: "any", minCritChance: null },
        { slot: 2, servant: "any", suit: "any", minCritChance: null },
        { slot: 3, servant: "any", suit: "any", minCritChance: null },
        { slot: 4, servant: "any", suit: "any", minCritChance: null },
      ],
      startupActions: [
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
      rules: [],
    };
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "load_advanced_battle_scenes") {
        return [advancedScene];
      }
      if (cmd === "get_servant_face_path") {
        return null;
      }
      return [];
    });

    renderWithTheme(
      <CommandEditor
        projectId="project_1"
        advancedMode
        partyLineup={[
          makeServant(1, "甲"),
          makeServant(2, "乙"),
          makeServant(3, "丙"),
          makeServant(4, "丁"),
          makeServant(5, "戊"),
          makeServant(6, "己"),
        ]}
      />
    );

    expect(await screen.findByText("Order Change")).toBeInTheDocument();
    expect(screen.getByText("甲")).toBeInTheDocument();
    expect(screen.getByText("丁")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "添加启动行动" }));

    expect(screen.getAllByRole("button", { name: "丁" }).length).toBeGreaterThan(0);
  });

  it("shows startup action targets for backline grand servants", async () => {
    const advancedScene: AdvancedBattleScene = {
      id: "advanced_scene_1",
      mainOutput: { servant: null, outputType: null },
      grandAutoOrderChange: true,
      commandConditions: [
        { slot: 0, servant: "any", suit: "any", minCritChance: null },
        { slot: 1, servant: "any", suit: "any", minCritChance: null },
        { slot: 2, servant: "any", suit: "any", minCritChance: null },
        { slot: 3, servant: "any", suit: "any", minCritChance: null },
        { slot: 4, servant: "any", suit: "any", minCritChance: null },
      ],
      startupActions: [
        {
          type: "equipment",
          id: "eq_1",
          skill: "skill_1",
          target: "servant_4",
          orderChange: null,
        },
      ],
      rules: [],
    };
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "load_advanced_battle_scenes") {
        return [advancedScene];
      }
      if (cmd === "get_servant_face_path") {
        return null;
      }
      return [];
    });

    renderWithTheme(
      <CommandEditor
        projectId="project_1"
        advancedMode
        grandServants={[{ slotIndex: 3, npCard: "auto", priority: "damage" }]}
        partyLineup={[
          makeServant(1, "甲"),
          makeServant(2, "乙"),
          makeServant(3, "丙"),
          makeServant(4, "丁"),
          makeServant(5, "戊"),
          makeServant(6, "己"),
        ]}
      />
    );

    expect(await screen.findByText("御主礼装 释放 技能 1")).toBeInTheDocument();
    expect(screen.getByText("to")).toBeInTheDocument();
    expect(screen.getAllByText("丁").length).toBeGreaterThan(0);
  });

  it("shows grand auto order change choice inside startup conditions", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "load_advanced_battle_scenes") {
        return [];
      }
      if (cmd === "get_servant_face_path") {
        return null;
      }
      return [];
    });

    renderWithTheme(
      <CommandEditor
        projectId="project_1"
        advancedMode
        grandServants={[{ slotIndex: 4, npCard: "auto", priority: "damage" }]}
        partyLineup={[
          makeServant(1, "甲"),
          makeServant(2, "乙"),
          makeServant(3, "丙"),
          makeServant(4, "丁"),
          makeServant(5, "戊"),
          makeServant(6, "己"),
        ]}
      />
    );

    expect(
      await screen.findByText("主冠位从者配置在后排，是否自动换位至前排？")
    ).toBeInTheDocument();
    expect(screen.getByText("控制栏")).toBeInTheDocument();
    expect(screen.getByText("启动阶段")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "是" }));

    expect(
      screen.getByText(/第一回合会自动将 戊 和前排指令卡最多的从者交换。/)
    ).toBeInTheDocument();
    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith(
        "save_advanced_battle_scenes",
        expect.objectContaining({
          scenes: expect.arrayContaining([
            expect.objectContaining({ grandAutoOrderChange: true }),
          ]),
        })
      );
    });
  });

  it("keeps manual startup card conditions when grand auto order change is declined", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "load_advanced_battle_scenes") {
        return [];
      }
      if (cmd === "get_servant_face_path") {
        return null;
      }
      return [];
    });

    renderWithTheme(
      <CommandEditor
        projectId="project_1"
        advancedMode
        grandServants={[{ slotIndex: 3, npCard: "auto", priority: "damage" }]}
        partyLineup={[
          makeServant(1, "甲"),
          makeServant(2, "乙"),
          makeServant(3, "丙"),
          makeServant(4, "丁"),
          makeServant(5, "戊"),
          makeServant(6, "己"),
        ]}
      />
    );

    await user.click(await screen.findByRole("button", { name: "否" }));

    expect(screen.getByRole("button", { name: "设置指令卡 1，ANYANY" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "改为自动换位" })).toBeInTheDocument();
    expect(screen.getByText("控制栏")).toBeInTheDocument();
    expect(screen.getByText("启动阶段")).toBeInTheDocument();
    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith(
        "save_advanced_battle_scenes",
        expect.objectContaining({
          scenes: expect.arrayContaining([
            expect.objectContaining({ grandAutoOrderChange: false }),
          ]),
        })
      );
    });

    await user.click(screen.getByRole("button", { name: "改为自动换位" }));

    expect(
      screen.getByText(/第一回合会自动将 丁 和前排指令卡最多的从者交换。/)
    ).toBeInTheDocument();
    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith(
        "save_advanced_battle_scenes",
        expect.objectContaining({
          scenes: expect.arrayContaining([
            expect.objectContaining({ grandAutoOrderChange: true }),
          ]),
        })
      );
    });
  });
});
