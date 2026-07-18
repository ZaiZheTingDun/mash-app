import { useState, type ComponentProps } from "react";
import { describe, it, expect, vi } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { CommandEditor as ActualCommandEditor } from "../CommandEditor";
import type { AdvancedBattleScene, BattleScene } from "../../../types/command";
import type { GrandCardStrategy, GrandClassDefinition, GrandServantConfig } from "../../../types/project";
import type { Servant } from "../../../types/servant";

const GRAND_CLASS_DEFINITIONS: GrandClassDefinition[] = [
  { id: "saber", label: "剑阶冠位", servantClass: "Saber", roles: [{ role: "main", label: "主", required: true }, { role: "deputy", label: "副", required: false }], cardPriorityEnabled: true, autoOrderChangeRoles: ["main"], validationMessage: "" },
  { id: "lancer", label: "枪阶冠位", servantClass: "Lancer", roles: [{ role: "single", label: "单体", required: true }, { role: "aoe", label: "光炮", required: true }], cardPriorityEnabled: false, autoOrderChangeRoles: ["single", "aoe"], validationMessage: "" },
  { id: "berserker", label: "狂阶冠位", servantClass: "Berserker", roles: [{ role: "main", label: "主", required: true }, { role: "deputy", label: "副", required: false }], cardPriorityEnabled: true, autoOrderChangeRoles: ["main"], validationMessage: "" },
];

function CommandEditor(props: ComponentProps<typeof ActualCommandEditor>) {
  const definition = GRAND_CLASS_DEFINITIONS.find(
    (candidate) => candidate.id === (props.grandClass ?? "saber"),
  );
  return <ActualCommandEditor {...props} grandClassDefinition={definition} />;
}

function makeServant(
  id: number,
  name_cn: string,
  noblePhantasmCard?: Servant["noblePhantasmCard"],
  cls = "saber",
): Servant {
  return {
    id,
    variantKey: String(id),
    name_cn,
    name_jp: name_cn,
    name_en: name_cn,
    class: cls,
    rarity: 5,
    noblePhantasmCard,
  };
}

