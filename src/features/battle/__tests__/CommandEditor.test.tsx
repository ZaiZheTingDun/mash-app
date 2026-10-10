import { commandEditorDevBridge } from "../../../commandEditorDevMock";
import { useState, type ComponentProps } from "react";
import { describe, it, expect, vi } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { CommandEditor as ActualCommandEditor } from "../CommandEditor";
import type { AdvancedBattleScene, BattleScene, PreparationAction } from "../../../types/command";
import type { MysticCode } from "../../../types/mysticCode";
import type { GrandCardStrategy, GrandClassDefinition, GrandServantConfig } from "../../../types/project";
import type { Servant } from "../../../types/servant";
import maleCommandSpellIcon from "../../../../src-tauri/resources/images/commandspell/commandspell_male.png";
import femaleCommandSpellIcon from "../../../../src-tauri/resources/images/commandspell/commandspell_female.png";

function mockInvoke(handler: Parameters<typeof commandEditorDevBridge>[0]) {
  const adapted = commandEditorDevBridge((command,args) => invoke(command,args));
  vi.mocked(invoke).mockImplementation(((command: string,args: Record<string, unknown>) => command === "load_command_editor" || command === "mutate_command_editor" ? adapted(command,args) : handler(command,args)) as typeof invoke);
}

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
  it.each([false, true])("renders skill names, spell skills and the correct master face in aligned action columns in advanced mode %s", async advancedMode => {
    const imageSpy = vi.spyOn(window, "Image").mockImplementation(function () {
      const image = document.createElement("img");
      Object.defineProperties(image, { complete: { value: true }, naturalWidth: { value: 100 } });
      return image;
    });
    try {
      const code = (id: number): MysticCode => ({
        id, name: `礼装 ${id}`, itemMalePath: null, itemFemalePath: null,
        masterFigureMalePath: null, masterFigureFemalePath: null,
        masterFaceMalePath: `/codes/${id}/master-face-male.png`,
        masterFaceFemalePath: `/codes/${id}/master-face-female.png`,
        skills: [{ id: 1, slot: 1, name: "御主礼装技能", iconPath: "/icons/master.png", targetingMode: "needsTarget" }],
      });
      const actions: PreparationAction[] = [
        { id: "servant", type: "servant", servant: "servant_1", skill: "skill_1", target: "servant_1" },
        { id: "equipment", type: "equipment", skill: "skill_1", target: "servant_1" },
        { id: "enemy", type: "enemyTarget", target: "enemy_3" },
        { id: "np", type: "commandSpell", spell: "np_release", target: "servant_1" },
        { id: "restore", type: "commandSpell", spell: "restore", target: "servant_1" },
      ];
      const scene = makeScene("scene", "skill_1");
      scene.turns[0].preparationActions = actions;
      const longSkillName = "这是一个需要固定宽度并截断的非常长的技能名称 A+++";
      mockInvoke(async command => {
        if (command === "load_battle_scenes") return [scene];
        if (command === "load_advanced_battle_scenes") return [{ id: "advanced_scene", startupActions: [], turns: [{ id: "advanced_turn", actions }], rules: [] }];
        if (command === "get_skill_icon_paths") return [{ path: "/icons/servant.png", name: longSkillName }, { path: null, name: "" }, { path: null, name: "" }];
        if (command === "get_command_spell_icon_paths") return [{ path: "/icons/skill_00601.png", name: "宝具解放" }, { path: "/icons/skill_00600.png", name: "灵基修复" }];
        return [];
      });
      const props = { projectId: "project_1", advancedMode, partyLineup: [makeServant(1, "甲")], homeMasterCode: code(470) };
      const view = renderWithTheme(<CommandEditor {...props} mysticCode={code(20)} mysticCodeGender="female" />);
      await waitFor(() => expect(view.container.querySelector(".command-row-skill-name")).toHaveTextContent(longSkillName));
      const rows = view.container.querySelectorAll(".battle-action-row.committed");
      expect(rows).toHaveLength(5);
      for (const row of rows) {
        expect(Array.from(row.querySelector(".battle-action-summary")!.children).map(child => child.className)).toEqual(["command-row-source", "command-row-skill", "command-row-outcome"]);
      }
      expect(rows[1].querySelector(".command-row-skill-name")).toHaveTextContent("御主礼装技能");
      expect(rows[2].querySelector(".command-row-source")).toHaveTextContent("御主");
      expect(rows[2].querySelector(".command-row-skill")).toHaveTextContent("选择敌方目标");
      expect(rows[2].querySelector(".command-row-outcome")).toHaveTextContent("03");
      for (const [index, name, filename] of [[3, "宝具解放", "skill_00601"], [4, "灵基修复", "skill_00600"]] as const) {
        expect(rows[index].querySelector(".command-row-skill")).toHaveTextContent(`释放${name}`);
        expect(rows[index].querySelector(".battle-inline-skill-icon img")).toHaveAttribute("src", `asset:///icons/${filename}.png`);
        expect(rows[index].querySelector(".command-row-outcome")).toHaveTextContent("给甲");
      }
      for (const gender of ["female", "male"] as const) {
        for (const selected of [true, false]) {
          view.rerender(<CommandEditor {...props} mysticCode={selected ? code(20) : null} mysticCodeGender={gender} />);
          await waitFor(() => expect(rows[2].querySelector(".command-row-source img")).toHaveAttribute("src", `asset:///codes/${selected ? 20 : 470}/master-face-${gender}.png`));
        }
      }
    } finally {
      imageSpy.mockRestore();
    }
  });

  it.each([false, true])("keeps a diamond entry and uses the configured master gender for command-spell source and summary in advanced mode %s", async advancedMode => {
    const action = { id: "spell", type: "commandSpell" as const, spell: "np_release" as const, target: null };
    const scene = makeScene("scene", "skill_1");
    scene.turns[0].preparationActions = [action];
    const advancedScene: AdvancedBattleScene = {
      id: "advanced_scene", startupActions: [],
      turns: [{ id: "advanced_turn", actions: [action] }], rules: [],
    };
    mockInvoke(async command => {
      if (command === "load_battle_scenes") return [scene];
      if (command === "load_advanced_battle_scenes") return [advancedScene];
      return [];
    });
    const props = { projectId: "project_1", advancedMode, partyLineup: [] };
    const view = renderWithTheme(<CommandEditor {...props} mysticCodeGender="female" />);
    const entry = await screen.findByRole("button", { name: "使用令咒" });
    expect(entry.querySelector(".command-diamond")).toBeInTheDocument();
    expect(entry.querySelector("img")).not.toBeInTheDocument();
    expect(document.querySelector(".command-row-source .command-spell-icon")).toHaveAttribute("src", femaleCommandSpellIcon);
    view.rerender(<CommandEditor {...props} mysticCodeGender="male" />);
    expect(entry.querySelector(".command-diamond")).toBeInTheDocument();
    expect(entry.querySelector("img")).not.toBeInTheDocument();
    expect(document.querySelector(".command-row-source .command-spell-icon")).toHaveAttribute("src", maleCommandSpellIcon);
    await userEvent.setup().click(entry);
    const source = screen.getByRole("button", { name: "令咒" });
    expect(source).toHaveAttribute("aria-pressed", "true");
    expect(within(source).getByRole("img", { name: "令咒" })).toHaveAttribute("src", maleCommandSpellIcon);
  });

  it.each([false, true])("inserts an enemy target action in preparation without changing steps in advanced mode %s", async advancedMode => {
    const user = userEvent.setup();
    const scene = makeScene("scene", "skill_1");
    const initialAction = scene.turns[0].preparationActions![0];
    scene.turns[0].enemyTarget = "enemy_5";
    const advancedScene: AdvancedBattleScene = {
      id: "advanced_scene",
      enemyTarget: "enemy_5",
      startupActions: [],
      turns: [{ id: "advanced_turn", actions: [initialAction] }],
      rules: [],
    };
    mockInvoke(async command => {
      if (command === "load_battle_scenes") return [scene];
      if (command === "load_advanced_battle_scenes") return [advancedScene];
      return [];
    });
    renderWithTheme(<CommandEditor projectId="project_1" advancedMode={advancedMode} partyLineup={[]} />);
    await user.click(await screen.findByRole("button", { name: "选择敌方目标" }));
    const preparation = screen.getByRole("button", { name: /^01 准备阶段/ });
    expect(preparation).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: /^02 敌方目标/ })).toHaveAttribute("aria-pressed", "false");
    await user.click(within(screen.getByRole("group", { name: "选择敌方目标" })).getByRole("button", { name: "敌人 3" }));
    const actions = [initialAction, expect.objectContaining({ type: "enemyTarget", target: "enemy_3" })];
    await waitFor(() => expect(vi.mocked(invoke)).toHaveBeenCalledWith(
      advancedMode ? "save_advanced_battle_scenes" : "save_battle_scenes",
      expect.objectContaining({
        scenes: [expect.objectContaining(advancedMode
          ? { enemyTarget: "enemy_5", turns: [expect.objectContaining({ actions })] }
          : { turns: [expect.objectContaining({ enemyTarget: "enemy_5", preparationActions: actions })] })],
      })
    ));
    expect(preparation).toHaveAttribute("aria-pressed", "true");
  });

  it.each([false, true])("keeps the selected skill source visible and allows reselecting it in advanced mode %s", async advancedMode => {
    const user = userEvent.setup();
    mockInvoke(async () => []);
    renderWithTheme(<CommandEditor projectId="project_1" advancedMode={advancedMode}
      partyLineup={[makeServant(1, "甲"), makeServant(2, "乙"), makeServant(3, "丙")]} />);
    await user.click(await screen.findByRole("button", { name: "添加技能指令" }));
    await user.click(screen.getByRole("button", { name: "甲" }));
    expect(screen.getByRole("button", { name: "甲" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "技能 2" })).toHaveTextContent("技能 2");
    await user.click(screen.getByRole("button", { name: "重选" }));
    expect(screen.queryByRole("button", { name: "技能 2" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "乙" }));
    expect(screen.getByRole("button", { name: "乙" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "技能 2" })).toHaveTextContent("技能 2");
  });

  it("keeps the saved document visible when a backend mutation fails", async () => {
    vi.mocked(invoke).mockImplementation(async command => {
      if(command === "load_command_editor") return {scenes:[makeScene("stable","skill_1")],wave:0,turn:0,canUndo:false};
      if(command === "mutate_command_editor") throw new Error("无法写入配置");
      return [];
    });
    renderWithTheme(<CommandEditor projectId="project_1" partyLineup={[]} />);
    await userEvent.click(await screen.findByRole("button",{name:"添加 Turn"}));
    expect(await screen.findByRole("alert")).toHaveTextContent("无法写入配置");
    expect(screen.getByRole("group", { name: "第 1/1 回合" })).toHaveTextContent("01/ 01");
    expect(screen.getByRole("button",{name:"添加 Turn"})).toBeEnabled();
  });

  it("confirms deleting a configured turn and restores it with undo", async () => {
    const scene=makeScene("scene","skill_1");scene.turns.push({...scene.turns[0],id:"second",preparationActions:[]});
    mockInvoke(async command => command === "load_battle_scenes" ? [scene] : []);
    renderWithTheme(<CommandEditor projectId="project_1" partyLineup={[]} />);
    await userEvent.click(await screen.findByRole("button",{name:"删除当前 Turn"}));
    await userEvent.click(screen.getByRole("button",{name:"取消"}));
    expect(screen.getByRole("group", { name: "第 1/2 回合" })).toHaveTextContent("01/ 02");
    await userEvent.click(screen.getByRole("button",{name:"删除当前 Turn"}));
    await userEvent.click(screen.getByRole("button",{name:"确认删除"}));
    expect(await screen.findByRole("group", { name: "第 1/1 回合" })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button",{name:"撤销上次修改"}));
    expect(await screen.findByRole("group", { name: "第 1/2 回合" })).toBeInTheDocument();
    expect(screen.getByLabelText(/^御主礼装 (?:释放 )?技能 1(?: |$)/)).toBeInTheDocument();
  });

  it("reports a failed Grand save without replacing the saved role", async () => {
    const onGrandServantsChange=vi.fn().mockRejectedValue(new Error("保存冠位失败"));
    mockInvoke(async()=>[]);
    renderWithTheme(<CommandEditor projectId="project_1" advancedMode partyLineup={[makeServant(1,"甲")]} onGrandServantsChange={onGrandServantsChange} />);
    await userEvent.click(await screen.findByRole("button",{name:"选择主冠位"}));
    await userEvent.click(screen.getByRole("button",{name:"甲"}));
    await userEvent.click(screen.getByRole("button",{name:"确认更换"}));
    expect(await screen.findByRole("alert")).toHaveTextContent("保存冠位失败");
    expect(screen.getByRole("button",{name:"选择主冠位"})).toBeEnabled();
  });

  it("cancels Grand replacement without writing and disables unconfigured candidates", async () => {
    const onGrandServantsChange=vi.fn();
    mockInvoke(async()=>[]);
    renderWithTheme(<CommandEditor projectId="project_1" advancedMode partyLineup={[makeServant(1,"甲")]} onGrandServantsChange={onGrandServantsChange} />);
    await userEvent.click(await screen.findByRole("button",{name:"选择主冠位"}));
    expect(screen.getByRole("button",{name:"位置 6 未配置从者"})).toBeDisabled();
    expect(screen.getByRole("button",{name:"确认更换"})).toBeDisabled();
    await userEvent.click(screen.getByRole("button",{name:"甲"}));
    await userEvent.click(screen.getByRole("button",{name:"取消"}));
    expect(onGrandServantsChange).not.toHaveBeenCalled();
  });

  it("shows one Battle at a time and keeps the editor body in a scroll region", async () => {
    const user = userEvent.setup();
    mockInvoke(async (cmd: string) => {
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

    expect(await screen.findByLabelText("第 1/2 面")).toBeInTheDocument();
    expect(screen.getByLabelText(/^御主礼装 (?:释放 )?技能 1(?: |$)/)).toBeInTheDocument();
    expect(screen.queryByLabelText(/^御主礼装 (?:释放 )?技能 3(?: |$)/)).not.toBeInTheDocument();
    expect(container.querySelector(".command-scroll-region")).not.toBeNull();

    await user.click(screen.getByRole("button", { name: "下一场战斗" }));

    expect(screen.getByLabelText("第 2/2 面")).toBeInTheDocument();
    expect(screen.getByLabelText(/^御主礼装 (?:释放 )?技能 3(?: |$)/)).toBeInTheDocument();
    expect(screen.queryByLabelText(/^御主礼装 (?:释放 )?技能 1(?: |$)/)).not.toBeInTheDocument();
  });

  it("adds, switches, and protects battle turns", async () => {
    const user = userEvent.setup();
    mockInvoke(async (cmd: string) => {
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

    expect(await screen.findByRole("group", { name: "第 1/1 回合" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Turn 帮助" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "删除当前 Turn" })).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "添加 Turn" }));

    expect(screen.getByRole("group", { name: "第 2/2 回合" })).toHaveTextContent("02/ 02");
    expect(screen.getByRole("button", { name: "删除当前 Turn" })).toBeEnabled();
    expect(screen.queryByLabelText(/^御主礼装 (?:释放 )?技能 1(?: |$)/)).not.toBeInTheDocument();
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

    await user.click(screen.getByRole("button", { name: "上一回合" }));
    expect(screen.getByLabelText(/^御主礼装 (?:释放 )?技能 1(?: |$)/)).toBeInTheDocument();
  });

  it("uses the advanced scene commands in advanced mode", async () => {
    mockInvoke(async (cmd: string) => {
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

    expect(await screen.findByText("冠位配置")).toBeInTheDocument();
    expect(screen.getByText("启动条件")).toBeInTheDocument();
    expect(screen.getByRole("button", {name:/控制行动/})).toBeInTheDocument();
    expect(screen.getByRole("button", {name: "添加技能指令"})).toBeInTheDocument();
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("load_advanced_battle_scenes", {
      projectId: "project_1",
    });
    expect(vi.mocked(invoke)).not.toHaveBeenCalledWith("load_battle_scenes", {
      projectId: "project_1",
    });
  });

  it("adds and switches Grand Battle turns beside the action control", async () => {
    const user = userEvent.setup();
    mockInvoke(async (cmd: string) => {
      if (cmd === "load_advanced_battle_scenes") {
        return [{
          id: "advanced_scene_1",
          startupActions: [{
            type: "equipment",
            id: "legacy_action",
            skill: "skill_1",
            target: null,
          }],
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

    expect(await screen.findByRole("button", {name: "添加技能指令"})).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "添加技能指令" })).toBeInTheDocument();
    const firstTurn = screen.getByRole("group", { name: "第 1/1 回合" });
    expect(firstTurn).toBeInTheDocument();
    expect(
      firstTurn.compareDocumentPosition(screen.getByRole("button", { name: "添加技能指令" }))
    ).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
    expect(screen.getByRole("button", { name: "删除当前 Turn" })).toBeDisabled();
    expect(screen.getByLabelText(/^御主礼装 (?:释放 )?技能 1(?: |$)/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "添加 Turn" }));

    expect(screen.getByRole("group", { name: "第 2/2 回合" })).toHaveTextContent("02/ 02");
    expect(screen.getByRole("button", { name: "删除当前 Turn" })).toBeEnabled();
    expect(screen.queryByLabelText(/^御主礼装 (?:释放 )?技能 1(?: |$)/)).not.toBeInTheDocument();
    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith(
        "save_advanced_battle_scenes",
        expect.objectContaining({
          scenes: [
            expect.objectContaining({
              startupActions: [],
              turns: [
                expect.objectContaining({
                  actions: [expect.objectContaining({ id: "legacy_action" })],
                }),
                expect.objectContaining({ actions: [] }),
              ],
            }),
          ],
        })
      );
    });

    await user.click(screen.getByRole("button", { name: "上一回合" }));
    expect(screen.getByLabelText(/^御主礼装 (?:释放 )?技能 1(?: |$)/)).toBeInTheDocument();
  });

  it("saves and clears one enemy target for the advanced battle", async () => {
    const user = userEvent.setup();
    mockInvoke(async (cmd: string) => {
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

    await user.click(await screen.findByRole("button", {name:/^02 敌方目标/}));
    expect(screen.getByText("敌方目标选择")).toBeInTheDocument();
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
    mockInvoke(async (cmd: string) => {
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

    await userEvent.click(await screen.findByRole("button", {name:/^02 敌方目标/}));
    expect(screen.getByRole("button", { name: "敌人 3" })).toHaveClass("selected");
  });

  it("cancels the advanced preparation picker without saving a partial skill", async () => {
    const user = userEvent.setup();
    mockInvoke(async () => []);
    renderWithTheme(<CommandEditor projectId="project_1" advancedMode partyLineup={[makeServant(1, "甲"), makeServant(2, "乙"), makeServant(3, "丙")]} />);
    await user.click(await screen.findByRole("button", { name: "添加技能指令" }));
    await user.click(screen.getByRole("button", { name: "御主礼装" }));
    expect(screen.getByRole("button", { name: "技能 1" })).toBeInTheDocument();
    vi.mocked(invoke).mockClear();
    const cancel = screen.getByRole("button", { name: "撤销添加行动" });
    expect(cancel).toHaveTextContent("取消");
    await user.click(cancel);
    expect(screen.getByRole("button", { name: "添加技能指令" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "技能 1" })).not.toBeInTheDocument();
    expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "mutate_command_editor" || command === "save_advanced_battle_scenes")).toBe(false);
  });

  it("shows advanced startup targets when the selected servant skill targets one ally", async () => {
    const user = userEvent.setup();
    mockInvoke(async (cmd: string, args) => {
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

    await screen.findByRole("button", { name: "添加技能指令" });
    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith("get_servant_skill_targeting", {
        servantId: 1,
        variantKey: "1",
      });
    });
    await user.click(screen.getByRole("button", { name: "添加技能指令" }));
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

  it("shows both target choices and a hint for an advanced mixed-targeting skill", async () => {
    const user = userEvent.setup();
    mockInvoke(async (cmd: string, args) => {
      if (cmd === "load_advanced_battle_scenes") return [];
      const servantId =
        args && !Array.isArray(args) && typeof args === "object" && "servantId" in args
          ? args.servantId
          : null;
      if (cmd === "get_servant_skill_targeting" && servantId === 1) {
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

    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith("get_servant_skill_targeting", {
        servantId: 1,
        variantKey: "1",
      });
    });
    await user.click(screen.getByRole("button", { name: "添加技能指令" }));
    const sourceButtons = screen.getAllByRole("button", { name: "甲" });
    await user.click(sourceButtons[sourceButtons.length - 1]);
    await user.click(screen.getByRole("button", { name: "技能 2" }));

    expect(screen.getByRole("button", { name: "无目标" })).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "乙" }).length).toBeGreaterThan(0);
    expect(screen.getByText("该技能存在可选择目标与无需选择目标两种形态")).toHaveClass(
      "battle-targeting-mode-hint"
    );
  });

  it("saves advanced startup servant skills without targets when targeting is not required", async () => {
    const user = userEvent.setup();
    mockInvoke(async (cmd: string) => {
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

    await screen.findByRole("button", { name: "添加技能指令" });
    await waitFor(() => {
      expect(vi.mocked(invoke)).toHaveBeenCalledWith("get_servant_skill_targeting", {
        servantId: 1,
        variantKey: "1",
      });
    });
    await user.click(screen.getByRole("button", { name: "添加技能指令" }));
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
              turns: [
                expect.objectContaining({
                  actions: [
                    expect.objectContaining({
                      type: "servant",
                      servant: "servant_1",
                      skill: "skill_1",
                      target: null,
                    }),
                  ],
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
    mockInvoke(async (cmd: string) => {
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

    await user.click(await screen.findByRole("button", { name: "添加技能指令" }));
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
    mockInvoke(async (cmd: string) => {
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

    expect(await screen.findByLabelText(/^御主礼装 (?:释放 )?技能 1(?: |$)/)).toBeInTheDocument();
    const summary = container.querySelector(".battle-action-summary");
    const icon = summary?.querySelector(".command-row-source > .battle-inline-square");
    expect(icon).toHaveClass("battle-inline-square");
    expect(icon).toHaveAccessibleName("御主礼装");
    expect(icon?.querySelector(".battle-support-badge")).toBeNull();
  });

  it("configures grand servants from the advanced main output section", async () => {
    const user = userEvent.setup();
    const onGrandServantsChange = vi.fn();
    mockInvoke(async (cmd: string) => {
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

    await screen.findByText("冠位配置");
    await user.click(screen.getByRole("button", {name:"选择主冠位"}));
    await user.click(screen.getByRole("button", { name: "甲" }));
    await user.click(screen.getByRole("button", {name:"确认更换"}));

    expect(onGrandServantsChange).toHaveBeenCalledWith([
      { memberId: null, slotIndex: 0, servantId: 1, isSupport: false, npCard: "auto", priority: "damage", role: "main" },
    ]);
  });

  it("allows any class in the advanced grand output picker", async () => {
    const user = userEvent.setup();
    const onGrandServantsChange = vi.fn();
    mockInvoke(async (cmd: string) => {
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

    await screen.findByText("冠位配置");
    await user.click(screen.getByRole("button", {name:"选择主冠位"}));
    expect(screen.getByRole("button", { name: "剑阶甲" })).not.toBeDisabled();
    expect(screen.getByRole("button", { name: "术阶丙" })).not.toBeDisabled();

    await user.click(screen.getByRole("button", { name: "剑阶甲" }));
    await user.click(screen.getByRole("button", {name:"确认更换"}));

    expect(onGrandServantsChange).toHaveBeenCalledWith([
      { memberId: null, slotIndex: 0, servantId: 1, isSupport: false, npCard: "auto", priority: "damage", role: "main" },
    ]);
  });

  it("keeps persisted grand servants that do not match the selected grand class", async () => {
    mockInvoke(async (cmd: string) => {
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

    await screen.findByText("冠位配置");

    expect(screen.getByRole("button", { name: "主冠位：剑阶甲" })).toBeInTheDocument();
  });

  it("adds a second grand output servant without class restriction", async () => {
    const user = userEvent.setup();
    const onGrandServantsChange = vi.fn();
    mockInvoke(async (cmd: string) => {
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

    await screen.findByText("冠位配置");
    expect(screen.getByRole("button", { name: "主冠位：剑阶甲" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", {name:"选择副冠位"}));
    expect(screen.getByRole("button", { name: "狂阶乙" })).not.toBeDisabled();

    await user.click(screen.getByRole("button", { name: "狂阶乙" }));
    await user.click(screen.getByRole("button", {name:"确认更换"}));

    expect(onGrandServantsChange).toHaveBeenCalledWith([
      { memberId: null, slotIndex: 0, servantId: null, isSupport: false, npCard: "auto", priority: "damage", role: "main" },
      { memberId: null, slotIndex: 1, servantId: 2, isSupport: false, npCard: "auto", priority: "damage", role: "deputy" },
    ]);
  });

  it("assigns explicit single and aoe roles for lancer grand servants", async () => {
    const user = userEvent.setup();
    const onGrandServantsChange = vi.fn();
    mockInvoke(async (cmd: string) => {
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

    await user.click(screen.getByRole("button", {name:"选择单体冠位"}));
    await user.click(screen.getByRole("button", { name: "单体甲" }));
    await user.click(screen.getByRole("button", {name:"确认更换"}));
    expect(screen.getByRole("button", { name: "单体冠位：单体甲" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", {name:"选择光炮冠位"}));
    await user.click(screen.getByRole("button", { name: "光炮乙" }));
    await user.click(screen.getByRole("button", {name:"确认更换"}));
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
    mockInvoke(async (cmd: string) => {
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
    renderWithTheme(
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

    await screen.findByText("冠位配置");
    await userEvent.click(screen.getByRole("button", {name:"选择主冠位"}));

    const altriaButtons = screen.getAllByRole("button", { name: "乙" });
    expect(altriaButtons).toHaveLength(2);
    expect(altriaButtons[0].querySelector(".battle-support-badge")).toBeNull();
    expect(altriaButtons[1].querySelector(".battle-support-badge")).not.toBeNull();
    expect(screen.getByRole("dialog", {name:"更换冠位从者"}).querySelectorAll(".battle-support-badge")).toHaveLength(1);
  });

  it("shows inferred NP color for automatic grand servant settings", async () => {
    const user = userEvent.setup();
    mockInvoke(async (cmd: string) => {
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

    expect(await screen.findByRole("combobox", { name: "主冠位宝具颜色" })).toHaveTextContent("自动读取（红）");

    await user.click(screen.getByRole("button", { name: "主冠位：甲" }));
    await user.click(screen.getByRole("combobox", { name: "宝具颜色" }));

    expect(await screen.findByRole("option", { name: "自动读取（红）" })).toBeInTheDocument();
  });

  it("shows grand card rules in the attack step without redundant ordering controls", async () => {
    const user = userEvent.setup();
    mockInvoke(async (cmd: string) => {
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

    await user.click(await screen.findByRole("button", {name:/攻击阶段/}));
    expect(screen.getByRole("button", {name:"添加规则"})).toBeInTheDocument();
    expect(screen.queryByText("技能指令")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /上移/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /下移/ })).not.toBeInTheDocument();
    expect(screen.queryByText("连携")).not.toBeInTheDocument();
    expect(screen.queryByText("必须包含")).not.toBeInTheDocument();
    expect(screen.queryByText("排除")).not.toBeInTheDocument();
  });

  it("hides grand card strategy when the feature toggle is disabled", async () => {
    mockInvoke(async (cmd: string) => {
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

    await screen.findByRole("button", {name: "添加技能指令"});

    expect(screen.queryByRole("button", { name: /指令卡策略/ })).not.toBeInTheDocument();
  });

  it("adds and edits custom grand card rules", async () => {
    const user = userEvent.setup();
    const onGrandCardStrategyChange = vi.fn();
    mockInvoke(async (cmd: string) => {
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

    await user.click(await screen.findByRole("button", { name: /攻击阶段/ }));
    await user.click(screen.getByRole("button", { name: "添加规则" }));

    expect(screen.queryByRole("region", { name: "设置策略" })).not.toBeInTheDocument();
    await user.click(await screen.findByRole("button", { name: /第 1 张，任意从者/ }));
    expect(await screen.findByRole("region", { name: "设置策略" })).toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: "设置策略" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "冠位从者" }));
    expect(screen.getByRole("group", { name: "指令卡类型" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "重选" }));
    await user.click(screen.getByRole("button", { name: "甲" }));
    await user.click(screen.getByRole("button", { name: "宝具" }));
    expect(screen.queryByRole("region", { name: "设置策略" })).not.toBeInTheDocument();

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

    expect(screen.getAllByRole("button", { name: /任意从者/ })).toHaveLength(2);
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
    mockInvoke(async (cmd: string) => {
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

    await screen.findByRole("button", {name: "添加技能指令"});
    expect(screen.queryByText("Order Change")).not.toBeInTheDocument();
    expect(screen.getByText("甲")).toBeInTheDocument();
    expect(screen.getByText("丁")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "添加技能指令" }));

    expect(screen.getAllByRole("button", { name: "丁" }).length).toBeGreaterThan(0);
  });

  it("saves advanced startup servant skills with skill selections", async () => {
    const user = userEvent.setup();
    mockInvoke(async (cmd, args) => {
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
    await user.click(await screen.findByRole("button", { name: "添加技能指令" }));
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
              turns: [
                expect.objectContaining({
                  actions: [
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
    mockInvoke(async (cmd: string) => {
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

    expect(await screen.findByLabelText(/^御主礼装 (?:释放 )?技能 1(?: |$)/)).toBeInTheDocument();
    expect(screen.getByText("给")).toBeInTheDocument();
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
    mockInvoke(async (cmd: string) => {
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

    await screen.findByRole("button", {name: "添加技能指令"});
    const summary = container.querySelector(".battle-action-summary");
    expect(summary).toHaveAccessibleName("甲 技能 2");
    const skillIcon = summary?.querySelector(".battle-inline-skill-icon");
    expect(skillIcon).toHaveAttribute("title", "技能 2");
    expect(skillIcon).toHaveTextContent("2");
  });

  it("shows grand auto order change choice inside startup conditions", async () => {
    const user = userEvent.setup();
    mockInvoke(async (cmd: string) => {
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
    expect(screen.getByRole("button", {name:/控制行动/})).toBeInTheDocument();
    expect(screen.getByRole("button", {name: "添加技能指令"})).toBeInTheDocument();

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
    mockInvoke(async (cmd: string) => {
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
    expect(screen.getByRole("button", {name:/控制行动/})).toBeInTheDocument();
    expect(screen.getByRole("button", {name: "添加技能指令"})).toBeInTheDocument();
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
