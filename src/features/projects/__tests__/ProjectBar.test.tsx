import { describe, it, expect, vi } from "vitest";
import type { ComponentProps } from "react";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { ProjectBar } from "../ProjectBar";
import type { Project } from "../../../types/project";
import type { GrandClassDefinition } from "../../../types/project";
import { createInitialProjectSlots } from "../../team/projectSlots";

function makeProject(id: string, name: string): Project {
  return {
    id,
    name,
    advancedMode: false,
    supportServantId: null,
    slots: createInitialProjectSlots(),
    repeatMission: false,
  };
}

describe("ProjectBar", () => {
  const definitions: GrandClassDefinition[] = [
    { id: "saber", label: "剑阶冠位", servantClass: "Saber", roles: [], cardPriorityEnabled: true, autoOrderChangeRoles: [], validationMessage: "" },
    { id: "lancer", label: "枪阶冠位", servantClass: "Lancer", roles: [], cardPriorityEnabled: false, autoOrderChangeRoles: [], validationMessage: "" },
    { id: "berserker", label: "狂阶冠位", servantClass: "Berserker", roles: [], cardPriorityEnabled: true, autoOrderChangeRoles: [], validationMessage: "" },
    { id: "extra1Fire", label: "Extra1 · 火", servantClass: "Extra1", selectionGroup: "extra1", selectionGroupLabel: "额外职阶 Ⅰ 冠位", selectionOptionLabel: "火", roles: [], cardPriorityEnabled: false, autoOrderChangeRoles: [], validationMessage: "" },
    { id: "extra1Earth", label: "Extra1 · 地", servantClass: "Extra1", selectionGroup: "extra1", selectionGroupLabel: "额外职阶 Ⅰ 冠位", selectionOptionLabel: "地", roles: [], cardPriorityEnabled: false, autoOrderChangeRoles: [], validationMessage: "" },
    { id: "extra2Wind", label: "Extra2 · 风", servantClass: "Extra2", selectionGroup: "extra2", selectionGroupLabel: "额外职阶 Ⅱ 冠位", selectionOptionLabel: "风", roles: [], cardPriorityEnabled: false, autoOrderChangeRoles: [], validationMessage: "" },
    { id: "extra2Water", label: "Extra2 · 水", servantClass: "Extra2", selectionGroup: "extra2", selectionGroupLabel: "额外职阶 Ⅱ 冠位", selectionOptionLabel: "水", roles: [], cardPriorityEnabled: false, autoOrderChangeRoles: [], validationMessage: "" },
  ];
  function renderProjectBar(overrides?: Partial<ComponentProps<typeof ProjectBar>>) {
    const props: ComponentProps<typeof ProjectBar> = {
      projects: [makeProject("p1", "项目甲")],
      grandClassDefinitions: definitions,
      activeProjectId: "p1",
      onProjectSelect: vi.fn(),
      onCreateProject: vi.fn(),
      onRenameProject: vi.fn(),
      onDuplicateProject: vi.fn(),
      onDeleteProject: vi.fn(),
      onOpenProjectSettings: vi.fn(),
      ...overrides,
    };
    return {
      ...renderWithTheme(<ProjectBar {...props} />),
      props,
    };
  }

  it("renders the active project name in the trigger pill", () => {
    const projects = [makeProject("p1", "项目甲"), makeProject("p2", "项目乙")];
    renderProjectBar({ projects, activeProjectId: "p2" });

    // Trigger label wraps the active project name with the ribbon
    // chrome (`～ … ～`); a substring match is enough.
    expect(screen.getByRole("button", { name: /项目乙/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /项目甲/ })).not.toBeInTheDocument();
  });

  it("falls back to '选择队伍' when no project is active", () => {
    renderProjectBar({ projects: [], activeProjectId: null });

    expect(screen.getByRole("button", { name: /选择队伍/ })).toBeInTheDocument();
  });

  it("lists only projects when the selector menu opens", async () => {
    const user = userEvent.setup();
    const projects = [makeProject("p1", "项目甲"), makeProject("p2", "项目乙")];
    renderProjectBar({ projects, activeProjectId: "p1" });

    await user.click(screen.getByRole("button", { name: /项目甲/ }));

    expect(await screen.findByRole("menuitem", { name: /项目甲/ })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: /项目乙/ })).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: /新建队伍/ })).not.toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: /删除当前队伍/ })).not.toBeInTheDocument();
  });

  it("invokes onProjectSelect when a non-active project is chosen", async () => {
    const user = userEvent.setup();
    const onProjectSelect = vi.fn();
    const projects = [makeProject("p1", "项目甲"), makeProject("p2", "项目乙")];
    renderProjectBar({ projects, activeProjectId: "p1", onProjectSelect });

    await user.click(screen.getByRole("button", { name: /项目甲/ }));
    await user.click(await screen.findByRole("menuitem", { name: /项目乙/ }));

    expect(onProjectSelect).toHaveBeenCalledWith("p2");
  });

  it("opens a name dialog before creating a project", async () => {
    const user = userEvent.setup();
    const onCreateProject = vi.fn();
    renderProjectBar({ onCreateProject });

    await user.click(screen.getByRole("button", { name: /队伍操作/ }));
    await user.click(await screen.findByRole("menuitem", { name: /新建队伍/ }));
    const input = await screen.findByRole("textbox", { name: /队伍名称/ });
    await user.clear(input);
    await user.type(input, "周回队伍");
    await user.click(screen.getByRole("button", { name: "新建" }));

    expect(onCreateProject).toHaveBeenCalledWith("周回队伍", false, "saber");
  });

  it("creates an advanced project when grand mode is selected", async () => {
    const user = userEvent.setup();
    const onCreateProject = vi.fn();
    renderProjectBar({ onCreateProject });

    await user.click(screen.getByRole("button", { name: /队伍操作/ }));
    await user.click(await screen.findByRole("menuitem", { name: /新建队伍/ }));
    await user.click(await screen.findByRole("combobox", { name: "队伍模式" }));
    await user.click(await screen.findByRole("option", { name: "戴冠战模式" }));
    await user.click(screen.getByRole("button", { name: "新建" }));

    expect(onCreateProject).toHaveBeenCalledWith("队伍 2", true, "saber");
  });

  it("creates a berserker grand project from the create dialog", async () => {
    const user = userEvent.setup();
    const onCreateProject = vi.fn();
    renderProjectBar({ onCreateProject });

    await user.click(screen.getByRole("button", { name: /队伍操作/ }));
    await user.click(await screen.findByRole("menuitem", { name: /新建队伍/ }));
    await user.click(await screen.findByRole("combobox", { name: "队伍模式" }));
    await user.click(await screen.findByRole("option", { name: "戴冠战模式" }));
    await user.click(screen.getByRole("combobox", { name: "冠位职阶" }));
    await user.click(await screen.findByRole("option", { name: "狂阶冠位" }));
    await user.click(screen.getByRole("button", { name: "新建" }));

    expect(onCreateProject).toHaveBeenCalledWith("队伍 2", true, "berserker");
  });

  it("creates a lancer grand project from the create dialog", async () => {
    const user = userEvent.setup();
    const onCreateProject = vi.fn();
    renderProjectBar({ onCreateProject });

    await user.click(screen.getByRole("button", { name: /队伍操作/ }));
    await user.click(await screen.findByRole("menuitem", { name: /新建队伍/ }));
    await user.click(await screen.findByRole("combobox", { name: "队伍模式" }));
    await user.click(await screen.findByRole("option", { name: "戴冠战模式" }));
    await user.click(screen.getByRole("combobox", { name: "冠位职阶" }));
    await user.click(await screen.findByRole("option", { name: "枪阶冠位" }));
    await user.click(screen.getByRole("button", { name: "新建" }));

    expect(onCreateProject).toHaveBeenCalledWith("队伍 2", true, "lancer");
  });

  it("selects Extra group first and then its stage attribute", async () => {
    const user = userEvent.setup();
    const onCreateProject = vi.fn();
    renderProjectBar({ onCreateProject });

    await user.click(screen.getByRole("button", { name: /队伍操作/ }));
    await user.click(await screen.findByRole("menuitem", { name: /新建队伍/ }));
    await user.click(await screen.findByRole("combobox", { name: "队伍模式" }));
    await user.click(await screen.findByRole("option", { name: "戴冠战模式" }));
    await user.click(screen.getByRole("combobox", { name: "冠位职阶" }));
    expect(screen.queryByRole("option", { name: "Extra1 · 火" })).not.toBeInTheDocument();
    await user.click(await screen.findByRole("option", { name: "额外职阶 Ⅰ 冠位" }));
    expect(screen.queryByText("副本属性")).not.toBeInTheDocument();
    await user.click(screen.getByRole("combobox", { name: "副本属性" }));
    await user.click(await screen.findByRole("option", { name: "地" }));
    await user.click(screen.getByRole("button", { name: "新建" }));

    expect(onCreateProject).toHaveBeenCalledWith("队伍 2", true, "extra1Earth");
  });

  it("resets the stage attribute when switching from Extra1 to Extra2", async () => {
    const user = userEvent.setup();
    renderProjectBar();

    await user.click(screen.getByRole("button", { name: /队伍操作/ }));
    await user.click(await screen.findByRole("menuitem", { name: /新建队伍/ }));
    await user.click(await screen.findByRole("combobox", { name: "队伍模式" }));
    await user.click(await screen.findByRole("option", { name: "戴冠战模式" }));

    const classSelect = screen.getByRole("combobox", { name: "冠位职阶" });
    await user.click(classSelect);
    await user.click(await screen.findByRole("option", { name: "额外职阶 Ⅰ 冠位" }));
    expect(screen.getByRole("combobox", { name: "副本属性" })).toHaveTextContent("火");

    await user.click(classSelect);
    await user.click(await screen.findByRole("option", { name: "额外职阶 Ⅱ 冠位" }));
    expect(screen.getByRole("combobox", { name: "副本属性" })).toHaveTextContent("风");
  });

  it("renames the active project from the action menu", async () => {
    const user = userEvent.setup();
    const onRenameProject = vi.fn();
    renderProjectBar({ onRenameProject });

    await user.click(screen.getByRole("button", { name: /队伍操作/ }));
    await user.click(await screen.findByRole("menuitem", { name: /重命名当前队伍/ }));
    const input = await screen.findByRole("textbox", { name: /队伍名称/ });
    await user.clear(input);
    await user.type(input, "新名字");
    await user.click(screen.getByRole("button", { name: "保存" }));

    expect(onRenameProject).toHaveBeenCalledWith("p1", "新名字");
  });

  it("opens team settings from the action menu", async () => {
    const user = userEvent.setup();
    const onOpenProjectSettings = vi.fn();
    renderProjectBar({ onOpenProjectSettings });

    await user.click(screen.getByRole("button", { name: /队伍操作/ }));
    await user.click(await screen.findByRole("menuitem", { name: /队伍设置/ }));

    expect(onOpenProjectSettings).toHaveBeenCalledOnce();
  });

  it("disables team settings when no project is active", async () => {
    const user = userEvent.setup();
    renderProjectBar({ projects: [], activeProjectId: null });

    await user.click(screen.getByRole("button", { name: /队伍操作/ }));

    expect(await screen.findByRole("menuitem", { name: /队伍设置/ })).toHaveAttribute(
      "aria-disabled",
      "true"
    );
  });

  it("duplicates the active project from the action menu", async () => {
    const user = userEvent.setup();
    const onDuplicateProject = vi.fn();
    renderProjectBar({ onDuplicateProject });

    await user.click(screen.getByRole("button", { name: /队伍操作/ }));
    await user.click(await screen.findByRole("menuitem", { name: /复制当前队伍/ }));
    const input = await screen.findByRole("textbox", { name: /队伍名称/ });
    expect(input).toHaveValue("项目甲 副本");
    await user.click(screen.getByRole("button", { name: "复制" }));

    expect(onDuplicateProject).toHaveBeenCalledWith("p1", "项目甲 副本");
  });

  describe("delete action", () => {
    it("calls onDeleteProject only after the user confirms the dialog", async () => {
      const user = userEvent.setup();
      const onDeleteProject = vi.fn();
      renderProjectBar({ onDeleteProject });

      await user.click(screen.getByRole("button", { name: /队伍操作/ }));
      await user.click(
        await screen.findByRole("menuitem", { name: /删除当前队伍/ })
      );

      expect(await screen.findByRole("alertdialog")).toBeInTheDocument();
      expect(onDeleteProject).not.toHaveBeenCalled();

      await user.click(screen.getByRole("button", { name: "删除" }));
      expect(onDeleteProject).toHaveBeenCalledWith("p1");
    });

    it("skips onDeleteProject when the user cancels the dialog", async () => {
      const user = userEvent.setup();
      const onDeleteProject = vi.fn();
      renderProjectBar({ onDeleteProject });

      await user.click(screen.getByRole("button", { name: /队伍操作/ }));
      await user.click(
        await screen.findByRole("menuitem", { name: /删除当前队伍/ })
      );
      await user.click(await screen.findByRole("button", { name: "取消" }));

      expect(onDeleteProject).not.toHaveBeenCalled();
    });
  });
});
