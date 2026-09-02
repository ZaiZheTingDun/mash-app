import { describe, it, expect, vi } from "vitest";
import type { ComponentProps } from "react";
import { fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { ProjectBar } from "../ProjectBar";
import {
  projectCatalogDropAction,
  projectDragId,
  projectGroupDragId,
  reorderProjectCatalogIds,
} from "../projectCatalogOrder";
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
      projectCatalog: {
        schemaVersion: 1,
        groups: [],
        ungroupedProjectIds: ["p1"],
      },
      grandClassDefinitions: definitions,
      activeProjectId: "p1",
      onProjectSelect: vi.fn(),
      onCreateProject: vi.fn(),
      onRenameProject: vi.fn(),
      onDuplicateProject: vi.fn(),
      onDeleteProject: vi.fn(),
      onCreateProjectGroup: vi.fn(async () => {}),
      onRenameProjectGroup: vi.fn(async () => {}),
      onDeleteProjectGroup: vi.fn(async () => {}),
      onMoveProjectToGroup: vi.fn(async () => {}),
      onReorderProjectGroups: vi.fn(async () => {}),
      onReorderProjectsInGroup: vi.fn(async () => {}),
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

  it("lists projects inside the ungrouped section when the selector opens", async () => {
    const user = userEvent.setup();
    const projects = [makeProject("p1", "项目甲"), makeProject("p2", "项目乙")];
    renderProjectBar({ projects, activeProjectId: "p1" });

    await user.click(screen.getByRole("button", { name: /项目甲/ }));

    expect(await screen.findByRole("button", { name: /未分组 2/ })).toBeInTheDocument();
    expect(screen.getByText("项目甲")).toBeInTheDocument();
    expect(screen.getByText("项目乙")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /新建队伍/ })).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: /删除当前队伍/ })).not.toBeInTheDocument();
  });

  it("invokes onProjectSelect when a non-active project is chosen", async () => {
    const user = userEvent.setup();
    const onProjectSelect = vi.fn();
    const projects = [makeProject("p1", "项目甲"), makeProject("p2", "项目乙")];
    renderProjectBar({ projects, activeProjectId: "p1", onProjectSelect });

    await user.click(screen.getByRole("button", { name: /项目甲/ }));
    await user.click(await screen.findByText("项目乙"));

    expect(onProjectSelect).toHaveBeenCalledWith("p2");
  });

  it("expands the active project group and searches across collapsed groups", async () => {
    const user = userEvent.setup();
    const projects = [makeProject("p1", "周回队伍"), makeProject("p2", "高难队伍")];
    renderProjectBar({
      projects,
      activeProjectId: "p1",
      projectCatalog: {
        schemaVersion: 1,
        groups: [{ id: "weekly", name: "90++ 周回", projectIds: ["p1"] }],
        ungroupedProjectIds: ["p2"],
      },
    });

    await user.click(screen.getByRole("button", { name: /周回队伍/ }));
    expect(await screen.findByText("90++ 周回")).toBeInTheDocument();
    expect(screen.getByText("周回队伍")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /90\+\+ 周回 1/ }));
    expect(screen.queryByText("周回队伍")).not.toBeInTheDocument();

    await user.type(screen.getByRole("textbox", { name: "搜索队伍" }), "周回");
    expect(screen.getByText("周回队伍")).toBeInTheDocument();
    expect(screen.getByText("位于：90++ 周回")).toBeInTheDocument();
  });

  it("creates a project group from the management dialog", async () => {
    const user = userEvent.setup();
    const onCreateProjectGroup = vi.fn(async () => {});
    renderProjectBar({ onCreateProjectGroup });

    await user.click(screen.getByRole("button", { name: /项目甲/ }));
    await user.click(await screen.findByRole("button", { name: /管理所有队伍/ }));
    await user.click(await screen.findByRole("button", { name: "新建分组" }));
    await user.type(screen.getByRole("textbox", { name: "分组名称" }), "活动周回");
    await user.click(screen.getByRole("button", { name: "新建" }));

    expect(onCreateProjectGroup).toHaveBeenCalledWith("活动周回");
  });

  it("removes the project group dropdown from the management dialog", async () => {
    const user = userEvent.setup();
    renderProjectBar({
      projectCatalog: {
        schemaVersion: 1,
        groups: [{ id: "weekly", name: "周回", projectIds: ["p1"] }],
        ungroupedProjectIds: [],
      },
    });

    await user.click(screen.getByRole("button", { name: /项目甲/ }));
    await user.click(await screen.findByRole("button", { name: /管理所有队伍/ }));
    expect(screen.queryByRole("combobox", { name: "项目甲所属分组" })).not.toBeInTheDocument();
  });

  it("makes each project group row draggable", async () => {
    const user = userEvent.setup();
    renderProjectBar({
      projectCatalog: {
        schemaVersion: 1,
        groups: [
          { id: "weekly", name: "周回", projectIds: ["p1"] },
          { id: "challenge", name: "高难", projectIds: [] },
        ],
        ungroupedProjectIds: [],
      },
    });

    await user.click(screen.getByRole("button", { name: /项目甲/ }));
    await user.click(await screen.findByRole("button", { name: /管理所有队伍/ }));
    expect(await screen.findByRole("button", { name: /^周回/ })).toHaveAttribute("tabindex", "0");
    expect(screen.getByRole("button", { name: /^高难/ })).toHaveAttribute("tabindex", "0");
  });

  it("makes each project row draggable", async () => {
    const user = userEvent.setup();
    const projects = [makeProject("p1", "队伍甲"), makeProject("p2", "队伍乙")];
    renderProjectBar({
      projects,
      projectCatalog: {
        schemaVersion: 1,
        groups: [{ id: "weekly", name: "周回", projectIds: ["p1", "p2"] }],
        ungroupedProjectIds: [],
      },
    });

    await user.click(screen.getByRole("button", { name: /队伍甲/ }));
    await user.click(await screen.findByRole("button", { name: /管理所有队伍/ }));
    expect(await screen.findByRole("button", { name: /队伍甲 普通/ })).toHaveAttribute(
      "tabindex",
      "0",
    );
    expect(screen.getByRole("button", { name: /队伍乙 普通/ })).toHaveAttribute("tabindex", "0");
  });

  it("renames a non-active project from its management row context menu", async () => {
    const user = userEvent.setup();
    const onRenameProject = vi.fn();
    const projects = [makeProject("p1", "队伍甲"), makeProject("p2", "队伍乙")];
    renderProjectBar({ projects, activeProjectId: "p1", onRenameProject });

    await user.click(screen.getByRole("button", { name: /队伍甲/ }));
    await user.click(await screen.findByRole("button", { name: /管理所有队伍/ }));
    fireEvent.contextMenu(await screen.findByRole("button", { name: /队伍乙 普通/ }));
    await user.click(await screen.findByRole("menuitem", { name: "重命名队伍" }));

    const input = await screen.findByRole("textbox", { name: "队伍名称" });
    expect(input).toHaveValue("队伍乙");
    await user.clear(input);
    await user.type(input, "高难队伍");
    await user.click(screen.getByRole("button", { name: "保存" }));

    expect(onRenameProject).toHaveBeenCalledWith("p2", "高难队伍");
  });

  it("deletes a non-active project from its management row context menu after confirmation", async () => {
    const user = userEvent.setup();
    const onDeleteProject = vi.fn();
    const projects = [makeProject("p1", "队伍甲"), makeProject("p2", "队伍乙")];
    renderProjectBar({ projects, activeProjectId: "p1", onDeleteProject });

    await user.click(screen.getByRole("button", { name: /队伍甲/ }));
    await user.click(await screen.findByRole("button", { name: /管理所有队伍/ }));
    fireEvent.contextMenu(await screen.findByRole("button", { name: /队伍乙 普通/ }));
    await user.click(await screen.findByRole("menuitem", { name: "删除队伍" }));

    const dialog = await screen.findByRole("alertdialog");
    expect(dialog).toHaveTextContent("确定删除队伍「队伍乙」吗？此操作无法撤销。");
    expect(onDeleteProject).not.toHaveBeenCalled();
    await user.click(within(dialog).getByRole("button", { name: "删除" }));

    expect(onDeleteProject).toHaveBeenCalledWith("p2");
  });

  it("computes persisted catalog order from drag ids", () => {
    expect(reorderProjectCatalogIds(["a", "b", "c"], "a", "c")).toEqual(["b", "c", "a"]);
    const current = ["a", "b"];
    expect(reorderProjectCatalogIds(current, "missing", "b")).toBe(current);
  });

  it("treats dropping a project on a group as a cross-group move", () => {
    expect(
      projectCatalogDropAction(
        projectDragId("p1"),
        projectGroupDragId("challenge"),
        "weekly",
        ["weekly", "challenge"],
        ["p1", "p2"],
      ),
    ).toEqual({ type: "moveProject", projectId: "p1", groupId: "challenge" });
    expect(
      projectCatalogDropAction(
        projectDragId("p1"),
        projectGroupDragId(null),
        "weekly",
        ["weekly"],
        ["p1", "p2"],
      ),
    ).toEqual({ type: "moveProject", projectId: "p1", groupId: null });
  });

  it("keeps project-on-project drops as in-group reordering", () => {
    expect(
      projectCatalogDropAction(
        projectDragId("p1"),
        projectDragId("p2"),
        "weekly",
        ["weekly"],
        ["p1", "p2"],
      ),
    ).toEqual({ type: "reorderProjects", groupId: "weekly", projectIds: ["p2", "p1"] });
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

    expect(onCreateProject).toHaveBeenCalledWith("周回队伍", false, "saber", null);
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

    expect(onCreateProject).toHaveBeenCalledWith("队伍 2", true, "saber", null);
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

    expect(onCreateProject).toHaveBeenCalledWith("队伍 2", true, "berserker", null);
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

    expect(onCreateProject).toHaveBeenCalledWith("队伍 2", true, "lancer", null);
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

    expect(onCreateProject).toHaveBeenCalledWith("队伍 2", true, "extra1Earth", null);
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
