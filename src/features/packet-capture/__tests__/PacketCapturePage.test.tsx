import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { listen, type Event } from "@tauri-apps/api/event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { PacketCapturePage } from "../PacketCapturePage";
import type { CaptureConfiguration, CaptureRecord, PacketCaptureWorkspace } from "../../../types/packetCapture";

const actions = vi.fn();
const reads = vi.fn();
const recordReads = vi.fn();
const savedRecords = new Map<string, CaptureRecord>();
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
  for (const record of records) savedRecords.set(record.entry.id, record);
  current.history = { records: records.map(r => r.entry), defaultRecordId: records[0]?.entry.id ?? null };
}
beforeEach(() => {
  current = { capture: idle, history: { records: [], defaultRecordId: null } };
  savedRecords.clear();
  recordReads.mockReset().mockImplementation(async (id: string) => savedRecords.get(id));
  actions.mockReset();
  vi.mocked(listen).mockReset().mockResolvedValue(() => {});
  reads.mockReset().mockImplementation(async () => current);
  vi.mocked(invoke).mockReset().mockImplementation((command, args) => {
    if (command === "get_packet_capture_workspace") return reads();
    if (command === "select_packet_capture_record") return recordReads((args as { recordId: string }).recordId);
    return actions(command, args);
  });
});
afterEach(() => { vi.useRealTimers(); });

