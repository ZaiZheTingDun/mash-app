import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { listen, type Event } from "@tauri-apps/api/event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { PacketCapturePage } from "../PacketCapturePage";
import type { CaptureConfiguration, CaptureRecord, PacketCaptureWorkspace } from "../../../types/packetCapture";

const actions = vi.fn();
const reads = vi.fn();
let current: PacketCaptureWorkspace;
const idle = { phase: "idle" as const, serial: null, message: "", response: null };
const configured: CaptureConfiguration = { serial: "device", packageName: "com.bilibili.fatego", reqableVersion: "3.3.5", message: "环境配置完成，请点击开始按钮（设备重启后可能需要重新配置）" };
function record(id: string, capturedAt = Date.UTC(2026, 9, 26, 12), body = "raw%3D+response") : CaptureRecord {
  return {
    entry: { id, capturedAt, serial: "device" },
    response: { captureId: id, capturedAt, recordId: 42, url: "https://example.test/?_key=toplogin", statusCode: 200,
      byteLength: body.length, format: "原始响应", body, items: [{ itemId: 100, name: "黄金果实", quantity: 1234 }] },
  };
}
function showRecords(...records: CaptureRecord[]) {
  current.history = { records: records.map(r => r.entry), selected: records[0] ?? null };
}
beforeEach(() => {
  current = { capture: idle, history: { records: [], selected: null } };
  actions.mockReset();
  vi.mocked(listen).mockReset().mockResolvedValue(() => {});
  reads.mockReset().mockImplementation(async () => current);
  vi.mocked(invoke).mockReset().mockImplementation((command, args) => command === "get_packet_capture_workspace" ? reads() : actions(command, args));
});
afterEach(() => { vi.useRealTimers(); });

