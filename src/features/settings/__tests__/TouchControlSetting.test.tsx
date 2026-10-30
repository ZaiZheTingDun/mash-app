import { act, screen, waitFor } from "@testing-library/react";
import { Theme } from "@radix-ui/themes";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { invoke } from "../../../tauri";
import { renderWithTheme } from "../../../test/renderWithTheme";
import type { TouchControlMode } from "../../../types/deviceControl";
import { TouchControlSetting } from "../TouchControlSetting";

vi.mock("../../../tauri", () => ({ invoke: vi.fn() }));

describe("TouchControlSetting", () => {
  it.each(["light", "dark"] as const)("saves the legacy control and reloads it in %s mode", async (appearance) => {
    let mode: TouchControlMode = "scrcpy";
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_touch_control_mode") return mode;
      if (command === "set_touch_control_mode") {
        mode = (args as { value: TouchControlMode }).value;
        return mode;
      }
      throw new Error(`unexpected command ${command}`);
    });
    const user = userEvent.setup();
    const view = () => <Theme appearance={appearance}><TouchControlSetting active /></Theme>;
    const { unmount } = renderWithTheme(view());
    const select = await screen.findByRole("combobox", { name: "触控方式" });
    await waitFor(() => expect(select).toHaveTextContent("scrcpy"));
    await user.click(select);
    await user.click(screen.getByRole("option", { name: "ADB Input（旧版）" }));
    expect(await screen.findByRole("status")).toHaveTextContent("下次任务开始生效");
    expect(invoke).toHaveBeenCalledWith("set_touch_control_mode", { value: "adb" });
    unmount();
    renderWithTheme(view());
    await waitFor(() => expect(screen.getByRole("combobox", { name: "触控方式" })).toHaveTextContent("ADB Input（旧版）"));
    await user.click(screen.getByRole("combobox", { name: "触控方式" }));
    await user.click(screen.getByRole("option", { name: "scrcpy" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("set_touch_control_mode", { value: "scrcpy" }));
  });

  it("waits for the backend save and retains the previous mode on failure", async () => {
    let fail!: (error: Error) => void;
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_touch_control_mode") return "scrcpy";
      return new Promise((_, reject) => { fail = reject; });
    });
    const user = userEvent.setup();
    renderWithTheme(<TouchControlSetting active />);
    const select = screen.getByRole("combobox", { name: "触控方式" });
    await waitFor(() => expect(select).toHaveTextContent("scrcpy"));
    await user.click(select);
    await user.click(screen.getByRole("option", { name: "ADB Input（旧版）" }));
    expect(select).toBeDisabled();
    expect(select).toHaveTextContent("scrcpy");
    await act(async () => fail(new Error("保存失败")));
    expect(await screen.findByRole("alert")).toHaveTextContent("保存失败");
    expect(select).not.toBeDisabled();
    expect(select).toHaveTextContent("scrcpy");
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("retries a failed read without enabling an unknown selection", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error("读取失败")).mockResolvedValue("adb");
    const user = userEvent.setup();
    renderWithTheme(<TouchControlSetting active />);
    expect(await screen.findByRole("alert")).toHaveTextContent("读取失败");
    expect(screen.getByRole("combobox", { name: "触控方式" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "重试读取" }));
    await waitFor(() => expect(screen.getByRole("combobox", { name: "触控方式" })).toHaveTextContent("ADB Input（旧版）"));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});
