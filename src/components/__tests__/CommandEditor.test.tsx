import { describe, it, expect, vi } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../test/renderWithTheme";
import { CommandEditor } from "../CommandEditor";
import type { AdvancedBattleScene, BattleScene } from "../../types/command";
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

  it("saves advanced strategy settings to the advanced scene file", async () => {
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
        partyLineup={[
          makeServant(1, "甲"),
          makeServant(2, "乙"),
          makeServant(3, "丙"),
        ]}
      />
    );

    await screen.findByText("主力输出");
    await user.click(screen.getByRole("button", { name: "设置主力输出" }));
    await user.click(screen.getByRole("button", { name: "甲" }));
    await user.click(screen.getByRole("button", { name: "宝具输出" }));

    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith(
        "save_advanced_battle_scenes",
        expect.objectContaining({
          projectId: "project_1",
          scenes: expect.arrayContaining([
            expect.objectContaining({
              mainOutput: expect.objectContaining({
                servant: "servant_1",
                outputType: "np",
              }),
              commandConditions: expect.any(Array),
              controlActions: expect.any(Array),
              startupActions: expect.any(Array),
              rules: [],
            }),
          ]),
        })
      );
    });
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

    expect(screen.getByRole("button", { name: "丁" })).toBeInTheDocument();
  });
});
