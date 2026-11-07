import { describe, expect, it, vi } from "vitest";
import type { ComponentProps } from "react";
import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { listen, type Event } from "@tauri-apps/api/event";
import { renderWithTheme } from "../../../test/renderWithTheme";
import type { Project } from "../../../types/project";
import { RankUpQuestPage } from "../RankUpQuestPage";

const PROJECT: Project = {
  id: "team-1",
  name: "强化队伍",
  supportServantId: null,
  supportServantVariantKey: null,
  repeatMission: true,
  repeatMode: "count",
  repeatCount: 99,
  apRecoveryItems: ["gold"],
  apRecoveryLimits: { gold: 2 },
  slots: [
    { id: "owned-1", type: "servant", servantId: 1 },
    { id: "support-1", type: "support", servantId: null },
  ],
};

function mockCommands() {
  vi.mocked(invoke).mockImplementation(async (command: string) => {
    if (command === "get_server") return "CN";
    if (command === "capture_rank_up_quest_page") {
      return {
        captureId: "capture-1",
        imagePath: "/tmp/rank-up.png",
        rows: [
          {
            candidateId: "capture-1:0",
            region: { x: 0.5, y: 0.2, w: 0.4, h: 0.18 },
            rankUpAnchor: { x: 0.61, y: 0.24, w: 0.06, h: 0.04, score: 0.95 },
            costAnchor: { x: 0.61, y: 0.3, w: 0.04, h: 0.03, score: 0.94 },
            signatureRegions: [
              { x: 0.51, y: 0.21, w: 0.09, h: 0.11 },
              { x: 0.62, y: 0.21, w: 0.25, h: 0.06 },
              { x: 0.94, y: 0.25, w: 0.03, h: 0.08 },
            ],
            actionable: true,
            anchorScore: 0.94,
            meanLuma: 145,
            meanSaturation: 100,
            meanValue: 170,
          },
          {
            candidateId: "capture-1:1",
            region: { x: 0.5, y: 0.42, w: 0.4, h: 0.18 },
            rankUpAnchor: null,
            costAnchor: { x: 0.61, y: 0.52, w: 0.04, h: 0.03, score: 0.9 },
            signatureRegions: [
              { x: 0.51, y: 0.43, w: 0.09, h: 0.11 },
              { x: 0.62, y: 0.43, w: 0.25, h: 0.06 },
              { x: 0.94, y: 0.47, w: 0.03, h: 0.08 },
            ],
            actionable: false,
            anchorScore: 0.9,
            meanLuma: 72,
            meanSaturation: 54,
            meanValue: 84,
          },
        ],
      };
    }
    return null;
  });
}

function renderPage(overrides: Partial<ComponentProps<typeof RankUpQuestPage>> = {}) {
  const callbacks = {
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
    onBack: vi.fn(),
    onAutomationStart: vi.fn(),
    onAutomationStartFailed: vi.fn(),
    onLogEntry: vi.fn(),
  };
  renderWithTheme(
    <RankUpQuestPage
      server="CN"
      projects={[PROJECT]}
      projectCatalog={{ schemaVersion: 1, groups: [], ungroupedProjectIds: [PROJECT.id] }}
      activeProjectId={PROJECT.id}
      {...callbacks}
      {...overrides}
    />,
  );
  return callbacks;
}

