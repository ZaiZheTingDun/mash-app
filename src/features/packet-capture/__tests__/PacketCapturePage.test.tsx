import { act, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { PacketCapturePage } from "../PacketCapturePage";
import type { CaptureConfiguration, CaptureStarted, CertificateInstallation, PacketCaptureResponseState } from "../../../types/packetCapture";

const actions = vi.fn();
const responses = vi.fn();
const idle: PacketCaptureResponseState = { phase: "idle", serial: null, message: "", response: null };
beforeEach(() => {
  actions.mockReset();
  responses.mockReset().mockResolvedValue(idle);
  vi.mocked(invoke).mockReset().mockImplementation((command) => command === "get_packet_capture_response" ? responses() : actions(command));
});
afterEach(() => { vi.useRealTimers(); });

const installed: CertificateInstallation = {
  serial: "127.0.0.1:5555",
  alreadyInstalled: false,
  fingerprint: "CA:FE",
  message: "证书已安装，设备重启后需重新安装",
};

describe("PacketCapturePage", () => {
  it("only installs on click, prevents duplicate submissions and renders the backend result", async () => {
    const user = userEvent.setup();
    let finish!: (result: CertificateInstallation) => void;
    actions.mockImplementation(() => new Promise((resolve) => { finish = resolve; }));
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    expect(actions).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "安装证书" }));
    expect(actions).toHaveBeenCalledExactlyOnceWith("install_capture_certificate");
    expect(screen.getByRole("button", { name: "安装证书" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "自动配置" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "开始抓包" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "返回" })).toBeDisabled();
    await act(async () => finish(installed));
    expect(await screen.findByText(installed.message)).toBeInTheDocument();
    expect(screen.getByText(`设备：${installed.serial}`)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "安装证书" })).toBeEnabled();
  });

  it("configures only on click and blocks certificate installation and navigation while pending", async () => {
    const user = userEvent.setup();
    const configured: CaptureConfiguration = {
      serial: "127.0.0.1:5555", packageName: "com.bilibili.fatego", reqableVersion: "3.3.5",
      message: "Reqable 已配置为仅抓国服 FGO，抓包未开启",
    };
    let finish!: (result: CaptureConfiguration) => void;
    actions.mockImplementation(() => new Promise((resolve) => { finish = resolve; }));
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    expect(actions).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "自动配置" }));
    expect(actions).toHaveBeenCalledExactlyOnceWith("configure_packet_capture");
    expect(screen.getByRole("button", { name: "自动配置" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "安装证书" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "返回" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "自动配置" }));
    await user.click(screen.getByRole("button", { name: "安装证书" }));
    expect(actions).toHaveBeenCalledTimes(1);
    await act(async () => finish(configured));
    expect(await screen.findByText(configured.message)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "自动配置" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "返回" })).toBeEnabled();
  });

  it("shows configuration failures and clears the error on a successful retry", async () => {
    const user = userEvent.setup();
    actions.mockRejectedValueOnce("请先在设备上安装国服 FGO").mockResolvedValueOnce({
      serial: "device", packageName: "com.bilibili.fatego", reqableVersion: "3.3.5", message: "配置完成",
    });
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    await user.click(screen.getByRole("button", { name: "自动配置" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("请先在设备上安装国服 FGO");
    await user.click(screen.getByRole("button", { name: "自动配置" }));
    expect(await screen.findByText("配置完成")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("starts capture only on click, blocks other operations, and renders confirmed backend status", async () => {
    const user = userEvent.setup();
    let finish!: (result: CaptureStarted) => void;
    actions.mockImplementation(() => new Promise((resolve) => { finish = resolve; }));
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    expect(actions).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "开始抓包" }));
    expect(actions).toHaveBeenCalledExactlyOnceWith("start_packet_capture");
    for (const name of ["安装证书", "自动配置", "开始抓包", "返回"]) {
      expect(screen.getByRole("button", { name })).toBeDisabled();
    }
    expect(screen.getByRole("status")).toHaveTextContent("VPN 授权");
    await user.click(screen.getByRole("button", { name: "开始抓包" }));
    expect(actions).toHaveBeenCalledTimes(1);
    await act(async () => finish({ serial: "device", alreadyActive: false, message: "已开始抓取国服 FGO 流量" }));
    expect(await screen.findByText("已开始抓取国服 FGO 流量")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "开始抓包" })).toBeEnabled();
  });

  it("allows retry after a start failure and displays the already running backend result", async () => {
    const user = userEvent.setup();
    actions.mockRejectedValueOnce("请先点击自动配置").mockResolvedValueOnce({
      serial: "device", alreadyActive: true, message: "国服抓包已在运行",
    });
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    await user.click(screen.getByRole("button", { name: "开始抓包" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("请先点击自动配置");
    await user.click(screen.getByRole("button", { name: "开始抓包" }));
    expect(await screen.findByText("国服抓包已在运行")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("shows actionable backend errors and allows retrying", async () => {
    const user = userEvent.setup();
    actions.mockRejectedValueOnce("未找到在线 ADB 设备，请先连接设备").mockResolvedValueOnce(installed);
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    await user.click(screen.getByRole("button", { name: "安装证书" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("未找到在线 ADB 设备");
    await user.click(screen.getByRole("button", { name: "安装证书" }));
    expect(await screen.findByText(installed.message)).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("polls the backend while waiting and renders the complete response as read-only text", async () => {
    vi.useFakeTimers();
    const found: PacketCaptureResponseState = {
      phase: "found", serial: "device", message: "已获取国服 toplogin 响应，抓包已停止",
      response: { recordId: 42, url: "https://example.test/ac.php?_key=toplogin", statusCode: 200,
        byteLength: 25, format: "JSON", body: '{"cache":{"userSvt":[]}}' },
    };
    responses.mockResolvedValueOnce({ ...idle, phase: "waiting", message: "等待国服 toplogin 响应，请重新进入游戏…" }).mockResolvedValue(found);
    const view = renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    await act(async () => {});
    expect(screen.getByRole("status")).toHaveTextContent("等待国服 toplogin 响应");
    await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
    const body = screen.getByRole("textbox", { name: "toplogin 响应正文" });
    expect(body).toHaveValue(found.response!.body);
    expect(body).toHaveAttribute("readonly");
    expect(screen.getByText(/HTTP 200/)).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("抓包已停止");
    expect(screen.queryByText(/等待国服 toplogin 响应/)).not.toBeInTheDocument();
    await act(async () => { await vi.advanceTimersByTimeAsync(3000); });
    expect(responses).toHaveBeenCalledTimes(2);
    view.unmount();
  });

  it("stops polling after leaving the page and ignores an in-flight late response", async () => {
    vi.useFakeTimers();
    let resolve!: (state: PacketCaptureResponseState) => void;
    responses.mockImplementation(() => new Promise((finish) => { resolve = finish; }));
    const view = renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    view.unmount();
    await act(async () => {
      resolve({ ...idle, phase: "waiting" });
      await vi.advanceTimersByTimeAsync(3000);
    });
    expect(responses).toHaveBeenCalledTimes(1);
  });

  it("shows backend response-read errors without displaying a stale body", async () => {
    responses.mockResolvedValue({ ...idle, phase: "error", message: "响应缓存读取不完整，请重新登录后重试" });
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("响应缓存读取不完整");
    expect(screen.queryByRole("textbox")).not.toBeInTheDocument();
  });

  it("retains the response when stopping fails and replaces the earlier running message", async () => {
    const user = userEvent.setup();
    actions.mockResolvedValue({ serial: "device", alreadyActive: false, message: "已开始抓取国服 FGO 流量" });
    responses.mockResolvedValueOnce(idle).mockResolvedValue({
      phase: "error", serial: "device", message: "已获取国服 toplogin 响应；无法确认 Reqable 抓包已停止，请在设备上手动关闭抓包",
      response: { recordId: 42, url: "https://example.test/ac.php?_key=toplogin", statusCode: 200,
        byteLength: 12, format: "JSON", body: '{"cache":{}}' },
    });
    renderWithTheme(<PacketCapturePage onBack={vi.fn()} />);
    await act(async () => {});
    await user.click(screen.getByRole("button", { name: "开始抓包" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("请在设备上手动关闭抓包");
    expect(screen.getByRole("textbox", { name: "toplogin 响应正文" })).toHaveValue('{"cache":{}}');
    expect(screen.queryByText("已开始抓取国服 FGO 流量")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "开始抓包" })).toBeEnabled();
  });
});
