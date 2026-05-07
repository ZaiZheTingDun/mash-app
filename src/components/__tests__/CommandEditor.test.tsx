import { describe, it, expect, vi } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../test/renderWithTheme";
import { CommandEditor } from "../CommandEditor";
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
});