describe("PacketCapturePage", () => {
  it("renders Skeletons immediately, then shows the index before the selected record finishes loading", async () => {
    const saved = record("a");
    showRecords(saved);
    let finishIndex!: (value: PacketCaptureWorkspace) => void;
    let finishRecord!: (value: CaptureRecord) => void;
    reads.mockImplementation(() => new Promise(resolve => { finishIndex = resolve; }));
    recordReads.mockImplementation(() => new Promise(resolve => { finishRecord = resolve; }));
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    expect(screen.getByRole("status", { name: "正在加载抓包历史" }).querySelector(".rt-Skeleton")).not.toBeNull();
    expect(screen.getByRole("status", { name: "正在加载账户物品" })).toBeInTheDocument();
    expect(screen.queryByText("暂无历史记录")).not.toBeInTheDocument();
    expect(recordReads).not.toHaveBeenCalled();
    await act(async () => finishIndex(current));
    expect(screen.getByRole("button", { name: /^查看 / })).toHaveAttribute("aria-pressed", "true");
    expect(screen.queryByRole("status", { name: "正在加载抓包历史" })).not.toBeInTheDocument();
    expect(screen.getByRole("region", { name: "抓包结果" })).toHaveAttribute("aria-busy", "true");
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "复制抓包结果" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "返回" })).toBeEnabled();
    await act(async () => finishRecord(saved));
    expect(screen.getByRole("textbox")).toHaveValue(saved.response.body);
    expect(screen.getByRole("region", { name: "数据预览" })).toHaveAttribute("aria-busy", "false");
    expect(screen.queryByRole("status", { name: "正在加载账户物品" })).not.toBeInTheDocument();
  });

  it.each(["resolve", "reject"])("selects immediately and ignores a stale record %s after switching again", async outcome => {
    const user = userEvent.setup();
    const a = record("a"), b = record("b", Date.UTC(2026, 9, 25), "b-raw"), c = record("c", Date.UTC(2026, 9, 24), "c-raw");
    showRecords(a, b, c);
    actions.mockResolvedValue(undefined);
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    await waitFor(() => expect(screen.getByRole("textbox")).toHaveValue(a.response.body));
    let finishB!: (value: CaptureRecord) => void;
    let failB!: (reason: string) => void;
    let finishC!: (value: CaptureRecord) => void;
    recordReads.mockImplementation((id: string) => id === "b"
      ? new Promise((resolve, reject) => { finishB = resolve; failB = reject; })
      : new Promise(resolve => { finishC = resolve; }));
    const choices = screen.getAllByRole("button", { name: /^查看 / });
    await user.click(choices[1]);
    expect(choices[1]).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("status", { name: "正在加载账户物品" })).toBeInTheDocument();
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
    expect(choices[2]).toBeEnabled();
    await user.click(choices[2]);
    expect(choices[2]).toHaveAttribute("aria-pressed", "true");
    await act(async () => finishC(c));
    expect(screen.getByRole("textbox")).toHaveValue("c-raw");
    await act(async () => outcome === "resolve" ? finishB(b) : failB("旧记录读取失败"));
    expect(screen.getByRole("textbox")).toHaveValue("c-raw");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "复制抓包结果" }));
    expect(actions).toHaveBeenCalledExactlyOnceWith("copy_packet_capture_response", { recordId: "c" });
  });

  it("ends Skeleton loading on a record failure and retries when selecting the same record", async () => {
    const user = userEvent.setup();
    showRecords(record("a"));
    recordReads.mockRejectedValueOnce("抓包记录读取失败");
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("抓包记录读取失败");
    expect(screen.queryByRole("status", { name: "正在加载账户物品" })).not.toBeInTheDocument();
    expect(screen.getByText("数据加载失败")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /^查看 / }));
    expect(await screen.findByRole("textbox")).toHaveValue("raw%3D+response");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(recordReads).toHaveBeenCalledTimes(2);
  });

  it("switches to the remaining record when deleting one whose data is still loading", async () => {
    const user = userEvent.setup();
    const latest = record("a"), older = record("b", Date.UTC(2026, 9, 25), "older-raw");
    showRecords(latest, older);
    let finishOld!: (value: CaptureRecord) => void;
    recordReads.mockImplementation((id: string) => id === "a" ? Promise.resolve(latest) : new Promise(resolve => { finishOld = resolve; }));
    actions.mockImplementation(async () => {
      current.history = { records: [latest.entry], defaultRecordId: "a" };
      return current.history;
    });
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    await user.click((await screen.findAllByRole("button", { name: /^查看 / }))[1]);
    expect(screen.getByRole("status", { name: "正在加载账户物品" })).toBeInTheDocument();
    await user.click(screen.getAllByRole("button", { name: /^删除 / })[1]);
    expect(await screen.findByRole("textbox")).toHaveValue(latest.response.body);
    await act(async () => finishOld(older));
    expect(screen.getByRole("textbox")).toHaveValue(latest.response.body);
    expect(screen.getAllByRole("button", { name: /^查看 / })).toHaveLength(1);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

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
    expect(actions).toHaveBeenCalledExactlyOnceWith("copy_packet_capture_response", { recordId: "latest" });
    expect(await screen.findByText("已复制")).toBeInTheDocument();
    expect(reads).toHaveBeenCalledTimes(1);
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
      if (command === "delete_packet_capture_record") {
        current.history = args?.recordId === null ? { records: [], defaultRecordId: null } : { records: [latest.entry], defaultRecordId: latest.entry.id };
      }
      return current.history;
    });
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    const region = screen.getByRole("complementary", { name: "抓包历史记录" });
    const choices = await within(region).findAllByRole("button", { name: /^查看 / });
    await user.click(choices[1]);
    expect(recordReads).toHaveBeenLastCalledWith("b");
    expect(screen.getByRole("textbox")).toHaveValue("older-raw");
    await user.click(within(region).getAllByRole("button", { name: /^删除 / })[1]);
    expect(actions).toHaveBeenLastCalledWith("delete_packet_capture_record", { recordId: "b" });
    expect(within(region).getAllByRole("button", { name: /^查看 / })).toHaveLength(1);
    expect(screen.getByRole("textbox")).toHaveValue(latest.response.body);
    await user.click(screen.getByRole("button", { name: "清空" }));
    expect(actions).toHaveBeenLastCalledWith("delete_packet_capture_record", { recordId: null });
    expect(await screen.findByText("暂无历史记录")).toBeInTheDocument();
    expect(screen.getByRole("textbox")).toHaveValue("");
    expect(reads).toHaveBeenCalledTimes(1);
  });

  it("resumes capture polling after selecting history without an immediate duplicate workspace read", async () => {
    vi.useFakeTimers();
    const latest = record("latest"), older = record("older", Date.UTC(2026, 9, 25), "older-raw");
    showRecords(latest, older);
    current.capture = { ...idle, phase: "waiting", serial: "device", message: "等待数据中" };
    const view = renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    await act(async () => {});
    await act(async () => { fireEvent.click(screen.getAllByRole("button", { name: /^查看 / })[1]); });
    expect(screen.getByRole("textbox")).toHaveValue("older-raw");
    expect(reads).toHaveBeenCalledTimes(1);
    await act(async () => { await vi.advanceTimersByTimeAsync(999); });
    expect(reads).toHaveBeenCalledTimes(1);
    await act(async () => { await vi.advanceTimersByTimeAsync(1); });
    expect(reads).toHaveBeenCalledTimes(2);
    expect(screen.getByRole("textbox")).toHaveValue("older-raw");
    expect(recordReads).toHaveBeenCalledTimes(2); // Polling only returns metadata.
    view.unmount();
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
    current.capture = { phase: "error", serial: "device", message: "已获取响应；请在设备上手动关闭抓包" };
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
