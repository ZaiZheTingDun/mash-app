import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { emit, invoke, listen, type Event } from "../../../tauri";
import type { WindowChrome } from "../../../types/windowChrome";
import { WindowTitlebar } from "../WindowTitlebar";

vi.mock("../../../tauri", () => ({ invoke: vi.fn(), listen: vi.fn(), emit: vi.fn() }));

const initialChrome: WindowChrome = {
  customTitlebar: true, maximized: false, title: "Mash", version: "0.15.0",
};
let currentChrome: WindowChrome;
let onResize: (event: Event<unknown>) => void;
const stopListening = vi.fn();

beforeEach(() => {
  vi.clearAllMocks();
  currentChrome = { ...initialChrome };
  vi.mocked(invoke).mockImplementation(async (command, args) => {
    if (command === "get_window_chrome") return { ...currentChrome };
    if (command === "window_chrome_action" && args?.action === "toggleMaximize") {
      currentChrome = { ...currentChrome, maximized: !currentChrome.maximized };
    }
    return undefined;
  });
  vi.mocked(emit).mockResolvedValue();
  vi.mocked(listen).mockImplementation(async (_event, handler) => {
    onResize = handler;
    return stopListening;
  });
});

describe("WindowTitlebar", () => {
  it("uses backend window state for maximize and restore controls", async () => {
    const user = userEvent.setup();
    renderWithTheme(<WindowTitlebar ready />);
    await user.click(await screen.findByRole("button", { name: "最大化" }));
    expect(invoke).toHaveBeenCalledWith("window_chrome_action", { action: "toggleMaximize" });
    await user.click(await screen.findByRole("button", { name: "还原窗口" }));
    expect(await screen.findByRole("button", { name: "最大化" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "最小化" }));
    expect(invoke).toHaveBeenCalledWith("window_chrome_action", { action: "minimize" });
    await user.click(screen.getByRole("button", { name: "关闭窗口" }));
    expect(invoke).toHaveBeenCalledWith("window_chrome_action", { action: "close" });
  });

  it("refreshes the restore button when the OS resizes the window and cleans up", async () => {
    const { unmount } = renderWithTheme(<WindowTitlebar ready />);
    await screen.findByRole("button", { name: "最大化" });
    currentChrome.maximized = true;
    await act(async () => onResize({ event: "tauri://resize", id: 1, payload: {} }));
    expect(await screen.findByRole("button", { name: "还原窗口" })).toBeInTheDocument();
    unmount();
    await waitFor(() => expect(stopListening).toHaveBeenCalledOnce());
  });

  it("allows dragging and double click on the title but does not drag controls", async () => {
    renderWithTheme(<WindowTitlebar ready />);
    const title = await screen.findByText("Mash");
    fireEvent.mouseDown(title, { button: 0, detail: 1 });
    expect(invoke).toHaveBeenCalledWith("window_chrome_action", { action: "startDragging" });
    vi.mocked(invoke).mockClear();
    fireEvent.mouseDown(title, { button: 2, detail: 1 });
    fireEvent.mouseDown(screen.getByRole("button", { name: "应用菜单" }), { button: 0, detail: 1 });
    expect(invoke).not.toHaveBeenCalledWith("window_chrome_action", { action: "startDragging" });
    fireEvent.mouseDown(title, { button: 0, detail: 2 });
    await screen.findByRole("button", { name: "还原窗口" });
  });

  it.each([
    ["截图…", "save-adb-screenshot-requested"],
    ["重置设备分辨率…", "device-resolution-requested"],
    ["自检…", "self-check-requested"],
    ["资源管理…", "resource-manager-requested"],
    ["检查更新…", "updater-check-requested"],
  ])("keeps the existing %s action", async (label, event) => {
    const user = userEvent.setup();
    renderWithTheme(<WindowTitlebar ready />);
    await user.click(await screen.findByRole("button", { name: "应用菜单" }));
    await user.click(await screen.findByRole("menuitem", { name: label }));
    expect(emit).toHaveBeenCalledWith(event);
  });

  it("keeps window controls available while startup actions are disabled", async () => {
    const user = userEvent.setup();
    renderWithTheme(<WindowTitlebar ready={false} />);
    await user.click(await screen.findByRole("button", { name: "应用菜单" }));
    expect(await screen.findByRole("menuitem", { name: "自检…" })).toHaveAttribute("aria-disabled", "true");
    await user.click(screen.getByRole("menuitem", { name: "关于 Mash" }));
    expect(await screen.findByRole("dialog", { name: "关于 Mash" })).toHaveTextContent("版本 0.15.0");
  });

  it("omits custom controls for windows that retain native decorations", async () => {
    currentChrome.customTitlebar = false;
    renderWithTheme(<WindowTitlebar ready />);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("get_window_chrome"));
    expect(screen.queryByRole("banner")).not.toBeInTheDocument();
  });

  it("reports failed window actions", async () => {
    const user = userEvent.setup();
    renderWithTheme(<WindowTitlebar ready />);
    await screen.findByRole("button", { name: "最小化" });
    vi.mocked(invoke).mockRejectedValueOnce(new Error("无法最小化"));
    await user.click(screen.getByRole("button", { name: "最小化" }));
    expect(await screen.findByRole("dialog", { name: "窗口操作失败" })).toHaveTextContent("无法最小化");
  });
});
