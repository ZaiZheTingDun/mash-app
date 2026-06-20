import { describe, expect, it, vi } from "vitest";
import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { listen, type Event } from "@tauri-apps/api/event";
import { useState } from "react";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { BattlePage } from "../BattlePage";
import type { Project } from "../../../types/project";
import type { Servant } from "../../../types/servant";

const SABER: Servant = {
  id: 1,
  variantKey: "1",
  name_cn: "剑阶甲",
  name_jp: "剑阶甲",
  name_en: "Saber A",
  class: "Saber",
  rarity: 5,
};

const BERSERKER: Servant = {
  id: 2,
  variantKey: "2",
  name_cn: "狂阶乙",
  name_jp: "狂阶乙",
  name_en: "Berserker B",
  class: "Berserker",
  rarity: 5,
};

const CASTER: Servant = {
  id: 3,
  variantKey: "3",
  name_cn: "术阶丙",
  name_jp: "术阶丙",
  name_en: "Caster C",
  class: "Caster",
  rarity: 5,
};

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

function renderBattlePage(initialProject: Project, servants: Servant[] = [SABER, BERSERKER, CASTER]) {
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
        servants={servants}
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

  it("recovers the start button when startup fails through the status event", async () => {
    let automationHandler: ((event: Event<{ state: string }>) => void) | null = null;
    vi.mocked(listen).mockImplementationOnce(async (event, handler) => {
      if (event === "automation-status") {
        automationHandler = handler as (event: Event<{ state: string }>) => void;
      }
      return () => {};
    });
    const user = userEvent.setup();
    mockProjectCommands();
    renderBattlePage(PROJECT);

    await user.click(await screen.findByRole("button", { name: "开始" }));
    expect(screen.getByRole("button", { name: "停止" })).toBeEnabled();

    act(() => {
      automationHandler?.({
        event: "automation-status",
        id: 0,
        payload: { state: 'Error { message: "no device found" }' },
      } as Event<{ state: string }>);
    });

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "开始" })).toBeEnabled();
    });
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

  it("passes grand support craft essence requirements to automation", async () => {
    const user = userEvent.setup();
    mockProjectCommands();
    renderBattlePage({
      ...PROJECT,
      supportGrandMode: true,
      supportGrandCraftEssenceIds: [1001, null, 1003],
    });

    await user.click(await screen.findByRole("button", { name: "开始" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("start_automation", {
        config: expect.objectContaining({
          supportGrandMode: true,
          supportGrandCraftEssenceIds: [1001, null, 1003],
        }),
      });
    });
  });

  it("blocks grand battle start until at least one grand servant is selected", async () => {
    const user = userEvent.setup();
    mockProjectCommands();
    const callbacks = renderBattlePage({
      ...PROJECT,
      advancedMode: true,
      grandServants: [],
    });

    await user.click(await screen.findByRole("button", { name: "开始" }));

    expect(screen.getByText("无法开始战斗")).toBeInTheDocument();
    expect(screen.getByText("戴冠战需要选择 1 到 2 名冠位从者")).toBeInTheDocument();
    expect(callbacks.onAutomationStart).not.toHaveBeenCalled();
    expect(vi.mocked(invoke).mock.calls.some(([cmd]) => cmd === "start_automation")).toBe(false);
  });

  it("defaults legacy grand battle projects to saber without blocking other classes", async () => {
    const user = userEvent.setup();
    mockProjectCommands();
    renderBattlePage({
      ...PROJECT,
      advancedMode: true,
      grandServants: [{ slotIndex: 0, npCard: "auto", priority: "damage" }],
      slots: [
        { id: "slot-0", type: "servant", servantId: CASTER.id },
        { id: "slot-1", type: "servant", servantId: null },
        { id: "slot-2", type: "support", servantId: null },
        { id: "slot-3", type: "servant", servantId: null },
        { id: "slot-4", type: "servant", servantId: null },
        { id: "slot-5", type: "servant", servantId: null },
      ],
    });

    await user.click(await screen.findByRole("button", { name: "开始" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("start_automation", {
        config: expect.objectContaining({
          grandClass: "saber",
          grandServants: [{ slotIndex: 0, npCard: "auto", priority: "damage" }],
        }),
      });
    });
  });

  it("starts berserker grand battle even when a grand servant slot is not berserker", async () => {
    const user = userEvent.setup();
    mockProjectCommands();
    renderBattlePage({
      ...PROJECT,
      advancedMode: true,
      grandClass: "berserker",
      grandServants: [{ slotIndex: 0, npCard: "auto", priority: "damage" }],
      slots: [
        { id: "slot-0", type: "servant", servantId: SABER.id },
        { id: "slot-1", type: "servant", servantId: null },
        { id: "slot-2", type: "support", servantId: null },
        { id: "slot-3", type: "servant", servantId: null },
        { id: "slot-4", type: "servant", servantId: null },
        { id: "slot-5", type: "servant", servantId: null },
      ],
    });

    await user.click(await screen.findByRole("button", { name: "开始" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("start_automation", {
        config: expect.objectContaining({
          grandClass: "berserker",
          grandServants: [{ slotIndex: 0, npCard: "auto", priority: "damage" }],
        }),
      });
    });
  });

  it("starts berserker grand battle when all selected servants match", async () => {
    const user = userEvent.setup();
    mockProjectCommands();
    renderBattlePage({
      ...PROJECT,
      advancedMode: true,
      grandClass: "berserker",
      supportServantId: BERSERKER.id,
      grandServants: [{ slotIndex: 0, npCard: "auto", priority: "damage" }],
      slots: [
        { id: "slot-0", type: "servant", servantId: BERSERKER.id },
        { id: "slot-1", type: "servant", servantId: null },
        { id: "slot-2", type: "support", servantId: null },
        { id: "slot-3", type: "servant", servantId: null },
        { id: "slot-4", type: "servant", servantId: null },
        { id: "slot-5", type: "servant", servantId: null },
      ],
    });

    await user.click(await screen.findByRole("button", { name: "开始" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("start_automation", {
        config: expect.objectContaining({
          grandClass: "berserker",
          grandServants: [{ slotIndex: 0, npCard: "auto", priority: "damage" }],
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

  it("marks automation stopped immediately when stop is requested", async () => {
    const user = userEvent.setup();
    mockProjectCommands();
    const callbacks = renderBattlePage(PROJECT);

    await user.click(await screen.findByRole("button", { name: "开始" }));
    await user.click(screen.getByRole("button", { name: "停止" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("stop_automation");
      expect(callbacks.onLogEntry).toHaveBeenCalledWith("已请求停止自动化");
      expect(screen.getByRole("button", { name: "返回" })).not.toBeDisabled();
    });
  });
});
