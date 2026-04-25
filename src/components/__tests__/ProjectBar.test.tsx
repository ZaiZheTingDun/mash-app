import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithTheme } from "../../test/renderWithTheme";
import { ProjectBar } from "../ProjectBar";
import type { Project } from "../../types/project";
import { createInitialProjectSlots } from "../ContentGrid";

function makeProject(id: string, name: string): Project {
  return {
    id,
    name,
    supportServantId: null,
    slots: createInitialProjectSlots(),
    repeatMission: false,
  };
}

describe("ProjectBar", () => {
  it("renders the active project name in the trigger pill", () => {
    const projects = [makeProject("p1", "项目甲"), makeProject("p2", "项目乙")];
    renderWithTheme(
      <ProjectBar
        projects={projects}
        activeProjectId="p2"
        onProjectSelect={vi.fn()}
        onCreateProject={vi.fn()}
        onDeleteProject={vi.fn()}
      />
    );

    // Trigger label wraps the active project name with the ribbon
    // chrome (`～ … ～`); a substring match is enough.
    expect(screen.getByRole("button", { name: /项目乙/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /项目甲/ })).not.toBeInTheDocument();
  });

  it("falls back to '新建项目' when no project is active", () => {
    renderWithTheme(
      <ProjectBar
        projects={[]}
        activeProjectId={null}
        onProjectSelect={vi.fn()}
        onCreateProject={vi.fn()}
        onDeleteProject={vi.fn()}
      />
    );

    expect(screen.getByRole("button", { name: /新建项目/ })).toBeInTheDocument();
  });

  it("lists all projects and create+delete actions when the menu opens", async () => {
    const user = userEvent.setup();
    const projects = [makeProject("p1", "项目甲"), makeProject("p2", "项目乙")];
    renderWithTheme(
      <ProjectBar
        projects={projects}
        activeProjectId="p1"
        onProjectSelect={vi.fn()}
        onCreateProject={vi.fn()}
        onDeleteProject={vi.fn()}
      />
    );

    await user.click(screen.getByRole("button", { name: /项目甲/ }));

    // Both project names are reachable as menu items.
    expect(await screen.findByRole("menuitem", { name: /项目甲/ })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: /项目乙/ })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: /新建项目/ })).toBeInTheDocument();
    expect(screen.getByRole("menuitem", { name: /删除当前项目/ })).toBeInTheDocument();
  });

  it("invokes onProjectSelect when a non-active project is chosen", async () => {
    const user = userEvent.setup();
    const onProjectSelect = vi.fn();
    const projects = [makeProject("p1", "项目甲"), makeProject("p2", "项目乙")];
    renderWithTheme(
      <ProjectBar
        projects={projects}
        activeProjectId="p1"
        onProjectSelect={onProjectSelect}
        onCreateProject={vi.fn()}
        onDeleteProject={vi.fn()}
      />
    );

    await user.click(screen.getByRole("button", { name: /项目甲/ }));
    await user.click(await screen.findByRole("menuitem", { name: /项目乙/ }));

    expect(onProjectSelect).toHaveBeenCalledWith("p2");
  });

  it("invokes onCreateProject from the menu", async () => {
    const user = userEvent.setup();
    const onCreateProject = vi.fn();
    renderWithTheme(
      <ProjectBar
        projects={[makeProject("p1", "项目甲")]}
        activeProjectId="p1"
        onProjectSelect={vi.fn()}
        onCreateProject={onCreateProject}
        onDeleteProject={vi.fn()}
      />
    );

    await user.click(screen.getByRole("button", { name: /项目甲/ }));
    await user.click(await screen.findByRole("menuitem", { name: /新建项目/ }));

    expect(onCreateProject).toHaveBeenCalledTimes(1);
  });

  describe("delete action", () => {
    let confirmSpy: ReturnType<typeof vi.spyOn>;

    beforeEach(() => {
      confirmSpy = vi.spyOn(window, "confirm");
    });

    afterEach(() => {
      confirmSpy.mockRestore();
    });

    it("calls onDeleteProject when the user confirms the prompt", async () => {
      const user = userEvent.setup();
      const onDeleteProject = vi.fn();
      confirmSpy.mockReturnValue(true);

      renderWithTheme(
        <ProjectBar
          projects={[makeProject("p1", "项目甲")]}
          activeProjectId="p1"
          onProjectSelect={vi.fn()}
          onCreateProject={vi.fn()}
          onDeleteProject={onDeleteProject}
        />
      );

      await user.click(screen.getByRole("button", { name: /项目甲/ }));
      await user.click(
        await screen.findByRole("menuitem", { name: /删除当前项目/ })
      );

      expect(confirmSpy).toHaveBeenCalledTimes(1);
      expect(onDeleteProject).toHaveBeenCalledWith("p1");
    });

    it("skips onDeleteProject when the user cancels the prompt", async () => {
      const user = userEvent.setup();
      const onDeleteProject = vi.fn();
      confirmSpy.mockReturnValue(false);

      renderWithTheme(
        <ProjectBar
          projects={[makeProject("p1", "项目甲")]}
          activeProjectId="p1"
          onProjectSelect={vi.fn()}
          onCreateProject={vi.fn()}
          onDeleteProject={onDeleteProject}
        />
      );

      await user.click(screen.getByRole("button", { name: /项目甲/ }));
      await user.click(
        await screen.findByRole("menuitem", { name: /删除当前项目/ })
      );

      expect(confirmSpy).toHaveBeenCalledTimes(1);
      expect(onDeleteProject).not.toHaveBeenCalled();
    });
  });
});
