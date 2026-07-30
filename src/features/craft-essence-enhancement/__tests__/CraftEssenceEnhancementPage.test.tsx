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

    await user.click(
      screen.getByRole("button", { name: "制作丸子（节省 QP 策略）" })
    );
    expect(invoke).toHaveBeenCalledWith(
      "start_craft_essence_enhancement_automation",
      { mode: "qpEfficient" }
    );

    await user.click(screen.getByRole("button", { name: "停止" }));
    expect(invoke).toHaveBeenCalledWith(
      "stop_craft_essence_enhancement_automation"
    );
  });

  it("starts the fast bomb strategy separately", async () => {
    const user = userEvent.setup();
    renderWithTheme(<CraftEssenceEnhancementPage onBack={() => {}} />);

    await user.click(
      screen.getByRole("button", { name: "制作丸子（快速策略）" })
    );

    expect(invoke).toHaveBeenCalledWith(
      "start_craft_essence_enhancement_automation",
      { mode: "fast" }
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
    await user.click(
      screen.getByRole("button", { name: "制作丸子（节省 QP 策略）" })
    );

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
      expect(
        screen.getByRole("button", { name: "制作丸子（节省 QP 策略）" })
      ).toBeEnabled();
    });
    expect(
      screen.getByText("当前页面：CraftEssenceEnhancement")
    ).toBeInTheDocument();
    expect(screen.queryByText("本阶段完成")).not.toBeInTheDocument();
  });

  it("recovers when startup fails", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error("unsupported server"));
    const onLogEntry = vi.fn();
    const user = userEvent.setup();
    renderWithTheme(
      <CraftEssenceEnhancementPage
        onBack={() => {}}
        onLogEntry={onLogEntry}
      />
    );

    await user.click(
      screen.getByRole("button", { name: "制作丸子（节省 QP 策略）" })
    );

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: "制作丸子（节省 QP 策略）" })
      ).toBeEnabled();
    });
    expect(onLogEntry).toHaveBeenCalledWith(
      "启动失败: Error: unsupported server"
    );
    expect(screen.queryByText(/启动失败/)).not.toBeInTheDocument();
  });

  it("uses only the shared bottom status log", () => {
    renderWithTheme(<CraftEssenceEnhancementPage onBack={() => {}} />);

    expect(screen.queryByText("运行日志")).not.toBeInTheDocument();
    expect(screen.queryByText("等待启动…")).not.toBeInTheDocument();
  });
});
