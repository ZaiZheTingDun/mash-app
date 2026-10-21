import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../../test/renderWithTheme";
import type { DeviceResolutionResetResult, DeviceResolutionStatus } from "../../../types/deviceResolution";
import { DeviceResolutionDialog } from "../DeviceResolutionDialog";

const before: DeviceResolutionStatus = {
  serial: "emulator-5554", displayId: 0,
  physicalSize: { width: 1440, height: 2560 },
  overrideSize: { width: 720, height: 1280 },
  effectiveSize: { width: 720, height: 1280 },
};
const after: DeviceResolutionStatus = { ...before, overrideSize: null, effectiveSize: before.physicalSize };

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "get_device_resolution") return before;
    if (command === "reset_device_resolution") return { before, after, verified: true, message: "已恢复设备默认分辨率" };
    throw new Error(`unexpected command: ${command}`);
  });
});

describe("DeviceResolutionDialog", () => {
  it("reads without mutating, then resets the displayed device and shows readback", async () => {
    const onLogEntry = vi.fn();
    renderWithTheme(<DeviceResolutionDialog open onOpenChange={vi.fn()} onLogEntry={onLogEntry} />);
    expect(await screen.findByText("emulator-5554")).toBeInTheDocument();
    expect(screen.getByText("主屏幕（0）")).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith("reset_device_resolution", expect.anything());
    await userEvent.setup().click(screen.getByRole("button", { name: "重置分辨率" }));
    expect(invoke).toHaveBeenCalledWith("reset_device_resolution", { serial: "emulator-5554" });
    expect(await screen.findByRole("status")).toHaveTextContent("已恢复设备默认分辨率");
    expect(screen.getByText("重置前：720 × 1280；重置后：1440 × 2560")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重置分辨率" })).toBeDisabled();
    expect(onLogEntry).toHaveBeenCalledWith("设备 emulator-5554：已恢复设备默认分辨率");
  });

  it("blocks reset when no device can be read and offers refresh", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error("请先在右下角选择 ADB 设备"));
    renderWithTheme(<DeviceResolutionDialog open onOpenChange={vi.fn()} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("请先在右下角选择 ADB 设备");
    expect(screen.getByRole("button", { name: "重置分辨率" })).toBeDisabled();
    await userEvent.setup().click(screen.getByRole("button", { name: "刷新" }));
    expect(await screen.findByText("emulator-5554")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重置分辨率" })).toBeEnabled();
  });

  it("invalidates the displayed device after a failed or changed-selection reset", async () => {
    renderWithTheme(<DeviceResolutionDialog open onOpenChange={vi.fn()} />);
    await screen.findByText("emulator-5554");
    vi.mocked(invoke).mockRejectedValueOnce(new Error("所选 ADB 设备已变化，请刷新后重试"));
    await userEvent.setup().click(screen.getByRole("button", { name: "重置分辨率" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("所选 ADB 设备已变化");
    expect(screen.getByRole("button", { name: "重置分辨率" })).toBeDisabled();
  });

  it.each([before, null])("does not report success when readback is unverified (%s)", async (readback) => {
    renderWithTheme(<DeviceResolutionDialog open onOpenChange={vi.fn()} />);
    await screen.findByText("emulator-5554");
    vi.mocked(invoke).mockResolvedValueOnce({ before, after: readback, verified: false, message: "重置命令已执行，但无法确认结果" });
    await userEvent.setup().click(screen.getByRole("button", { name: "重置分辨率" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("无法确认结果");
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("prevents duplicate reset and closing while reset is in flight", async () => {
    let resolveReset!: (value: DeviceResolutionResetResult) => void;
    const onOpenChange = vi.fn();
    renderWithTheme(<DeviceResolutionDialog open onOpenChange={onOpenChange} />);
    await screen.findByText("emulator-5554");
    vi.mocked(invoke).mockImplementationOnce(() => new Promise((resolve) => { resolveReset = resolve; }));
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "重置分辨率" }));
    expect(screen.getByRole("button", { name: "重置分辨率" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "关闭" })).toBeDisabled();
    await user.keyboard("{Escape}");
    expect(onOpenChange).not.toHaveBeenCalled();
    await act(async () => resolveReset({ before, after, verified: true, message: "已恢复设备默认分辨率" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "关闭" })).toBeEnabled());
  });
});
