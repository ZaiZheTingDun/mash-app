import { describe, expect, it, vi } from "vitest";
import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { listen, type Event } from "@tauri-apps/api/event";
import { useState } from "react";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { BattlePage } from "../BattlePage";
import type { Project } from "../../../types/project";
import type { GrandClassDefinition } from "../../../types/project";
import type { BattleRunStatus } from "../../../types/battleRunStatus";

const GRAND_CLASS_DEFINITIONS: GrandClassDefinition[] = [
  {
    id: "saber", label: "剑阶冠位", servantClass: "Saber",
    roles: [{ role: "main", label: "主", required: true }, { role: "deputy", label: "副", required: false }],
    cardPriorityEnabled: true, autoOrderChangeRoles: ["main"], validationMessage: "戴冠战需要选择 1 到 2 名冠位从者",
  },
  {
    id: "lancer", label: "枪阶冠位", servantClass: "Lancer",
    roles: [{ role: "single", label: "单体", required: true }, { role: "aoe", label: "光炮", required: true }],
    cardPriorityEnabled: false, autoOrderChangeRoles: ["single", "aoe"], validationMessage: "枪阶戴冠战需要分别选择单体和光炮从者",
  },
  {
    id: "berserker", label: "狂阶冠位", servantClass: "Berserker",
    roles: [{ role: "main", label: "主", required: true }, { role: "deputy", label: "副", required: false }],
    cardPriorityEnabled: true, autoOrderChangeRoles: ["main"], validationMessage: "戴冠战需要选择 1 到 2 名冠位从者",
  },
];
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

function renderBattlePage(
  initialProject: Project,
  servants: Servant[] = [SABER, BERSERKER, CASTER],
  battleRunStatus: BattleRunStatus | null = null,
) {
  const callbacks = {
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
        projectCatalog={{
          schemaVersion: 1,
          groups: [],
          ungroupedProjectIds: projects.map((project) => project.id),
        }}
        grandClassDefinitions={GRAND_CLASS_DEFINITIONS}
        servants={servants}
        activeProjectId={activeProjectId}
        onProjectSelect={setActiveProjectId}
        onCreateProject={callbacks.onCreateProject}
        onRenameProject={callbacks.onRenameProject}
        onDuplicateProject={callbacks.onDuplicateProject}
        onDeleteProject={callbacks.onDeleteProject}
        onCreateProjectGroup={callbacks.onCreateProjectGroup}
        onRenameProjectGroup={callbacks.onRenameProjectGroup}
        onDeleteProjectGroup={callbacks.onDeleteProjectGroup}
        onMoveProjectToGroup={callbacks.onMoveProjectToGroup}
        onReorderProjectGroups={callbacks.onReorderProjectGroups}
        onReorderProjectsInGroup={callbacks.onReorderProjectsInGroup}
        onOpenProjectSettings={callbacks.onOpenProjectSettings}
        onUpdateProject={handleUpdateProject}
        onBack={callbacks.onBack}
        onAutomationStart={callbacks.onAutomationStart}
        onLogEntry={callbacks.onLogEntry}
        battleRunStatus={battleRunStatus}
      />
    );
  }

  renderWithTheme(<Harness />);
  return callbacks;
}

