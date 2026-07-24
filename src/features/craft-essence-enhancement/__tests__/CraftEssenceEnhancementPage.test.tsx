import { describe, expect, it, vi } from "vitest";
import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { listen, type Event } from "@tauri-apps/api/event";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { CraftEssenceEnhancementPage } from "../CraftEssenceEnhancementPage";

describe("CraftEssenceEnhancementPage", () => {
  it("starts and stops the independent automation commands", async () => {
    const user = userEvent.setup();
    renderWithTheme(<CraftEssenceEnhancementPage onBack={() => {}} />);

    await user.click(screen.getByRole("button", { name: "开始" }));
    expect(invoke).toHaveBeenCalledWith(
      "start_craft_essence_enhancement_automation"
    );

    await user.click(screen.getByRole("button", { name: "停止" }));
    expect(invoke).toHaveBeenCalledWith(
      "stop_craft_essence_enhancement_automation"
    );
  });

  it("returns to idle controls after a terminal event", async () => {
    let handler:
      | ((event: Event<{ status: "finished"; currentScreen: string; message: string }>) => void)
      | null = null;
    vi.mocked(listen).mockImplementationOnce(async (event, callback) => {
      if (event === "craft-essence-enhancement-automation-status") {
        handler = callback as typeof handler;
      }
      return () => {};
    });
    const user = userEvent.setup();
    renderWithTheme(<CraftEssenceEnhancementPage onBack={() => {}} />);
    await user.click(screen.getByRole("button", { name: "开始" }));

    act(() => {
      handler?.({
        event: "craft-essence-enhancement-automation-status",
        id: 0,
        payload: {
          status: "finished",
          currentScreen: "CraftEssenceEnhancement",
          message: "本阶段完成",
        },
      } as Event<{ status: "finished"; currentScreen: string; message: string }>);
    });

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "开始" })).toBeEnabled();
    });
    expect(screen.getByText("本阶段完成")).toBeInTheDocument();
  });

  it("recovers when startup fails", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error("unsupported server"));
    const user = userEvent.setup();
    renderWithTheme(<CraftEssenceEnhancementPage onBack={() => {}} />);

    await user.click(screen.getByRole("button", { name: "开始" }));

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "开始" })).toBeEnabled();
    });
    expect(screen.getByText(/启动失败/)).toBeInTheDocument();
  });
});
