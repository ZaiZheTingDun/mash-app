import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type Event } from "@tauri-apps/api/event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { FriendPointSummonPage } from "../FriendPointSummonPage";

describe("FriendPointSummonPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("starts and stops the friendship point automation", async () => {
    renderWithTheme(<FriendPointSummonPage onBack={vi.fn()} />);

    fireEvent.click(screen.getByRole("button", { name: "开始友情点抽取" }));
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("start_friend_point_summon_automation");
    });

    fireEvent.click(screen.getByRole("button", { name: "停止" }));
    expect(invoke).toHaveBeenCalledWith("stop_friend_point_summon_automation");
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
    fireEvent.click(screen.getByRole("button", { name: "开始友情点抽取" }));

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

    expect(await screen.findByText("已完成 2 批，共 200 次召唤")).toBeInTheDocument();

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
      expect(screen.getByRole("button", { name: "开始友情点抽取" })).toBeEnabled();
    });
  });
});