function makeScene(id: string, skill: string): BattleScene {
  return {
    id,
    turns: [
      {
        id: `${id}_turn_1`,
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
      },
    ],
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

  it("adds, switches, and protects battle turns", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "load_battle_scenes") {
        return [makeScene("scene_1", "skill_1")];
      }
      if (cmd === "get_servant_face_path") {
        return null;
      }
      return [];
    });

    renderWithTheme(
      <CommandEditor
        projectId="project_1"
        partyLineup={[
          makeServant(1, "甲"),
          makeServant(2, "乙"),
          makeServant(3, "丙"),
        ]}
      />
    );

    expect(await screen.findByRole("button", { name: "Turn 1" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "删除当前 Turn" })).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "添加 Turn" }));

    expect(screen.getByRole("button", { name: "Turn 2" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "删除当前 Turn" })).toBeEnabled();
    expect(screen.queryByText("御主礼装 释放 技能 1")).not.toBeInTheDocument();
    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith(
        "save_battle_scenes",
        expect.objectContaining({
          scenes: [
            expect.objectContaining({
              turns: expect.arrayContaining([
                expect.objectContaining({ id: "scene_1_turn_1" }),
                expect.objectContaining({ preparationActions: [] }),
              ]),
            }),
          ],
        })
      );
    });

    await user.click(screen.getByRole("button", { name: "Turn 1" }));
    expect(screen.getByText("御主礼装 释放 技能 1")).toBeInTheDocument();
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

  it("saves and clears one enemy target for the advanced battle", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "load_advanced_battle_scenes") return [];
      if (cmd === "get_servant_face_path") return null;
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

    expect(await screen.findByText("敌方目标选择")).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /敌人/ })).toHaveLength(6);

    await user.click(screen.getByRole("button", { name: "敌人 5" }));
    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith(
        "save_advanced_battle_scenes",
        expect.objectContaining({
          projectId: "project_1",
          scenes: [expect.objectContaining({ enemyTarget: "enemy_5" })],
        })
      );
    });

    await user.click(screen.getByRole("button", { name: "敌人 5" }));
    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenLastCalledWith(
        "save_advanced_battle_scenes",
        expect.objectContaining({
          projectId: "project_1",
          scenes: [expect.objectContaining({ enemyTarget: null })],
        })
      );
    });
  });

  it("restores the saved advanced enemy target", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "load_advanced_battle_scenes") {
        return [{
          id: "advanced_scene_1",
          enemyTarget: "enemy_3",
          rules: [],
        } satisfies AdvancedBattleScene];
      }
      if (cmd === "get_servant_face_path") return null;
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

    expect(await screen.findByRole("button", { name: "敌人 3" })).toHaveClass("selected");
  });

  it("shows advanced startup targets when the selected servant skill targets one ally", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockImplementation(async (cmd: string, args) => {
      if (cmd === "load_advanced_battle_scenes") {
        return [];
      }
      const servantId =
        args && !Array.isArray(args) && typeof args === "object" && "servantId" in args
          ? args.servantId
          : null;
      if (cmd === "get_servant_skill_targeting" && servantId === 1) {
        return [
          {
            servantCollectionNo: 1,
            skillId: 101,
            skillNum: 1,
            funcTargetTypes: ["ptOne"],
          },
        ];
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

    await screen.findByRole("button", { name: "添加启动行动" });
    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith("get_servant_skill_targeting", {
        servantId: 1,
        variantKey: "1",
      });
    });
    await user.click(screen.getByRole("button", { name: "添加启动行动" }));
    const sourceButtons = screen.getAllByRole("button", { name: "甲" });
    await user.click(sourceButtons[sourceButtons.length - 1]);
    await user.click(screen.getByRole("button", { name: "技能 1" }));

    await waitFor(() => {
      expect(screen.queryByRole("button", { name: "无目标" })).not.toBeInTheDocument();
    });
    expect(screen.getAllByRole("button", { name: "甲" }).length).toBeGreaterThan(0);
    expect(screen.getAllByRole("button", { name: "乙" }).length).toBeGreaterThan(0);
    expect(screen.getAllByRole("button", { name: "丙" }).length).toBeGreaterThan(0);
  });

  it("saves advanced startup servant skills without targets when targeting is not required", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "load_advanced_battle_scenes") {
        return [];
      }
      if (cmd === "get_servant_skill_targeting") {
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

    await screen.findByRole("button", { name: "添加启动行动" });
    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith("get_servant_skill_targeting", {
        servantId: 1,
        variantKey: "1",
      });
    });
    await user.click(screen.getByRole("button", { name: "添加启动行动" }));
    const sourceButtons = screen.getAllByRole("button", { name: "甲" });
    await user.click(sourceButtons[sourceButtons.length - 1]);
    await user.click(screen.getByRole("button", { name: "技能 1" }));

    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith(
        "save_advanced_battle_scenes",
        expect.objectContaining({
          projectId: "project_1",
          scenes: [
            expect.objectContaining({
              startupActions: [
                expect.objectContaining({
                  type: "servant",
                  servant: "servant_1",
                  skill: "skill_1",
                  target: null,
                }),
              ],
            }),
          ],
        })
      );
    });
    expect(screen.queryByRole("button", { name: "无目标" })).not.toBeInTheDocument();
  });

  it("keeps showing advanced startup targets when automatic skill target recognition is disabled", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "load_advanced_battle_scenes") {
        return [];
      }
      if (cmd === "get_servant_skill_targeting") {
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
        disableAutoSkillTargetRecognition
        partyLineup={[
          makeServant(1, "甲"),
          makeServant(2, "乙"),
          makeServant(3, "丙"),
        ]}
      />
    );

    await user.click(await screen.findByRole("button", { name: "添加启动行动" }));
    const sourceButtons = screen.getAllByRole("button", { name: "甲" });
    await user.click(sourceButtons[sourceButtons.length - 1]);
    await user.click(screen.getByRole("button", { name: "技能 1" }));

    expect(await screen.findByRole("button", { name: "无目标" })).toBeInTheDocument();
  });

  it("renders advanced equipment actions with a square actor icon", async () => {
    const scene: AdvancedBattleScene = {
      id: "advanced_1",
      startupActions: [
        {
          type: "equipment",
          id: "eq_1",
          skill: "skill_1",
          target: null,
        },
      ],
      rules: [],
    };
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "load_advanced_battle_scenes") {
        return [scene];
      }
      if (cmd === "get_servant_face_path") {
        return null;
      }
      return [];
    });

    const { container } = renderWithTheme(
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

    expect(await screen.findByText("御主礼装 释放 技能 1")).toBeInTheDocument();
    const summary = container.querySelector(".battle-action-summary");
    const icon = summary?.firstElementChild;
    expect(icon).toHaveClass("battle-inline-square");
    expect(icon).toHaveAccessibleName("御主礼装");
    expect(icon?.querySelector(".battle-support-badge")).toBeNull();
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
      { memberId: null, slotIndex: 0, servantId: 1, isSupport: false, npCard: "auto", priority: "damage", role: "main" },
    ]);
  });

  it("allows any class in the advanced grand output picker", async () => {
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
        grandClass="berserker"
        onGrandServantsChange={onGrandServantsChange}
        partyLineup={[
          makeServant(1, "剑阶甲", undefined, "saber"),
          makeServant(2, "狂阶乙", undefined, "berserker"),
          makeServant(3, "术阶丙", undefined, "caster"),
        ]}
      />
    );

    await screen.findByText("主力输出");
    expect(screen.getByRole("button", { name: "剑阶甲" })).not.toBeDisabled();
    expect(screen.getByRole("button", { name: "术阶丙" })).not.toBeDisabled();

    await user.click(screen.getByRole("button", { name: "剑阶甲" }));

    expect(onGrandServantsChange).toHaveBeenCalledWith([
      { memberId: null, slotIndex: 0, servantId: 1, isSupport: false, npCard: "auto", priority: "damage", role: "main" },
    ]);
  });

  it("keeps persisted grand servants that do not match the selected grand class", async () => {
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
        grandClass="berserker"
        grandServants={[{ slotIndex: 0, npCard: "auto", priority: "damage" }]}
        partyLineup={[
          makeServant(1, "剑阶甲", undefined, "saber"),
          makeServant(2, "狂阶乙", undefined, "berserker"),
          makeServant(3, "术阶丙", undefined, "caster"),
        ]}
      />
    );

    await screen.findByText("主力输出");

    expect(screen.getByRole("button", { name: "主冠位：剑阶甲" })).toBeInTheDocument();
  });

  it("adds a second grand output servant without class restriction", async () => {
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
        grandClass="berserker"
        grandServants={[{ slotIndex: 0, npCard: "auto", priority: "damage" }]}
        onGrandServantsChange={onGrandServantsChange}
        partyLineup={[
          makeServant(1, "剑阶甲", undefined, "saber"),
          makeServant(2, "狂阶乙", undefined, "berserker"),
          makeServant(3, "术阶丙", undefined, "caster"),
        ]}
      />
    );

    await screen.findByText("主力输出");
    expect(screen.getByRole("button", { name: "主冠位：剑阶甲" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "狂阶乙" })).not.toBeDisabled();

    await user.click(screen.getByRole("button", { name: "狂阶乙" }));

    expect(onGrandServantsChange).toHaveBeenCalledWith([
      { memberId: null, slotIndex: 0, servantId: null, isSupport: false, npCard: "auto", priority: "damage", role: "main" },
      { memberId: null, slotIndex: 1, servantId: 2, isSupport: false, npCard: "auto", priority: "damage", role: "deputy" },
    ]);
  });

  it("assigns explicit single and aoe roles for lancer grand servants", async () => {
    const user = userEvent.setup();
    const onGrandServantsChange = vi.fn();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "load_advanced_battle_scenes") return [];
      if (cmd === "get_servant_face_path") return null;
      return [];
    });

    function LancerHarness() {
      const [grandServants, setGrandServants] = useState<GrandServantConfig[]>([]);
      return (
        <CommandEditor
          projectId="project_1"
          advancedMode
          grandClass="lancer"
          grandServants={grandServants}
          onGrandServantsChange={(next) => {
            onGrandServantsChange(next);
            setGrandServants(next);
          }}
          partyLineup={[
            makeServant(1, "单体甲", "buster", "lancer"),
            makeServant(2, "光炮乙", "arts", "lancer"),
            makeServant(3, "丙"),
          ]}
        />
      );
    }

    renderWithTheme(<LancerHarness />);
    expect(await screen.findByRole("button", { name: "选择单体冠位" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "选择光炮冠位" })).toBeInTheDocument();
    expect(screen.queryByText("主")).not.toBeInTheDocument();
    expect(screen.queryByText("副")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "单体甲" }));
    expect(screen.getByRole("button", { name: "单体冠位：单体甲" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "光炮乙" }));
    expect(screen.getByRole("button", { name: "光炮冠位：光炮乙" })).toBeInTheDocument();
    expect(onGrandServantsChange).toHaveBeenLastCalledWith([
      expect.objectContaining({ slotIndex: 0, role: "single" }),
      expect.objectContaining({ slotIndex: 1, role: "aoe" }),
    ]);

    await user.click(screen.getByRole("button", { name: "单体冠位：单体甲" }));
    expect(screen.queryByRole("combobox", { name: "出卡策略" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "设为主" })).not.toBeInTheDocument();
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
    await user.click(screen.getByRole("combobox", { name: "宝具颜色" }));

    expect(await screen.findByRole("option", { name: "自动读取（红）" })).toBeInTheDocument();
  });

  it("keeps grand card rules collapsed at the bottom without inline ordering controls", async () => {
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
        grandCardPriorityEnabled
        partyLineup={[
          makeServant(1, "甲"),
          makeServant(2, "乙"),
          makeServant(3, "丙"),
        ]}
      />
    );

    const strategyToggle = await screen.findByRole("button", { name: /指令卡策略/ });
    expect(strategyToggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByText("启动阶段").compareDocumentPosition(strategyToggle)).toBe(
      Node.DOCUMENT_POSITION_FOLLOWING
    );

    await user.click(strategyToggle);
    expect(screen.queryByRole("button", { name: /上移/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /下移/ })).not.toBeInTheDocument();
    expect(screen.queryByText("连携")).not.toBeInTheDocument();
    expect(screen.queryByText("必须包含")).not.toBeInTheDocument();
    expect(screen.queryByText("排除")).not.toBeInTheDocument();
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

    expect(screen.queryByRole("button", { name: /指令卡策略/ })).not.toBeInTheDocument();
  });

  it("adds and edits custom grand card rules", async () => {
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

    function StrategyHarness() {
      const [strategy, setStrategy] = useState<GrandCardStrategy | undefined>();
      return (
        <CommandEditor
          projectId="project_1"
          advancedMode
          grandServants={[{ slotIndex: 0, npCard: "auto", priority: "damage" }]}
          grandCardPriorityEnabled
          grandCardStrategy={strategy}
          onGrandCardStrategyChange={(next) => {
            onGrandCardStrategyChange(next);
            setStrategy(next);
          }}
          partyLineup={[
            makeServant(1, "甲", "buster"),
            makeServant(2, "乙"),
            makeServant(3, "丙"),
          ]}
        />
      );
    }

    renderWithTheme(<StrategyHarness />);

    await user.click(await screen.findByRole("button", { name: /指令卡策略/ }));
    await user.click(screen.getByRole("button", { name: "添加规则" }));

    expect(await screen.findByRole("dialog", { name: "设置策略" })).toBeInTheDocument();
    const grandOption = screen.getByRole("button", { name: "冠位从者" });
    await user.click(grandOption);
    expect(grandOption).toHaveAttribute("aria-pressed", "true");
    await user.click(grandOption);
    expect(grandOption).toHaveAttribute("aria-pressed", "false");
    await user.click(screen.getByRole("button", { name: "甲" }));
    await user.click(screen.getByRole("radio", { name: "宝具" }));
    expect(screen.queryByRole("radio", { name: "红" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "完成" }));

    const lastCall = onGrandCardStrategyChange.mock.calls[
      onGrandCardStrategyChange.mock.calls.length - 1
    ]?.[0] as GrandCardStrategy;
    expect(lastCall.customRules).toHaveLength(1);
    expect(lastCall.customRules?.[0]).toMatchObject({
      slots: [
        expect.objectContaining({
          servantId: 1,
          grandServant: false,
          kind: "np",
          color: "buster",
        }),
        expect.objectContaining({ servantId: null }),
        expect.objectContaining({ servantId: null }),
      ],
    });

    expect(screen.getAllByRole("button", { name: /未选择从者/ })).toHaveLength(2);
    await user.click(screen.getByRole("button", { name: "删除自定义规则" }));
    const deleteCall = onGrandCardStrategyChange.mock.calls[
      onGrandCardStrategyChange.mock.calls.length - 1
    ]?.[0];
    expect(deleteCall).toMatchObject({ customRules: [] });
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

  it("saves advanced startup servant skills with skill selections", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockImplementation(async (cmd, args) => {
      if (cmd === "load_advanced_battle_scenes") return [];
      if (cmd === "get_servant_face_path") return null;
      if (cmd === "get_skill_icon_paths") {
        return [
          { path: null, name: "技能 1" },
          { path: null, name: "技能 2" },
          { path: null, name: "技能 3" },
        ];
      }
      if (cmd === "get_servant_skill_selection") {
        const invokeArgs = args as { servantId?: number } | undefined;
        if (invokeArgs?.servantId !== 1) return [];
        return [
          {
            servantCollectionNo: 1,
            skillId: 101,
            skillNum: 1,
            selectionType: "SelectAddInfo",
            supplementaryTypes: [],
            options: [
              { index: 0, label: "攻击" },
              { index: 1, label: "防御" },
            ],
          },
        ];
      }
      if (cmd === "get_servant_skill_targeting") return [];
      if (cmd === "save_advanced_battle_scenes") return null;
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

    await waitFor(() =>
      expect(vi.mocked(invoke)).toHaveBeenCalledWith("get_servant_skill_selection", {
        servantId: 1,
        variantKey: "1",
      })
    );
    await user.click(await screen.findByRole("button", { name: "添加启动行动" }));
    const sourceButtons = screen.getAllByRole("button", { name: "甲" });
    await user.click(sourceButtons[sourceButtons.length - 1]);
    await user.click(screen.getByRole("button", { name: "技能 1" }));
    await user.click(screen.getByRole("button", { name: "防御" }));

    await waitFor(() =>
      expect(vi.mocked(invoke)).toHaveBeenCalledWith(
        "save_advanced_battle_scenes",
        expect.objectContaining({
          projectId: "project_1",
          scenes: [
            expect.objectContaining({
              startupActions: [
                expect.objectContaining({
                  type: "servant",
                  skill: "skill_1",
                  skillSelection: {
                    type: "SelectAddInfo",
                    index: 1,
                    optionCount: 2,
                    label: "防御",
                  },
                }),
              ],
            }),
          ],
        })
      )
    );
    expect(screen.getByLabelText(/并选择 防御/)).toBeInTheDocument();
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

  it("falls back to numbered skill icons in advanced startup summaries when no icon file exists", async () => {
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
          type: "servant",
          id: "sa_1",
          servant: "servant_1",
          skill: "skill_2",
          target: null,
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
      if (cmd === "get_skill_icon_paths") {
        return [
          { path: null, name: "" },
          { path: null, name: "" },
          { path: null, name: "" },
        ];
      }
      return [];
    });

    const { container } = renderWithTheme(
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

    await screen.findByText("启动阶段");
    const summary = container.querySelector(".battle-action-summary");
    expect(summary).toHaveAccessibleName("甲 技能 2");
    const skillIcon = summary?.querySelector(".battle-inline-skill-icon");
    expect(skillIcon).toHaveAttribute("title", "技能 2");
    expect(skillIcon).toHaveTextContent("2");
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