describe("PacketCapturePage", () => {
  it("renders the empty layout and only exposes one environment action without device calls on mount", async () => {
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    expect(await screen.findByText("暂无历史记录")).toBeInTheDocument();
    expect(screen.getByText("尚无账户物品")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "复制抓包结果" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "清空" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "环境配置" }).querySelector("svg")).toBeNull();
    expect(screen.queryByRole("button", { name: "安装证书" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "自动配置" })).not.toBeInTheDocument();
    expect(actions).not.toHaveBeenCalled();
  });

  it("runs the combined backend setup once, disables navigation while pending, and shows its result", async () => {
    const user = userEvent.setup();
    let finish!: (value: CaptureConfiguration) => void;
    actions.mockImplementation(() => new Promise(resolve => { finish = resolve; }));
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    await user.click(screen.getByRole("button", { name: "环境配置" }));
    expect(actions).toHaveBeenCalledExactlyOnceWith("setup_packet_capture", undefined);
    expect(screen.getByRole("status")).toHaveTextContent("首次启动协议");
    for (const name of ["环境配置", "返回"]) expect(screen.getByRole("button", { name })).toBeDisabled();
    expect(screen.getByRole("button", { name: "停止" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "环境配置" }));
    expect(actions).toHaveBeenCalledTimes(1);
    await act(async () => finish(configured));
    expect(await screen.findByRole("status")).toHaveTextContent(configured.message);
    expect(screen.getByRole("button", { name: "环境配置" })).toBeEnabled();
  });

  it("reports setup failures and clears the error after a successful retry", async () => {
    const user = userEvent.setup();
    actions.mockRejectedValueOnce("无法取得 root 权限").mockResolvedValueOnce(configured);
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    await user.click(screen.getByRole("button", { name: "环境配置" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("无法取得 root 权限");
    await user.click(screen.getByRole("button", { name: "环境配置" }));
    expect(await screen.findByRole("status")).toHaveTextContent(configured.message);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("updates the setup hint from backend progress and ignores events after completion", async () => {
    const user = userEvent.setup();
    let progress!: (event: Event<string>) => void;
    const unlisten = vi.fn();
    vi.mocked(listen).mockImplementation(async (name, handler) => {
      expect(name).toBe("packet-capture-setup-progress");
      progress = handler as typeof progress;
      return unlisten;
    });
    let finish!: (value: CaptureConfiguration) => void;
    actions.mockImplementation(() => {
      expect(listen).toHaveBeenCalledTimes(1);
      return new Promise(resolve => { finish = resolve; });
    });
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    await user.click(screen.getByRole("button", { name: "环境配置" }));
    const emit = (payload: string) => progress({ event: "packet-capture-setup-progress", id: 1, payload });
    act(() => emit("当前为服务条款页面，请滑动到底部并点击底部按钮接受条款"));
    expect(screen.getByRole("status")).toHaveTextContent(/^当前为服务条款页面，请滑动到底部并点击底部按钮接受条款$/);
    const modeHint = "检测到 Reqable 工作模式选择页，正在自动处理，如无响应请手动选择独立模式并进入下一步";
    act(() => emit(modeHint));
    expect(screen.getByRole("status")).toHaveTextContent(modeHint);
    await act(async () => finish(configured));
    expect(screen.getByRole("status")).toHaveTextContent(configured.message);
    expect(unlisten).toHaveBeenCalledTimes(1);
    act(() => emit("当前为服务条款页面，请滑动到底部并点击底部按钮接受条款"));
    expect(screen.getByRole("status")).toHaveTextContent(configured.message);
  });

  it("unsubscribes setup progress when leaving during configuration", async () => {
    const user = userEvent.setup();
    const unlisten = vi.fn();
    vi.mocked(listen).mockResolvedValue(unlisten);
    let finish!: (value: CaptureConfiguration) => void;
    actions.mockImplementation(() => new Promise(resolve => { finish = resolve; }));
    const view = renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    await user.click(screen.getByRole("button", { name: "环境配置" }));
    view.unmount();
    expect(unlisten).toHaveBeenCalledTimes(1);
    await act(async () => finish(configured));
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it("starts only on click, waits for backend confirmation and offers stop while capturing", async () => {
    const user = userEvent.setup();
    let finish!: (value: unknown) => void;
    actions.mockImplementation(() => new Promise(resolve => { finish = resolve; }));
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    await user.click(screen.getByRole("button", { name: "开始" }));
    expect(actions).toHaveBeenCalledExactlyOnceWith("start_packet_capture", undefined);
    expect(screen.getByRole("status")).toHaveTextContent("VPN 授权");
    for (const name of ["环境配置", "返回"]) expect(screen.getByRole("button", { name })).toBeDisabled();
    expect(screen.getByRole("button", { name: "停止" })).toBeEnabled();
    current = { ...current, capture: { ...idle, phase: "waiting", message: "等待游戏响应" } };
    await act(async () => finish({ serial: "device", alreadyActive: false, message: "已启动抓包" }));
    expect(await screen.findByRole("button", { name: "停止" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "环境配置" })).toBeDisabled();
    actions.mockImplementation(async () => { current = { ...current, capture: { ...idle, message: "抓包已停止" } }; });
    await user.click(screen.getByRole("button", { name: "停止" }));
    expect(actions).toHaveBeenLastCalledWith("stop_packet_capture", undefined);
    expect(await screen.findByRole("button", { name: "开始" })).toBeEnabled();
  });

  it("shows raw response as a single read-only line and copies through the native backend", async () => {
    const user = userEvent.setup();
    const saved = record("latest");
    showRecords(saved);
    actions.mockResolvedValue(undefined);
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    const input = await screen.findByRole("textbox", { name: "抓包原始响应" });
    await waitFor(() => expect(input).toHaveValue(saved.response.body));
    expect(input.tagName).toBe("INPUT");
    expect(input).toHaveAttribute("readonly");
    expect(screen.getByText("黄金果实")).toBeInTheDocument();
    expect(screen.getByText("1,234")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "复制抓包结果" }));
    expect(actions).toHaveBeenCalledExactlyOnceWith("copy_packet_capture_response", undefined);
    expect(await screen.findByText("已复制")).toBeInTheDocument();
  });

  it("surfaces copy errors and retains the complete raw response", async () => {
    const user = userEvent.setup();
    showRecords(record("a"));
    actions.mockRejectedValue("复制失败，请重试");
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    await waitFor(() => expect(screen.getByRole("button", { name: "复制抓包结果" })).toBeEnabled());
    await user.click(screen.getByRole("button", { name: "复制抓包结果" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("复制失败");
    expect(screen.getByRole("textbox")).toHaveValue("raw%3D+response");
    expect(screen.queryByText("已复制")).not.toBeInTheDocument();
  });

  it("selects, deletes individual records and clears history using backend results", async () => {
    const user = userEvent.setup();
    const latest = record("a"), older = record("b", Date.UTC(2026, 9, 25, 12), "older-raw");
    showRecords(latest, older);
    actions.mockImplementation(async (command: string, args?: { recordId: string | null }) => {
      if (command === "select_packet_capture_record") current.history = { ...current.history, selected: older };
      if (command === "delete_packet_capture_record") {
        current.history = args?.recordId === null ? { records: [], selected: null } : { records: [latest.entry], selected: latest };
      }
      return current.history;
    });
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    const region = screen.getByRole("complementary", { name: "抓包历史记录" });
    const choices = await within(region).findAllByRole("button", { name: /^查看 / });
    await user.click(choices[1]);
    expect(actions).toHaveBeenLastCalledWith("select_packet_capture_record", { recordId: "b" });
    expect(screen.getByRole("textbox")).toHaveValue("older-raw");
    await user.click(within(region).getAllByRole("button", { name: /^删除 / })[1]);
    expect(actions).toHaveBeenLastCalledWith("delete_packet_capture_record", { recordId: "b" });
    expect(within(region).getAllByRole("button", { name: /^查看 / })).toHaveLength(1);
    expect(screen.getByRole("textbox")).toHaveValue(latest.response.body);
    await user.click(screen.getByRole("button", { name: "清空" }));
    expect(actions).toHaveBeenLastCalledWith("delete_packet_capture_record", { recordId: null });
    expect(await screen.findByText("暂无历史记录")).toBeInTheDocument();
    expect(screen.getByRole("textbox")).toHaveValue("");
  });

  it("keeps records when deletion fails", async () => {
    const user = userEvent.setup();
    showRecords(record("a"));
    actions.mockRejectedValue("删除抓包记录失败");
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    await user.click(await screen.findByRole("button", { name: /^删除 / }));
    expect(await screen.findByRole("alert")).toHaveTextContent("删除抓包记录失败");
    expect(screen.getByRole("button", { name: /^查看 / })).toBeInTheDocument();
  });

  it("polls while waiting, stops after completion and renders persisted results even if stopping failed", async () => {
    vi.useFakeTimers();
    const saved = record("a");
    reads.mockResolvedValueOnce({ ...current, capture: { ...idle, phase: "waiting", message: "等待响应" } });
    showRecords(saved);
    current.capture = { phase: "error", serial: "device", message: "已获取响应；请在设备上手动关闭抓包", response: saved.response };
    const view = renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    await act(async () => {});
    expect(screen.getByRole("status")).toHaveTextContent("等待响应");
    await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
    expect(screen.getByRole("alert")).toHaveTextContent("手动关闭抓包");
    expect(screen.getByRole("textbox")).toHaveValue(saved.response.body);
    await act(async () => { await vi.advanceTimersByTimeAsync(3000); });
    expect(reads).toHaveBeenCalledTimes(2);
    view.unmount();
  });

  it("cancels polling when leaving and ignores a late response", async () => {
    vi.useFakeTimers();
    let finish!: (value: PacketCaptureWorkspace) => void;
    reads.mockImplementation(() => new Promise(resolve => { finish = resolve; }));
    const view = renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    view.unmount();
    await act(async () => { finish({ ...current, capture: { ...idle, phase: "waiting" } }); await vi.advanceTimersByTimeAsync(3000); });
    expect(reads).toHaveBeenCalledTimes(1);
  });

  it("retries a transient workspace read failure instead of abandoning the capture", async () => {
    vi.useFakeTimers();
    reads.mockRejectedValueOnce("连接中断");
    const view = renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    await act(async () => {});
    expect(screen.getByRole("alert")).toHaveTextContent("连接中断");
    await act(async () => { await vi.advanceTimersByTimeAsync(2000); });
    expect(reads).toHaveBeenCalledTimes(2);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    view.unmount();
  });
});


describe("PacketCapturePage cancellation", () => {
  it("allows stopping setup immediately and ignores late progress and success", async () => {
    const user = userEvent.setup();
    let progress!: (event: Event<string>) => void;
    const unlisten = vi.fn();
    vi.mocked(listen).mockImplementation(async (_name, handler) => {
      progress = handler as typeof progress;
      return unlisten;
    });
    let finishSetup!: (value: CaptureConfiguration) => void;
    let finishStop!: () => void;
    actions.mockImplementation((command: string) => {
      if (command === "setup_packet_capture") return new Promise(resolve => { finishSetup = resolve; });
      if (command === "stop_packet_capture") return new Promise<void>(resolve => { finishStop = resolve; });
      throw new Error(command);
    });
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    await user.click(screen.getByRole("button", { name: "环境配置" }));
    await user.click(screen.getByRole("button", { name: "停止" }));
    expect(screen.getByRole("button", { name: "停止" })).toBeDisabled();
    expect(screen.getByRole("status")).toHaveTextContent("正在停止");
    expect(unlisten).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole("button", { name: "停止" }));
    expect(actions.mock.calls.filter(([command]) => command === "stop_packet_capture")).toHaveLength(1);
    act(() => progress({ event: "packet-capture-setup-progress", id: 1, payload: configured.message }));
    expect(screen.getByRole("status")).toHaveTextContent("正在停止");
    current = { ...current, capture: { ...idle, message: "环境配置已停止" } };
    await act(async () => finishStop());
    await waitFor(() => expect(screen.getByRole("status")).toHaveTextContent("环境配置已停止"));
    await act(async () => finishSetup(configured));
    expect(screen.getByRole("status")).toHaveTextContent("环境配置已停止");
    expect(screen.getByRole("button", { name: "开始" })).toBeEnabled();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it("keeps stopping while an interrupted start rejects and does not surface its stale error", async () => {
    const user = userEvent.setup();
    let failStart!: (reason: string) => void;
    let finishStop!: () => void;
    actions.mockImplementation((command: string) => {
      if (command === "start_packet_capture") return new Promise((_resolve, reject) => { failStart = reject; });
      if (command === "stop_packet_capture") return new Promise<void>(resolve => { finishStop = resolve; });
      throw new Error(command);
    });
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    await user.click(screen.getByRole("button", { name: "开始" }));
    await user.click(screen.getByRole("button", { name: "停止" }));
    await act(async () => failStart("抓包启动已停止"));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "停止" })).toBeDisabled();
    expect(screen.getByRole("status")).toHaveTextContent("正在停止");
    current = { ...current, capture: { ...idle, message: "抓包已停止" } };
    await act(async () => finishStop());
    await waitFor(() => expect(screen.getByRole("button", { name: "开始" })).toBeEnabled());
  });

  it("does not invoke setup if stopped before its progress subscription is ready", async () => {
    const user = userEvent.setup();
    let finishListen!: (unlisten: () => void) => void;
    const unlisten = vi.fn();
    vi.mocked(listen).mockImplementation(() => new Promise(resolve => { finishListen = resolve; }));
    actions.mockResolvedValue(undefined);
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    await user.click(screen.getByRole("button", { name: "环境配置" }));
    expect(actions).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "停止" }));
    expect(actions).toHaveBeenCalledExactlyOnceWith("stop_packet_capture", undefined);
    await act(async () => finishListen(unlisten));
    expect(actions).toHaveBeenCalledTimes(1);
    expect(unlisten).toHaveBeenCalledTimes(1);
  });
});


describe("PacketCapturePage stop recovery", () => {
  it("keeps a retryable stop action after a backend-confirmed stop failure", async () => {
    const user = userEvent.setup();
    current.capture = { ...idle, phase: "error", serial: "pinned-device", message: "停止失败：设备离线" };
    actions.mockImplementation(async (command: string) => {
      expect(command).toBe("stop_packet_capture");
      current = { ...current, capture: { ...idle, message: "抓包已停止" } };
    });
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("设备离线");
    expect(screen.getByRole("button", { name: "环境配置" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "停止" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "开始" })).toBeEnabled());
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});
