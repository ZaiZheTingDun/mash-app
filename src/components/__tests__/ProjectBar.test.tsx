import { describe, it, expect, vi } from "vitest";
import type { ComponentProps } from "react";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithTheme } from "../../test/renderWithTheme";
import { ProjectBar } from "../ProjectBar";
import type { Project } from "../../types/project";
import { createInitialProjectSlots } from "../projectSlots";

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
  function renderProjectBar(overrides?: Partial<ComponentProps<typeof ProjectBar>>) {
    const props: ComponentProps<typeof ProjectBar> = {
      projects: [makeProject("p1", "项目甲")],
      activeProjectId: "p1",
      onProjectSelect: vi.fn(),
      onCreateProject: vi.fn(),
      onRenameProject: vi.fn(),
      onDuplicateProject: vi.fn(),
      onDeleteProject: vi.fn(),
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

    expect(onCreateProject).toHaveBeenCalledWith("周回队伍", false);
  });

  it("creates an advanced project when the advanced toggle is enabled", async () => {
    const user = userEvent.setup();
    const onCreateProject = vi.fn();
    renderProjectBar({ onCreateProject });

    await user.click(screen.getByRole("button", { name: /队伍操作/ }));
    await user.click(await screen.findByRole("menuitem", { name: /新建队伍/ }));
    await user.click(await screen.findByRole("switch", { name: /高级模式/ }));
    await user.click(screen.getByRole("button", { name: "新建" }));

    expect(onCreateProject).toHaveBeenCalledWith("队伍 2", true);
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