describe("RankUpQuestPage", () => {
  it("shows the shared header with team selection and a footer status", async () => {
    mockCommands();
    const user = userEvent.setup();
    const callbacks = renderPage();

    expect(screen.getByRole("heading", { name: "强化任务", level: 1 })).toBeInTheDocument();
    expect(within(screen.getByRole("banner")).getByRole("button", { name: PROJECT.name })).toBeInTheDocument();
    expect(within(screen.getByRole("banner")).getByRole("button", { name: "队伍操作" })).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("请截图并选择强化任务");
    await user.click(screen.getByRole("button", { name: "返回" }));
    expect(callbacks.onBack).toHaveBeenCalledOnce();
  });

  it("uses the formation selector's grouped search and selection callbacks", async () => {
    mockCommands();
    const user = userEvent.setup();
    const secondProject = { ...PROJECT, id: "team-2", name: "第二强化队伍" };
    const callbacks = renderPage({
      projects: [PROJECT, secondProject],
      projectCatalog: {
        schemaVersion: 1,
        groups: [{ id: "group-1", name: "强化分组", projectIds: [secondProject.id] }],
        ungroupedProjectIds: [PROJECT.id],
      },
    });

    await user.click(screen.getByRole("button", { name: PROJECT.name }));
    expect(screen.getByRole("button", { name: /强化分组/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "管理所有队伍…" })).toBeInTheDocument();
    await user.type(screen.getByRole("textbox", { name: "搜索队伍" }), "第二");
    await user.click(screen.getByRole("button", { name: /第二强化队伍/ }));
    expect(callbacks.onProjectSelect).toHaveBeenCalledWith(secondProject.id);
  });

  it("opens the shared party operations menu", async () => {
    mockCommands();
    const user = userEvent.setup();
    const callbacks = renderPage();
    await user.click(screen.getByRole("button", { name: "队伍操作" }));
    await user.click(screen.getByRole("menuitem", { name: "队伍设置" }));
    expect(callbacks.onOpenProjectSettings).toHaveBeenCalledOnce();
  });

  it.each([null, "missing-team"])("does not run an unselected fallback party for %s", async (activeProjectId) => {
    mockCommands();
    const user = userEvent.setup();
    renderPage({ activeProjectId });
    expect(screen.getByRole("button", { name: "选择队伍" })).toBeEnabled();
    await user.click(screen.getByRole("radio", { name: "按顺序完成所有强化任务" }));
    expect(screen.getByRole("status")).toHaveTextContent("请选择战斗队伍");
    expect(screen.getByRole("button", { name: "开始强化任务" })).toBeDisabled();
  });

  it("refreshes the screenshot and only allows selecting an actionable row", async () => {
    mockCommands();
    const user = userEvent.setup();
    renderPage();

    await user.click(await screen.findByRole("button", { name: "截取游戏画面" }));

    expect(screen.getByRole("button", { name: "选择可强化任务" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "不可点击的强化任务" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "开始强化任务" })).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "选择可强化任务" }));
    expect(screen.getByRole("button", { name: "选择可强化任务", pressed: true })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "开始强化任务" })).toBeEnabled();
    expect(screen.getByRole("status")).toHaveTextContent("已选择强化任务，可以开始");
  });

  it("starts single mode with candidate identity only and ignores project repeat count", async () => {
    mockCommands();
    const user = userEvent.setup();
    renderPage();

    await user.click(await screen.findByRole("button", { name: "截取游戏画面" }));
    await user.click(screen.getByRole("button", { name: "选择可强化任务" }));
    await user.click(screen.getByRole("button", { name: "开始强化任务" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("start_rank_up_quest_automation", {
        config: expect.objectContaining({
          projectId: "team-1",
          repeatMission: false,
          maxMissionRuns: null,
          apRecoveryItems: ["gold"],
          apRecoveryLimits: expect.objectContaining({ gold: 2 }),
        }),
        workflow: {
          mode: "single",
          captureId: "capture-1",
          candidateId: "capture-1:0",
        },
      });
    });
    const call = vi.mocked(invoke).mock.calls.find(
      ([command]) => command === "start_rank_up_quest_automation",
    );
    expect(call?.[1]).not.toHaveProperty("workflow.x");
    expect(call?.[1]).not.toHaveProperty("workflow.region");
    expect(screen.getByRole("button", { name: "停止" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "返回" })).toBeDisabled();
  });

  it("allows all mode to start without capturing or selecting a row", async () => {
    mockCommands();
    const user = userEvent.setup();
    renderPage();

    await user.click(await screen.findByText("按顺序完成所有强化任务"));
    expect(screen.getByRole("status")).toHaveTextContent("就绪，将按列表顺序执行");
    expect(screen.queryByRole("button", { name: "截取游戏画面" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "开始强化任务" }));

    expect(invoke).toHaveBeenCalledWith(
      "start_rank_up_quest_automation",
      expect.objectContaining({
        workflow: { mode: "all", captureId: null, candidateId: null },
      }),
    );
  });

  it("disables the workflow on the Japanese server", async () => {
    renderPage({ server: "JP" });

    expect(await screen.findByRole("alert")).toHaveTextContent("强化任务自动化仅支持国服。");
    expect(screen.getByRole("button", { name: "开始强化任务" })).toBeDisabled();
  });

  it("keeps actions disabled until the backend server is known", () => {
    renderPage({ server: null });
    expect(screen.getByRole("status")).toHaveTextContent("正在读取服务器…");
    expect(screen.getByRole("button", { name: "截取游戏画面" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "开始强化任务" })).toBeDisabled();
  });

  it("shows capture errors in the footer and allows retrying", async () => {
    vi.mocked(invoke).mockImplementation(async (command: string) => {
      if (command === "get_server") return "CN";
      if (command === "capture_rank_up_quest_page") throw new Error("设备未连接");
      return null;
    });
    const user = userEvent.setup();
    renderPage();

    await user.click(await screen.findByRole("button", { name: "截取游戏画面" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("设备未连接");
    expect(screen.getByRole("button", { name: "截取游戏画面" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "开始强化任务" })).toBeDisabled();
  });

  it("shows running status and restores controls after stopping", async () => {
    mockCommands();
    type Payload = { status: "running" | "idle"; currentScreen: string; message: string };
    let eventHandler: ((event: Event<Payload>) => void) | undefined;
    vi.mocked(listen).mockImplementation(async (event, handler) => {
      if (event === "automation-status") eventHandler = handler as typeof eventHandler;
      return () => {};
    });
    const user = userEvent.setup();
    renderPage();
    await user.click(await screen.findByText("按顺序完成所有强化任务"));
    await user.click(screen.getByRole("button", { name: "开始强化任务" }));
    expect(screen.getByRole("status")).toHaveTextContent("当前页面：正在启动");
    expect(screen.getByRole("button", { name: PROJECT.name })).toBeDisabled();
    expect(screen.getByRole("button", { name: "队伍操作" })).toBeDisabled();

    act(() => eventHandler?.({
      event: "automation-status", id: 1,
      payload: { status: "running", currentScreen: "RankUpQuest", message: "执行强化任务" },
    }));
    expect(screen.getByRole("status")).toHaveTextContent("当前页面：RankUpQuest");
    await user.click(screen.getByRole("button", { name: "停止" }));
    expect(invoke).toHaveBeenCalledWith("stop_automation");
    act(() => eventHandler?.({
      event: "automation-status", id: 2,
      payload: { status: "idle", currentScreen: "RankUpQuest", message: "已停止" },
    }));
    expect(screen.getByRole("button", { name: "开始强化任务" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "返回" })).toBeEnabled();
    expect(screen.getByRole("button", { name: PROJECT.name })).toBeEnabled();
    expect(screen.getByRole("button", { name: "队伍操作" })).toBeEnabled();
  });
});