describe("BattlePage", () => {
  it("explains AP recovery limits from the heading help icon", async () => {
    const user = userEvent.setup();
    mockProjectCommands();
    renderBattlePage(PROJECT);

    await user.hover(await screen.findByRole("button", { name: "行动力恢复说明" }));

    expect(await screen.findByRole("tooltip")).toHaveTextContent(
      "某种道具达到上限后会继续尝试其他已选道具"
    );
  });

  it("starts automation with ordered apples and repeat count", async () => {
    const user = userEvent.setup();
    const project = {
      ...PROJECT,
      repeatMission: true,
      repeatMode: "count",
      repeatCount: 100,
      apRecoveryItems: ["rainbow", "copper", "gold"],
      apRecoveryLimits: { rainbow: 3, copper: null, gold: 2 },
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
          apRecoveryLimits: expect.objectContaining({
            gold: 2,
            copper: null,
            rainbow: 3,
          }),
          supportCraftEssenceIds: [1485],
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
    let automationHandler: ((event: Event<{ status: "error" }>) => void) | null = null;
    vi.mocked(listen).mockImplementationOnce(async (event, handler) => {
      if (event === "automation-status") {
        automationHandler = handler as (event: Event<{ status: "error" }>) => void;
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
        payload: { status: "error" },
      } as Event<{ status: "error" }>);
    });

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "开始" })).toBeEnabled();
    });
  });

  it("passes support score, skill, and NP requirements to automation", async () => {
    const user = userEvent.setup();
    mockProjectCommands();
    renderBattlePage({
      ...PROJECT,
      supportStarMapScoreMin: 62,
      supportGrandStarMapScoreMin: 16,
      supportNoblePhantasmLevelMin: 2,
      supportSkillLevelMins: [10, null, 9],
      supportAppendSkillLevelMins: [null, 10, null, null, 6],
    });

    await user.click(await screen.findByRole("button", { name: "开始" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("start_automation", {
        config: expect.objectContaining({
          supportStarMapScoreMin: 62,
          supportGrandStarMapScoreMin: 16,
          supportNoblePhantasmLevelMin: 2,
          supportSkillLevelMins: [10, null, 9],
          supportAppendSkillLevelMins: [null, 10, null, null, 6],
        }),
      });
    });
  });

  it("passes the selected support servant variant to automation", async () => {
    const user = userEvent.setup();
    mockProjectCommands();
    renderBattlePage({
      ...PROJECT,
      supportServantId: 444,
      supportServantVariantKey: "444:1",
    });

    await user.click(await screen.findByRole("button", { name: "开始" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("start_automation", {
        config: expect.objectContaining({
          supportServantId: 444,
          supportServantVariantKey: "444:1",
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
      supportGrandCraftEssenceIdLists: [[1001, 1002], [], [1003]],
    });

    await user.click(await screen.findByRole("button", { name: "开始" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("start_automation", {
        config: expect.objectContaining({
          supportGrandMode: true,
          supportGrandCraftEssenceIds: [1001, null, 1003],
          supportGrandCraftEssenceIdLists: [[1001, 1002], [], [1003]],
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

  it("requires both single and aoe roles before starting a lancer grand battle", async () => {
    const user = userEvent.setup();
    mockProjectCommands();
    const callbacks = renderBattlePage({
      ...PROJECT,
      advancedMode: true,
      grandClass: "lancer",
      grandServants: [
        { slotIndex: 0, npCard: "auto", priority: "damage", lancerRole: "single" },
      ],
    });

    await user.click(await screen.findByRole("button", { name: "开始" }));

    expect(screen.getByText("枪阶戴冠战需要分别选择单体和光炮从者")).toBeInTheDocument();
    expect(callbacks.onAutomationStart).not.toHaveBeenCalled();
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

  it("shows run progress in the count control and footer while running", async () => {
    const user = userEvent.setup();
    mockProjectCommands();
    renderBattlePage(
      {
        ...PROJECT,
        repeatMission: true,
        repeatMode: "count",
        repeatCount: 10,
      },
      [SABER, BERSERKER, CASTER],
      {
        phase: "running",
        startedAtMs: 0,
        endedAtMs: null,
        lastCompletedAtMs: 1_000,
        completedRuns: 1,
        maxRuns: 10,
        apRecoveryUsage: { gold: 0, silver: 0, bronze: 0, copper: 0, rainbow: 0 },
      },
    );

    await user.click(await screen.findByRole("button", { name: "开始" }));

    expect(screen.queryByRole("spinbutton", { name: "重复次数" })).not.toBeInTheDocument();
    expect(screen.getByText("1/10")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "开始" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "停止" })).toBeEnabled();
  });

  it("opens advanced loot settings and persists five-star CE drop options", async () => {
    const user = userEvent.setup();
    mockProjectCommands();
    renderBattlePage(PROJECT);

    await user.click(await screen.findByRole("button", { name: "高级设置" }));

    expect(await screen.findByRole("dialog")).toHaveTextContent("战利品掉落");
    await user.click(screen.getByRole("switch", { name: "五星礼装掉落自动停止" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("update_project", {
        project: expect.objectContaining({
          id: "project-1",
          recognitionSettings: expect.objectContaining({
            stopOnFiveStarCeDrop: true,
            fiveStarCeDropTargetCount: 1,
          }),
        }),
      });
    });

    expect(await screen.findByRole("spinbutton", { name: "五星礼装掉落个数" })).toHaveValue(1);
    await user.click(screen.getByRole("button", { name: "增加五星礼装掉落个数" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("update_project", {
        project: expect.objectContaining({
          id: "project-1",
          recognitionSettings: expect.objectContaining({
            stopOnFiveStarCeDrop: true,
            fiveStarCeDropTargetCount: 2,
          }),
        }),
      });
    });
  });

  it("passes enabled five-star CE drop stop settings to automation", async () => {
    const user = userEvent.setup();
    mockProjectCommands();
    renderBattlePage({
      ...PROJECT,
      recognitionSettings: {
        stopOnFiveStarCeDrop: true,
        fiveStarCeDropTargetCount: 3,
      },
    });

    await user.click(await screen.findByRole("button", { name: "开始" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("start_automation", {
        config: expect.objectContaining({
          stopOnFiveStarCeDrop: true,
          fiveStarCeDropTargetCount: 3,
        }),
      });
    });
  });

  it("passes disabled five-star CE drop stop settings to automation by default", async () => {
    const user = userEvent.setup();
    mockProjectCommands();
    renderBattlePage(PROJECT);

    await user.click(await screen.findByRole("button", { name: "开始" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("start_automation", {
        config: expect.objectContaining({
          stopOnFiveStarCeDrop: false,
          fiveStarCeDropTargetCount: 1,
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

  it("shows and persists a per-apple limit with unlimited as the default", async () => {
    const user = userEvent.setup();
    mockProjectCommands();
    renderBattlePage({
      ...PROJECT,
      apRecoveryItems: ["gold"],
    });

    const unlimited = await screen.findByRole("button", {
      name: "黄金果实当前无限使用，点击设置数量",
    });
    expect(unlimited).toHaveClass("is-active");
    expect(
      screen.queryByRole("spinbutton", { name: "黄金果实使用数量" })
    ).not.toBeInTheDocument();

    await user.click(unlimited);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("update_project", {
        project: expect.objectContaining({
          apRecoveryLimits: expect.objectContaining({ gold: 1 }),
        }),
      });
    });
    const quantity = await screen.findByRole("spinbutton", {
      name: "黄金果实使用数量",
    });
    expect(
      screen.getByRole("button", { name: "黄金果实当前限量使用，点击改为无限" })
    ).not.toHaveClass("is-active");
    expect(quantity).toBeEnabled();
    expect(quantity).toHaveValue(1);

    await user.click(screen.getByRole("button", { name: "增加黄金果实使用数量" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("update_project", {
        project: expect.objectContaining({
          apRecoveryLimits: expect.objectContaining({ gold: 2 }),
        }),
      });
    });
    expect(quantity).toHaveValue(2);
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
