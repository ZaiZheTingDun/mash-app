import { Theme } from "@radix-ui/themes";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { listen, type Event } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { PacketCapturePage } from "../PacketCapturePage";
import { invokeDevMock } from "../../../devMock";
import type { CaptureAppList, CaptureConfiguration, CaptureRecord, PacketCaptureWorkspace } from "../../../types/packetCapture";

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));

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
  vi.mocked(openUrl).mockReset().mockResolvedValue(undefined);
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
  it("reports a native browser-open failure with a manual URL and allows retry", async () => {
    const user = userEvent.setup();
    current.capture = { phase: "idle", serial: null, message: "请在 Reqable 设置中开启 API 服务后重试" };
    vi.mocked(openUrl).mockRejectedValueOnce(new Error("native opener failed"));
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    const link = await screen.findByRole("link", { name: "开启 API 服务" });
    await user.click(link);
    expect(await screen.findByRole("alert")).toHaveTextContent("无法打开浏览器，请手动访问：https://docs.mash.xiaotongx.com/");
    await user.click(link);
    await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
    expect(openUrl).toHaveBeenCalledTimes(2);
  });

  it("links API service setup instructions within startup errors without changing the full message", async () => {
    const user = userEvent.setup();
    const message = "Reqable 控制接口尚未就绪：Reqable API 服务未启用，请在 Reqable 设置中开启 API 服务后重试";
    actions.mockRejectedValue(message);
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    await waitFor(() => expect(screen.getByRole("button", { name: "开始" })).toBeEnabled());
    await user.click(screen.getByRole("button", { name: "开始" }));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(message);
    const guide = within(alert).getByRole("link", { name: "开启 API 服务" });
    expect(guide).toHaveAttribute("href", "https://docs.mash.xiaotongx.com/guides/package-capture/#api-%E6%9C%8D%E5%8A%A1");
    expect(guide).toHaveAttribute("target", "_blank");
    expect(guide).toHaveAttribute("rel", "noopener noreferrer");
    await user.click(guide);
    expect(openUrl).toHaveBeenCalledExactlyOnceWith("https://docs.mash.xiaotongx.com/guides/package-capture/#api-%E6%9C%8D%E5%8A%A1");
  });

  it("shows API key copy progress during startup and stops accepting hints after cancellation", async () => {
    const user = userEvent.setup();
    let progress!: (event: Event<string>) => void;
    const unlisten = vi.fn();
    vi.mocked(listen).mockImplementation(async (name, handler) => {
      if (name === "packet-capture-context-changed") return () => {};
      expect(name).toBe("packet-capture-start-progress");
      progress = handler as typeof progress;
      return unlisten;
    });
    let rejectStart!: (reason: string) => void;
    actions.mockImplementation((command: string) => {
      if (command === "start_packet_capture") return new Promise((_resolve, reject) => { rejectStart = reject; });
      if (command === "stop_packet_capture") { rejectStart("抓包启动已停止"); return Promise.resolve(); }
      throw new Error(command);
    });
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    await waitFor(() => expect(screen.getByRole("button", { name: "开始" })).toBeEnabled());
    await user.click(screen.getByRole("button", { name: "开始" }));
    const hint = "API 服务开启后，请创建 API key 并点击复制，mash 将自动验证并保存";
    act(() => progress({ event: "packet-capture-start-progress", id: 1, payload: hint }));
    expect(screen.getByRole("status")).toHaveTextContent(hint);
    const guide = screen.getByRole("link", { name: "创建 API key" });
    expect(guide).toHaveAttribute("href", "https://docs.mash.xiaotongx.com/guides/package-capture/#api-%E6%9C%8D%E5%8A%A1");
    expect(guide).toHaveAttribute("target", "_blank");
    await user.click(guide);
    expect(openUrl).toHaveBeenCalledTimes(1);
    const invalidFormatHint = "已收到剪贴板内容，但不是 API key 格式，请在 Reqable 中复制生成的 API key";
    act(() => progress({ event: "packet-capture-start-progress", id: 1, payload: invalidFormatHint }));
    expect(screen.getByRole("status")).toHaveTextContent(invalidFormatHint);
    expect(screen.getByRole("link", { name: "复制生成的 API key" })).toHaveAttribute("href", "https://docs.mash.xiaotongx.com/guides/package-capture/#api-%E6%9C%8D%E5%8A%A1");
    await user.click(screen.getByRole("link", { name: "复制生成的 API key" }));
    expect(openUrl).toHaveBeenCalledTimes(2);
    expect(openUrl).toHaveBeenLastCalledWith("https://docs.mash.xiaotongx.com/guides/package-capture/#api-%E6%9C%8D%E5%8A%A1");
    expect(screen.getByRole("button", { name: "停止" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "停止" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "开始" })).toBeEnabled());
    expect(unlisten).toHaveBeenCalledTimes(1);
    act(() => progress({ event: "packet-capture-start-progress", id: 1, payload: "late key hint" }));
    expect(screen.queryByText("late key hint")).not.toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("removes the startup progress subscription after a successful key import and start", async () => {
    const user = userEvent.setup();
    let progress!: (event: Event<string>) => void;
    const unlisten = vi.fn();
    vi.mocked(listen).mockImplementation(async (name, handler) => {
      if (name === "packet-capture-context-changed") return () => {};
      progress = handler as typeof progress; return unlisten;
    });
    let finish!: () => void;
    actions.mockImplementation(() => new Promise(resolve => {
      finish = () => {
        current.capture = { phase: "waiting", serial: "device", message: "等待数据中，请重新进入游戏……" };
        resolve({ serial: "device", message: "已开始抓取国服 FGO 流量" });
      };
    }));
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    await waitFor(() => expect(screen.getByRole("button", { name: "开始" })).toBeEnabled());
    await user.click(screen.getByRole("button", { name: "开始" }));
    act(() => progress({ event: "packet-capture-start-progress", id: 1, payload: "API key 已验证并保存，正在启动抓包…" }));
    expect(screen.getByRole("status")).toHaveTextContent("API key 已验证并保存");
    await act(async () => finish());
    expect(unlisten).toHaveBeenCalledTimes(1);
    act(() => progress({ event: "packet-capture-start-progress", id: 1, payload: "late hint" }));
    expect(screen.getByRole("status")).toHaveTextContent("等待数据中");
  });

  it("renders the browser development mock's empty workspace without attempting a record read", async () => {
    vi.mocked(invoke).mockImplementation((command, args) => invokeDevMock(command, args as Record<string, unknown> | undefined));
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    expect(await screen.findByText("暂无历史记录")).toBeInTheDocument();
    expect(screen.getByText("尚无账户物品")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "返回" })).toBeEnabled();
    expect(vi.mocked(invoke).mock.calls.some(([command]) => command === "select_packet_capture_record")).toBe(false);
  });

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
    expect(screen.getByRole("button", { name: "导出抓包结果" })).toBeDisabled();
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
      if (name === "packet-capture-context-changed") return () => {};
      expect(name).toBe("packet-capture-setup-progress");
      progress = handler as typeof progress;
      return unlisten;
    });
    let finish!: (value: CaptureConfiguration) => void;
    actions.mockImplementation(() => {
      expect(listen).toHaveBeenCalledTimes(2);
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
    vi.mocked(listen).mockImplementation(async name => name === "packet-capture-context-changed" ? () => {} : unlisten);
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

  it.each(["stop", "complete"])("locks navigation through the post-start read until capture %s", async outcome => {
    vi.useFakeTimers();
    const onBack = vi.fn();
    let finishStart!: (value: unknown) => void;
    actions.mockImplementation(() => new Promise(resolve => { finishStart = resolve; }));
    renderWithTheme(<PacketCapturePage onBack={onBack} />);
    await act(async () => {});
    const back = screen.getByRole("button", { name: "返回" });
    expect(back).toBeEnabled();
    let finishRead!: (value: PacketCaptureWorkspace) => void;
    reads.mockImplementationOnce(() => new Promise(resolve => { finishRead = resolve; }));
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "开始" })); });
    await act(async () => finishStart({ serial: "device", alreadyActive: false, message: "已启动抓包" }));
    // Start has resolved, but the workspace still contains the old idle state.
    expect(back).toBeDisabled();
    expect(screen.getByRole("button", { name: "停止" })).toBeEnabled();
    current = { ...current, capture: { ...idle, phase: "waiting", serial: "device", message: "等待游戏响应" } };
    await act(async () => finishRead(current));
    expect(back).toBeDisabled();
    fireEvent.click(back);
    expect(onBack).not.toHaveBeenCalled();
    if (outcome === "stop") {
      let finishStop!: () => void;
      actions.mockImplementation(() => new Promise<void>(resolve => { finishStop = resolve; }));
      await act(async () => { fireEvent.click(screen.getByRole("button", { name: "停止" })); });
      expect(back).toBeDisabled();
      current = { ...current, capture: idle };
      await act(async () => finishStop());
    } else {
      showRecords(record("captured"));
      current = { ...current, capture: { ...idle, phase: "found", serial: "device", message: "抓包已完成" } };
      await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
      expect(screen.getByRole("textbox")).toHaveValue("raw%3D+response");
    }
    expect(back).toBeEnabled();
    fireEvent.click(back);
    expect(onBack).toHaveBeenCalledOnce();
  });

  it("keeps navigation locked and offers stop when the first post-start workspace read fails", async () => {
    vi.useFakeTimers();
    let finishStart!: (value: unknown) => void;
    actions.mockImplementation(() => new Promise(resolve => { finishStart = resolve; }));
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    await act(async () => {});
    reads.mockRejectedValueOnce("连接中断");
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "开始" })); });
    await act(async () => finishStart({ serial: "device", alreadyActive: false, message: "已启动抓包" }));
    expect(screen.getByRole("alert")).toHaveTextContent("连接中断");
    expect(screen.getByRole("button", { name: "返回" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "停止" })).toBeEnabled();
    actions.mockImplementation(async () => { current = { ...current, capture: idle }; });
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "停止" })); });
    expect(screen.getByRole("button", { name: "返回" })).toBeEnabled();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
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

  it("exports the displayed record through the save dialog and blocks duplicate requests while pending", async () => {
    const user = userEvent.setup();
    const latest = record("latest"), older = record("older", Date.UTC(2026, 9, 25), "raw%3D+older\r\n");
    showRecords(latest, older);
    let finish!: (path: string | null) => void;
    actions.mockImplementation(() => new Promise(resolve => { finish = resolve; }));
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    await user.click((await screen.findAllByRole("button", { name: /^查看 / }))[1]);
    await waitFor(() => expect(screen.getByRole("button", { name: "导出抓包结果" })).toBeEnabled());
    const button = screen.getByRole("button", { name: "导出抓包结果" });
    await user.click(button);
    expect(actions).toHaveBeenCalledExactlyOnceWith("export_packet_capture_response", { recordId: "older" });
    expect(button).toHaveTextContent("导出中…");
    expect(button).toBeDisabled();
    expect(screen.getByRole("button", { name: "复制抓包结果" })).toBeDisabled();
    await user.click(button);
    expect(actions).toHaveBeenCalledTimes(1);
    await act(async () => finish("/tmp/capture.txt"));
    expect(button).toHaveTextContent("已导出");
    expect(button).toBeEnabled();
    expect(screen.getByRole("textbox")).toHaveValue("raw%3D+older");
    expect(reads).toHaveBeenCalledTimes(1);
    expect(recordReads).toHaveBeenCalledTimes(2);
  });

  it.each(["cancel", "failure"])("retains the response after export %s and allows retry", async outcome => {
    const user = userEvent.setup();
    showRecords(record("a"));
    if (outcome === "cancel") actions.mockResolvedValue(null);
    else actions.mockRejectedValue("写入抓包结果失败");
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    await waitFor(() => expect(screen.getByRole("button", { name: "导出抓包结果" })).toBeEnabled());
    await user.click(screen.getByRole("button", { name: "导出抓包结果" }));
    expect(screen.getByRole("button", { name: "导出抓包结果" })).toBeEnabled();
    expect(screen.queryByText("已导出")).not.toBeInTheDocument();
    expect(screen.getByRole("textbox")).toHaveValue("raw%3D+response");
    if (outcome === "failure") expect(screen.getByRole("alert")).toHaveTextContent("写入抓包结果失败");
    else expect(screen.queryByRole("alert")).not.toBeInTheDocument();
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
    vi.mocked(listen).mockImplementation(async (name, handler) => {
      if (name === "packet-capture-context-changed") return () => {};
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
    vi.mocked(listen).mockImplementation(name => name === "packet-capture-context-changed" ? Promise.resolve(() => {}) : new Promise(resolve => { finishListen = resolve; }));
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
    expect(screen.getByRole("button", { name: "返回" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "停止" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "开始" })).toBeEnabled());
    expect(screen.getByRole("button", { name: "返回" })).toBeEnabled();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});

describe("PacketCapturePage client selection", () => {
  const channel = { packageName: "com.bilibili.fgo.mi", name: "国服 FGO（渠道服）" };
  const official = { packageName: "com.bilibili.fatego", name: "国服 FGO（官服）" };

  function contextChanged() {
    const handler = vi.mocked(listen).mock.calls.find(([name]) => name === "packet-capture-context-changed")![1];
    act(() => handler({ event: "packet-capture-context-changed", id: 1, payload: null }));
  }

  it.each(["server", "device"] as const)("refreshes the displayed target after switching %s and pins setup/start to it", async change => {
    const user = userEvent.setup();
    const original: CaptureAppList = { serial: "device", server: "CN", apps: [channel], selectedPackage: channel.packageName };
    const app = change === "server" ? { name: "日服 FGO（官服）", packageName: "com.aniplex.fategrandorder" } : official;
    const next: CaptureAppList = { serial: change === "device" ? "other-device" : "device", server: change === "server" ? "JP" : "CN", apps: [app], selectedPackage: app.packageName };
    actions.mockResolvedValue(original);
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    const trigger = screen.getByRole("button", { name: "选择客户端" });
    await waitFor(() => expect(trigger).toBeEnabled());
    await user.click(trigger);
    await screen.findByRole("button", { name: `${channel.name} · ${channel.packageName}` });
    await user.keyboard("{Escape}");
    let finishRefresh!: (value: CaptureAppList) => void;
    actions.mockImplementation(command => command === "get_packet_capture_apps" ? new Promise(resolve => { finishRefresh = resolve; }) : Promise.resolve(configured));
    contextChanged();
    expect(trigger).toHaveTextContent("选择客户端");
    expect(trigger).not.toHaveTextContent(channel.packageName);
    await waitFor(() => expect(actions).toHaveBeenLastCalledWith("get_packet_capture_apps", undefined));
    await user.click(screen.getByRole("button", { name: "环境配置" }));
    expect(actions.mock.calls.some(([command]) => command === "setup_packet_capture")).toBe(false);
    await act(async () => finishRefresh(next));
    await waitFor(() => expect(actions).toHaveBeenLastCalledWith("setup_packet_capture", { clientContext: { serial: next.serial, server: next.server, selectedPackage: next.selectedPackage } }));
    expect(trigger).toHaveTextContent(app.packageName);
    await waitFor(() => expect(screen.getByRole("button", { name: "开始" })).toBeEnabled());
    await user.click(screen.getByRole("button", { name: "开始" }));
    expect(actions).toHaveBeenLastCalledWith("start_packet_capture", { clientContext: { serial: next.serial, server: next.server, selectedPackage: next.selectedPackage } });
  });

  it("discards discovery from before a context change and retries after its device lease is released", async () => {
    const user = userEvent.setup();
    const original: CaptureAppList = { serial: "old-device", server: "CN", apps: [channel], selectedPackage: channel.packageName };
    const next: CaptureAppList = { ...original, serial: "new-device", apps: [official], selectedPackage: official.packageName };
    let finishOld!: (value: CaptureAppList) => void;
    actions.mockImplementationOnce(() => new Promise(resolve => { finishOld = resolve; })).mockResolvedValue(next);
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    const trigger = screen.getByRole("button", { name: "选择客户端" });
    await waitFor(() => expect(trigger).toBeEnabled());
    await user.click(trigger);
    contextChanged();
    expect(actions).toHaveBeenCalledTimes(1);
    await act(async () => finishOld(original));
    await waitFor(() => expect(trigger).toHaveTextContent(official.packageName));
    expect(trigger).not.toHaveTextContent(channel.packageName);
    expect(actions).toHaveBeenCalledTimes(2);
  });

  it("blocks device actions when refreshing a changed context fails", async () => {
    const user = userEvent.setup();
    actions.mockRejectedValue("设备暂时离线");
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    await waitFor(() => expect(screen.getByRole("button", { name: "环境配置" })).toBeEnabled());
    contextChanged();
    await waitFor(() => expect(actions).toHaveBeenCalledExactlyOnceWith("get_packet_capture_apps", undefined));
    await user.click(screen.getByRole("button", { name: "环境配置" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("客户端信息已变更");
    await user.click(screen.getByRole("button", { name: "开始" }));
    expect(actions).toHaveBeenCalledTimes(1);
  });

  it.each(["light", "dark"] as const)("refreshes from the adjacent action menu without clearing the selection in %s mode", async appearance => {
    const user = userEvent.setup();
    const list = { serial: "device", server: "CN", apps: [channel], selectedPackage: channel.packageName };
    actions.mockResolvedValue(list);
    renderWithTheme(<Theme appearance={appearance}><PacketCapturePage onBack={vi.fn()} /></Theme>);
    const trigger = screen.getByRole("button", { name: "选择客户端" });
    await waitFor(() => expect(trigger).toBeEnabled());
    await user.click(trigger);
    const dropdown = await screen.findByRole("dialog", { name: "抓包客户端" });
    await within(dropdown).findByRole("button", { name: `${channel.name} · ${channel.packageName}` });
    expect(within(dropdown).queryByText("刷新")).not.toBeInTheDocument();

    let finishRefresh!: (value: typeof list) => void;
    actions.mockImplementation(() => new Promise(resolve => { finishRefresh = resolve; }));
    await user.click(screen.getByRole("button", { name: "客户端操作" }));
    expect(screen.queryByRole("dialog", { name: "抓包客户端" })).not.toBeInTheDocument();
    await user.click(await screen.findByRole("menuitem", { name: "刷新" }));
    expect(trigger).toHaveTextContent(channel.packageName);
    expect(screen.getByRole("button", { name: "返回" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "环境配置" })).toBeEnabled();
    expect(actions).toHaveBeenLastCalledWith("get_packet_capture_apps", undefined);
    await user.click(screen.getByRole("button", { name: "客户端操作" }));
    expect(await screen.findByRole("menuitem", { name: "刷新" })).toHaveAttribute("aria-disabled", "true");
    await act(async () => finishRefresh(list));
    expect(screen.getByRole("menuitem", { name: "刷新" })).not.toHaveAttribute("aria-disabled", "true");
    await user.keyboard("{Escape}");
  });

  it.each([false, true])("waits for client discovery before setup and respects stop while queued (%s)", async stopWhileQueued => {
    const user = userEvent.setup();
    const list = { serial: "device", server: "CN", apps: [channel], selectedPackage: channel.packageName };
    let finishDiscovery!: (value: typeof list) => void;
    actions.mockImplementation(async command => {
      if (command === "get_packet_capture_apps") return new Promise(resolve => { finishDiscovery = resolve; });
      if (command === "setup_packet_capture") return configured;
    });
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    await waitFor(() => expect(screen.getByRole("button", { name: "选择客户端" })).toBeEnabled());
    await user.click(screen.getByRole("button", { name: "选择客户端" }));
    await user.keyboard("{Escape}");
    await user.click(screen.getByRole("button", { name: "环境配置" }));
    expect(actions.mock.calls.some(([command]) => command === "setup_packet_capture")).toBe(false);
    expect(screen.getByRole("button", { name: "停止" })).toBeEnabled();
    if (stopWhileQueued) await user.click(screen.getByRole("button", { name: "停止" }));
    await act(async () => finishDiscovery(list));
    await waitFor(() => expect(screen.getByRole("button", { name: "环境配置" })).toBeEnabled());
    expect(actions.mock.calls.filter(([command]) => command === "setup_packet_capture")).toHaveLength(stopWhileQueued ? 0 : 1);
  });

  it.each(["light", "dark"] as const)("keeps the page and selected label stable while reopening the dropdown in %s mode", async appearance => {
    const user = userEvent.setup();
    const onNavigationLockChange = vi.fn();
    const list = { serial: "device", server: "CN", apps: [channel], selectedPackage: channel.packageName };
    showRecords(record("saved"));
    actions.mockResolvedValue(list);
    renderWithTheme(<Theme appearance={appearance}><PacketCapturePage onBack={vi.fn()} onNavigationLockChange={onNavigationLockChange} /></Theme>);
    const response = await screen.findByRole("textbox", { name: "抓包原始响应" });
    await waitFor(() => expect(response).toHaveValue("raw%3D+response"));
    const trigger = screen.getByRole("button", { name: "选择客户端" });
    await user.click(trigger);
    await screen.findByRole("button", { name: `${channel.name} · ${channel.packageName}` });
    await user.keyboard("{Escape}");
    reads.mockClear();
    recordReads.mockClear();
    onNavigationLockChange.mockClear();

    let finishRefresh!: (value: typeof list) => void;
    actions.mockImplementation(() => new Promise(resolve => { finishRefresh = resolve; }));
    await user.click(trigger);
    expect(trigger).toHaveTextContent(channel.packageName);
    expect(screen.getByRole("button", { name: `${channel.name} · ${channel.packageName}` })).toBeInTheDocument();
    expect(screen.queryByLabelText("正在读取客户端")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "返回" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "复制抓包结果" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "环境配置" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "开始" })).toBeEnabled();
    expect(onNavigationLockChange).not.toHaveBeenCalled();
    expect(screen.getByRole("textbox", { name: "抓包原始响应" })).toBe(response);
    await act(async () => finishRefresh(list));
    await waitFor(() => expect(screen.getByRole("button", { name: "环境配置" })).toBeEnabled());
    expect(reads).not.toHaveBeenCalled();
    expect(recordReads).not.toHaveBeenCalled();
    expect(onNavigationLockChange).not.toHaveBeenCalled();
    await user.keyboard("{Escape}");
  });

  it.each(["light", "dark"] as const)("opens the channel list directly from the header in %s mode", async appearance => {
    const user = userEvent.setup();
    actions.mockResolvedValue({ serial: "device", server: "CN", apps: [channel], selectedPackage: channel.packageName });
    renderWithTheme(<Theme appearance={appearance}><PacketCapturePage onBack={vi.fn()} /></Theme>);
    const trigger = screen.getByRole("button", { name: "选择客户端" });
    await waitFor(() => expect(trigger).toBeEnabled());
    await user.click(trigger);
    const dropdown = await screen.findByRole("dialog", { name: "抓包客户端" });
    expect(await within(dropdown).findByRole("button", { name: `${channel.name} · ${channel.packageName}` })).toHaveAttribute("aria-pressed", "true");
    expect(trigger).toHaveTextContent(channel.packageName);
    expect(screen.queryByRole("combobox")).not.toBeInTheDocument();
    expect(actions).toHaveBeenCalledExactlyOnceWith("get_packet_capture_apps", undefined);
    await user.keyboard("{Escape}");
    await user.click(screen.getByRole("button", { name: "环境配置" }));
    expect(actions).toHaveBeenLastCalledWith("setup_packet_capture", { clientContext: { serial: "device", server: "CN", selectedPackage: channel.packageName } });
  });

  it("saves a channel choice and closes the dropdown after backend confirmation", async () => {
    const user = userEvent.setup();
    const list = { serial: "device", server: "CN", apps: [official, channel], selectedPackage: null };
    let finishChoice!: (value: typeof list | Omit<typeof list, "selectedPackage"> & { selectedPackage: string }) => void;
    actions.mockImplementation(async command => {
      if (command === "setup_packet_capture") throw new Error("请在顶部选择需要抓包的客户端，再进行环境配置");
      return command === "select_packet_capture_app" ? new Promise(resolve => { finishChoice = resolve; }) : list;
    });
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    const trigger = screen.getByRole("button", { name: "选择客户端" });
    await waitFor(() => expect(trigger).toBeEnabled());
    await user.click(screen.getByRole("button", { name: "环境配置" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("请在顶部选择需要抓包的客户端");
    await user.click(trigger);
    const choice = await screen.findByRole("button", { name: `${channel.name} · ${channel.packageName}` });
    expect(screen.getByRole("alert")).toHaveTextContent("请在顶部选择需要抓包的客户端");
    expect(choice).toHaveAttribute("aria-pressed", "false");
    await user.click(choice);
    expect(actions).toHaveBeenLastCalledWith("select_packet_capture_app", { serial: "device", server: "CN", packageName: channel.packageName });
    expect(screen.getByRole("dialog", { name: "抓包客户端" })).toBeInTheDocument();
    expect(trigger).toHaveTextContent("选择客户端");
    await act(async () => finishChoice({ ...list, selectedPackage: channel.packageName }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "抓包客户端" })).not.toBeInTheDocument());
    expect(trigger).toHaveTextContent(channel.packageName);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("keeps the dropdown and backend selection after a rejected change and allows refreshing", async () => {
    const user = userEvent.setup();
    const list = { serial: "device", server: "CN", apps: [official, channel], selectedPackage: official.packageName };
    actions.mockImplementation(async command => {
      if (command === "select_packet_capture_app") throw new Error("设备已切换，请刷新客户端列表后重试");
      return list;
    });
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    const trigger = screen.getByRole("button", { name: "选择客户端" });
    await waitFor(() => expect(trigger).toBeEnabled());
    await user.click(trigger);
    await user.click(await screen.findByRole("button", { name: `${channel.name} · ${channel.packageName}` }));
    const dropdown = screen.getByRole("dialog", { name: "抓包客户端" });
    expect(await within(dropdown).findByRole("alert")).toHaveTextContent("设备已切换");
    expect(trigger).toHaveTextContent(official.packageName);
    expect(within(dropdown).getByRole("button", { name: `${official.name} · ${official.packageName}` })).toHaveAttribute("aria-pressed", "true");
    await user.click(screen.getByRole("button", { name: "客户端操作" }));
    await user.click(await screen.findByRole("menuitem", { name: "刷新" }));
    await waitFor(() => expect(trigger).toHaveTextContent(official.packageName));
    await user.click(trigger);
    const refreshed = await screen.findByRole("dialog", { name: "抓包客户端" });
    expect(await within(refreshed).findByRole("button", { name: `${official.name} · ${official.packageName}` })).toHaveAttribute("aria-pressed", "true");
  });

  it("shows an empty client list and blocks selection during capture", async () => {
    const user = userEvent.setup();
    actions.mockResolvedValue({ serial: "device", server: "CN", apps: [], selectedPackage: null });
    const view = renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    await waitFor(() => expect(screen.getByRole("button", { name: "选择客户端" })).toBeEnabled());
    await user.click(screen.getByRole("button", { name: "选择客户端" }));
    expect(await screen.findByText("未检测到当前服务器的 FGO 客户端")).toBeInTheDocument();
    view.unmount();
    current.capture = { phase: "waiting", serial: "device", message: "等待数据中" };
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    await screen.findByRole("button", { name: "停止" });
    expect(screen.getByRole("button", { name: "选择客户端" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "客户端操作" })).toBeDisabled();
  });
});
