import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type Event } from "@tauri-apps/api/event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { FriendPointSummonPage } from "../FriendPointSummonPage";

describe("FriendPointSummonPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("shows the shared page header, summon details and initial progress", () => {
    const onBack = vi.fn();
    renderWithTheme(<FriendPointSummonPage onBack={onBack} />);

    expect(screen.getByRole("heading", { name: "友情点召唤", level: 1 })).toBeInTheDocument();
    const mode = screen.getByRole("region", { name: "召唤方式" });
    expect(within(mode).getByText("100 次召唤")).toBeInTheDocument();
    expect(within(mode).getByText("仅支持国服")).toBeInTheDocument();
    const progress = screen.getByRole("region", { name: "召唤进度" });
    expect(within(progress).getByText("已完成批次")).toBeInTheDocument();
    expect(within(progress).getByText("累计召唤")).toBeInTheDocument();
    expect(within(progress).getAllByRole("definition").map((item) => item.textContent)).toEqual(["0批", "0次"]);
    expect(screen.getByRole("status")).toHaveTextContent("当前页面：等待启动");

    fireEvent.click(screen.getByRole("button", { name: "返回" }));
    expect(onBack).toHaveBeenCalledOnce();
  });

  it("starts and stops the friendship point automation", async () => {
    const onAutomationStart = vi.fn();
    renderWithTheme(<FriendPointSummonPage onBack={vi.fn()} onAutomationStart={onAutomationStart} />);

    fireEvent.click(screen.getByRole("button", { name: "开始" }));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("start_friend_point_summon_automation");
    });
    expect(onAutomationStart).toHaveBeenCalledOnce();
    expect(screen.queryByRole("button", { name: "开始" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "返回" })).toBeDisabled();
    expect(screen.getByRole("status")).toHaveTextContent("当前页面：正在识别…");

    fireEvent.click(screen.getByRole("button", { name: "停止" }));
    expect(invoke).toHaveBeenCalledWith("stop_friend_point_summon_automation");
  });

  it("restores the start action and back navigation when startup fails", async () => {
    vi.mocked(invoke).mockRejectedValueOnce("设备未连接");
    const onLogEntry = vi.fn();
    renderWithTheme(<FriendPointSummonPage onBack={vi.fn()} onLogEntry={onLogEntry} />);

    fireEvent.click(screen.getByRole("button", { name: "开始" }));

    expect(await screen.findByRole("button", { name: "开始" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "返回" })).toBeEnabled();
    expect(onLogEntry).toHaveBeenCalledWith("启动失败: 设备未连接");
  });

  it("shows backend-owned progress and unlocks after completion", async () => {
    type Payload = {
      state: string;
      status: "running" | "finished";
      currentScreen: string;
      message: string;
      completedBatches: number;
      summonedCount: number;
    };
    let eventHandler: ((event: Event<Payload>) => void) | undefined;
    vi.mocked(listen).mockImplementation(async (event, handler) => {
      if (event === "friend-point-summon-automation-status") {
        eventHandler = handler as typeof eventHandler;
      }
      return () => {};
    });

    renderWithTheme(<FriendPointSummonPage onBack={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "开始" }));

    await waitFor(() => expect(eventHandler).toBeDefined());
    act(() => {
      eventHandler?.({
        event: "friend-point-summon-automation-status",
        id: 1,
        payload: {
          state: "Running",
          status: "running",
          currentScreen: "FriendPointSummonResult",
          message: "完成第 2 批",
          completedBatches: 2,
          summonedCount: 200,
        },
      });
    });

    const progress = screen.getByRole("region", { name: "召唤进度" });
    expect(within(progress).getAllByRole("definition").map((item) => item.textContent)).toEqual(["2批", "200次"]);
    expect(screen.getByRole("status")).toHaveTextContent("当前页面：召唤结果");

    act(() => {
      eventHandler?.({
        event: "friend-point-summon-automation-status",
        id: 2,
        payload: {
          state: "Finished",
          status: "finished",
          currentScreen: "FriendPointSummonResult",
          message: "完成",
          completedBatches: 2,
          summonedCount: 200,
        },
      });
    });

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "开始" })).toBeEnabled();
    });
    expect(screen.getByRole("button", { name: "返回" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "停止" })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "开始" }));
    expect(within(progress).getAllByRole("definition").map((item) => item.textContent)).toEqual(["0批", "0次"]);
  });
});
