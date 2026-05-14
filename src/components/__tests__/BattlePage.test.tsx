import { describe, expect, it, vi } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { useState } from "react";
import { renderWithTheme } from "../../test/renderWithTheme";
import { BattlePage } from "../BattlePage";
import type { Project } from "../../types/project";

const PROJECT: Project = {
  id: "project-1",
  name: "project 1",
  supportServantId: null,
  supportServantVariantKey: null,
  repeatMission: false,
  repeatMode: "single",
  repeatCount: null,
  apRecoveryItems: [],
  slots: [
    { id: "slot-0", type: "servant", servantId: null },
    { id: "slot-1", type: "servant", servantId: null },
    { id: "slot-2", type: "support", servantId: null, craftEssenceId: 1485 },
    { id: "slot-3", type: "servant", servantId: null },
    { id: "slot-4", type: "servant", servantId: null },
    { id: "slot-5", type: "servant", servantId: null },
  ],
};

function mockProjectCommands() {
  vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
    const typedArgs = args as { project?: Project } | undefined;
    if (cmd === "update_project") {
      return typedArgs?.project ?? null;
    }
    return null;
  });
}

function renderBattlePage(initialProject: Project) {
  const callbacks = {
    onCreateProject: vi.fn(),
    onRenameProject: vi.fn(),
    onDuplicateProject: vi.fn(),
    onDeleteProject: vi.fn(),
    onBack: vi.fn(),
    onAutomationStart: vi.fn(),
    onLogEntry: vi.fn(),
  };

  function Harness() {
    const [projects, setProjects] = useState<Project[]>([initialProject]);
    const [activeProjectId, setActiveProjectId] = useState<string | null>(initialProject.id);
    const handleUpdateProject = async (project: Project) => {
      const saved = await invoke<Project>("update_project", { project });
      setProjects((prev) => prev.map((item) => (item.id === saved.id ? saved : item)));
    };

    return (
      <BattlePage
        projects={projects}
        activeProjectId={activeProjectId}
        onProjectSelect={setActiveProjectId}
        onCreateProject={callbacks.onCreateProject}
        onRenameProject={callbacks.onRenameProject}
        onDuplicateProject={callbacks.onDuplicateProject}
        onDeleteProject={callbacks.onDeleteProject}
        onUpdateProject={handleUpdateProject}
        onBack={callbacks.onBack}
        onAutomationStart={callbacks.onAutomationStart}
        onLogEntry={callbacks.onLogEntry}
      />
    );
  }

  renderWithTheme(<Harness />);
  return callbacks;
}

describe("BattlePage", () => {
  it("starts automation with ordered apples and repeat count", async () => {
    const user = userEvent.setup();
    const project = {
      ...PROJECT,
      repeatMission: true,
      repeatMode: "count",
      repeatCount: 100,
      apRecoveryItems: ["rainbow", "copper", "gold"],
    } satisfies Project;
    mockProjectCommands();
    renderBattlePage(project);

    await user.click(await screen.findByRole("button", { name: "开始" }));
    await user.click(screen.getByRole("button", { name: "确认开始" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("start_automation", {
        config: expect.objectContaining({
          projectId: "project-1",
          repeatMission: false,
          maxMissionRuns: 100,
          apRecoveryItems: ["gold", "copper", "rainbow"],
        }),
      });
    });
  });

  it("requests the shared operation log after start", async () => {
    const user = userEvent.setup();
    mockProjectCommands();
    const callbacks = renderBattlePage(PROJECT);

    await user.click(await screen.findByRole("button", { name: "开始" }));

    expect(callbacks.onAutomationStart).toHaveBeenCalledTimes(1);
  });

  it("passes support skill and NP level requirements to automation", async () => {
    const user = userEvent.setup();
    mockProjectCommands();
    renderBattlePage({
      ...PROJECT,
      supportNoblePhantasmLevelMin: 2,
      supportSkillLevelMins: [10, null, 9],
      supportAppendSkillLevelMins: [null, 10, null, null, 6],
    });

    await user.click(await screen.findByRole("button", { name: "开始" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("start_automation", {
        config: expect.objectContaining({
          supportNoblePhantasmLevelMin: 2,
          supportSkillLevelMins: [10, null, 9],
          supportAppendSkillLevelMins: [null, 10, null, null, 6],
        }),
      });
    });
  });

  it("persists count mode and repeat count from the inline stepper", async () => {
    const user = userEvent.setup();
    mockProjectCommands();
    renderBattlePage({
      ...PROJECT,
      repeatMission: true,
      repeatMode: "count",
      repeatCount: 11,
    });

    await user.click(await screen.findByRole("button", { name: "增加重复次数" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("update_project", {
        project: expect.objectContaining({
          id: "project-1",
          repeatMission: true,
          repeatMode: "count",
          repeatCount: 12,
        }),
      });
    });
  });

  it("saves saint quartz on switch toggle and warns before start", async () => {
    const user = userEvent.setup();
    mockProjectCommands();
    renderBattlePage(PROJECT);

    await user.click(await screen.findByRole("checkbox", { name: /圣晶石/ }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("update_project", {
        project: expect.objectContaining({
          id: "project-1",
          apRecoveryItems: ["rainbow"],
        }),
      });
    });

    await user.click(screen.getByRole("button", { name: "开始" }));

    expect(screen.getByText("确认开始任务")).toBeInTheDocument();
    expect(vi.mocked(invoke).mock.calls.some(([cmd]) => cmd === "start_automation")).toBe(false);

    await user.click(screen.getByRole("button", { name: "确认开始" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("start_automation", {
        config: expect.objectContaining({
          apRecoveryItems: ["rainbow"],
        }),
      });
    });
  });

  it("starts with pending local draft before project save resolves", async () => {
    const user = userEvent.setup();
    const pendingUpdates: Array<(project: Project) => void> = [];
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      const typedArgs = args as { project?: Project } | undefined;
      if (cmd === "update_project") {
        return new Promise<Project>((resolve) => {
          pendingUpdates.push(resolve);
          void typedArgs;
        });
      }
      return null;
    });
    renderBattlePage({
      ...PROJECT,
      repeatMission: true,
      repeatMode: "count",
      repeatCount: 2,
    });

    await user.click(await screen.findByRole("button", { name: "增加重复次数" }));
    await user.click(screen.getByRole("button", { name: "开始" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("start_automation", {
        config: expect.objectContaining({
          maxMissionRuns: 3,
        }),
      });
    });

    pendingUpdates[0]?.({
      ...PROJECT,
      repeatMission: true,
      repeatMode: "count",
      repeatCount: 3,
    });
  });

  it("disables stop-after-current after it is requested", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      const typedArgs = args as { project?: Project } | undefined;
      if (cmd === "update_project") {
        return typedArgs?.project ?? null;
      }
      return null;
    });
    renderBattlePage(PROJECT);

    await user.click(await screen.findByRole("button", { name: "开始" }));
    expect(screen.getByRole("button", { name: "返回" })).toBeDisabled();
    const stopAfterCurrentButton = screen.getByRole("button", {
      name: "运行完当前轮次后停止",
    });
    await user.click(stopAfterCurrentButton);

    await waitFor(() => {
      expect(stopAfterCurrentButton).toBeDisabled();
    });
  });
});
