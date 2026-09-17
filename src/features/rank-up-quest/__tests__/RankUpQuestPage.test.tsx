import { describe, expect, it, vi } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
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

function renderPage() {
  const callbacks = {
    onProjectSelect: vi.fn(),
    onBack: vi.fn(),
    onAutomationStart: vi.fn(),
    onAutomationStartFailed: vi.fn(),
    onLogEntry: vi.fn(),
  };
  renderWithTheme(
    <RankUpQuestPage
      projects={[PROJECT]}
      activeProjectId={PROJECT.id}
      {...callbacks}
    />,
  );
  return callbacks;
}

describe("RankUpQuestPage", () => {
  it("refreshes the screenshot and only allows selecting an actionable row", async () => {
    mockCommands();
    const user = userEvent.setup();
    renderPage();

    await user.click(await screen.findByRole("button", { name: "截取游戏画面" }));

    expect(screen.getByRole("button", { name: "选择可强化任务" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "不可点击的强化任务" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "开始强化任务" })).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "选择可强化任务" }));
    expect(screen.getByRole("button", { name: "开始强化任务" })).toBeEnabled();
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
    await user.click(screen.getByRole("button", { name: "开始强化任务" }));

    expect(invoke).toHaveBeenCalledWith(
      "start_rank_up_quest_automation",
      expect.objectContaining({
        workflow: { mode: "all", captureId: null, candidateId: null },
      }),
    );
  });

  it("disables the workflow on the Japanese server", async () => {
    vi.mocked(invoke).mockImplementation(async (command: string) =>
      command === "get_server" ? "JP" : null,
    );
    renderPage();

    expect(await screen.findByText("强化任务自动化首版仅支持国服。")).toBeVisible();
    expect(screen.getByRole("button", { name: "开始强化任务" })).toBeDisabled();
  });
});
