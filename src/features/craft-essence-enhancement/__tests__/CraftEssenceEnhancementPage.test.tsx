import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { listen, type Event } from "@tauri-apps/api/event";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { CraftEssenceEnhancementPage } from "../CraftEssenceEnhancementPage";

describe("CraftEssenceEnhancementPage", () => {
  beforeEach(() => vi.clearAllMocks());

  it("shows the shared page layout and selects a strategy before starting", async () => {
    const user = userEvent.setup();
    const onBack = vi.fn();
    renderWithTheme(<CraftEssenceEnhancementPage onBack={onBack} />);

    expect(screen.getByRole("heading", { name: "强化概念礼装", level: 1 })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "素材规则" })).toHaveTextContent("程序不会解锁任何礼装");
    expect(screen.getByRole("radio", { name: "节省 QP 策略" })).toBeChecked();
    expect(screen.getByRole("status")).toHaveTextContent("当前页面：等待启动");

    await user.click(screen.getByRole("radio", { name: "快速策略" }));
    expect(screen.getByRole("radio", { name: "快速策略" })).toBeChecked();
    expect(invoke).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "返回" }));
    expect(onBack).toHaveBeenCalledOnce();
  });

  it("starts and stops the independent automation commands", async () => {
    const user = userEvent.setup();
    const onAutomationStart = vi.fn();
    renderWithTheme(<CraftEssenceEnhancementPage onBack={() => {}} onAutomationStart={onAutomationStart} />);

    await user.click(
      screen.getByRole("button", { name: "开始制作丸子" })
    );
    expect(invoke).toHaveBeenCalledWith(
      "start_craft_essence_enhancement_automation",
      { mode: "qpEfficient" }
    );
    expect(onAutomationStart).toHaveBeenCalledOnce();
    expect(screen.getByRole("radio", { name: "快速策略" })).toBeDisabled();
    expect(screen.getByRole("radio", { name: "节省 QP 策略" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "返回" })).toBeDisabled();
    expect(screen.getByRole("status")).toHaveTextContent("当前页面：正在识别…");

    await user.click(screen.getByRole("button", { name: "停止" }));
    expect(invoke).toHaveBeenCalledWith(
      "stop_craft_essence_enhancement_automation"
    );
  });

  it("starts the fast bomb strategy separately", async () => {
    const user = userEvent.setup();
    renderWithTheme(<CraftEssenceEnhancementPage onBack={() => {}} />);

    await user.click(screen.getByRole("radio", { name: "快速策略" }));
    await user.click(screen.getByRole("button", { name: "开始制作丸子" }));

    expect(invoke).toHaveBeenCalledWith(
      "start_craft_essence_enhancement_automation",
      { mode: "fast" }
    );
  });

  it.each(["finished", "error"] as const)("returns to idle controls after a %s event", async (status) => {
    let handler:
      | ((event: Event<{ status: "finished" | "error"; currentScreen: string; message: string }>) => void)
      | null = null;
    vi.mocked(listen).mockImplementationOnce(async (event, callback) => {
      if (event === "craft-essence-enhancement-automation-status") {
        handler = callback as typeof handler;
      }
      return () => {};
    });
    const user = userEvent.setup();
    renderWithTheme(<CraftEssenceEnhancementPage onBack={() => {}} />);
    await user.click(screen.getByRole("radio", { name: "快速策略" }));
    await user.click(
      screen.getByRole("button", { name: "开始制作丸子" })
    );

    act(() => {
      handler?.({
        event: "craft-essence-enhancement-automation-status",
        id: 0,
        payload: {
          status,
          currentScreen: "CraftEssenceEnhancement",
          message: status === "error" ? "识别失败" : "本阶段完成",
        },
      });
    });

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: "开始制作丸子" })
      ).toBeEnabled();
    });
    expect(screen.getByRole("radio", { name: "快速策略" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "快速策略" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "返回" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "停止" })).not.toBeInTheDocument();
    if (status === "error") {
      expect(screen.getByRole("alert")).toHaveTextContent("识别失败");
    } else {
      expect(screen.getByRole("status")).toHaveTextContent("当前页面：礼装强化");
    }
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
      screen.getByRole("button", { name: "开始制作丸子" })
    );

    await waitFor(() => {
      expect(
        screen.getByRole("button", { name: "开始制作丸子" })
      ).toBeEnabled();
    });
    expect(onLogEntry).toHaveBeenCalledWith(
      "启动失败: Error: unsupported server"
    );
    expect(screen.getByRole("alert")).toHaveTextContent("启动失败: Error: unsupported server");
    expect(screen.getByRole("button", { name: "返回" })).toBeEnabled();

    await user.click(screen.getByRole("button", { name: "开始制作丸子" }));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "停止" })).toBeEnabled();
  });

  it("uses only the shared bottom status log", () => {
    renderWithTheme(<CraftEssenceEnhancementPage onBack={() => {}} />);

    expect(screen.queryByText("运行日志")).not.toBeInTheDocument();
    expect(screen.queryByText("等待启动…")).not.toBeInTheDocument();
  });
});
