import { describe, it, expect, vi, beforeEach } from "vitest";
import { screen, waitFor, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { listen, type Event } from "@tauri-apps/api/event";
import { renderWithTheme } from "../../test/renderWithTheme";
import { StatusBar } from "../StatusBar";
import { SERVER_LABELS } from "../../types/server";

// The default `invoke` mock in `setup.ts` returns "JP" for `get_server`
// and `false` for `get_use_bluestack`; individual tests below override
// `set_server` (and friends) per-call to assert specific behaviour
// without leaking into other tests.

// Capture the handler so tests can drive automation events through the
// component the same way the runners do.
type AutomationPayload = { state: string };
type AutomationListener = (event: Event<AutomationPayload>) => void;
type AdbResetPayload = {
  message: string;
  done: boolean;
  ok?: boolean | null;
};
type AdbResetListener = (event: Event<AdbResetPayload>) => void;

function captureAutomationListener(
  targetEvent = "automation-status"
): { trigger: (state: string) => void } {
  const ref: { current: AutomationListener | null } = { current: null };
  vi.mocked(listen).mockImplementation(async (event, cb) => {
    if (event === targetEvent) {
      ref.current = cb as AutomationListener;
    }
    return () => {};
  });
  return {
    trigger: (state: string) => {
      // The mock unsubscribe returns a no-op so we just invoke the
      // captured callback with the same shape Tauri's `emit` produces.
      // act() is required so React flushes the resulting state update
      // before the test reads from the DOM.
      act(() => {
        ref.current?.({
          event: targetEvent,
          id: 0,
          payload: { state },
        } as Event<AutomationPayload>);
      });
    },
  };
}

describe("StatusBar", () => {
  beforeEach(() => {
    // Reset the listen mock back to the default no-op subscription each
    // test rebuilds — otherwise a captured handler from one test would
    // bleed into the next.
    vi.mocked(listen).mockImplementation(async () => () => {});
  });

  it("hydrates the server selector from get_server on mount", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_server") return "CN";
      if (cmd === "get_use_bluestack") return false;
      if (cmd === "check_adb")
        return { connected: false, deviceName: null };
      return null;
    });
    const user = userEvent.setup();
    renderWithTheme(<StatusBar />);

    // Open the popover so the Select trigger renders into the DOM.
    await user.click(screen.getByRole("button", { name: /游戏未连接/ }));

    const trigger = await screen.findByRole("combobox", { name: "服务器" });
    // Radix `<Select.Trigger>` renders the current value text inline.
    await waitFor(() => {
      expect(trigger).toHaveTextContent(SERVER_LABELS.CN);
    });
  });

  it("dispatches set_server with the chosen value and reverts on backend rejection", async () => {
    // First two `set_server` calls succeed; third one rejects to prove
    // the optimistic update is rolled back on failure.
    let setServerCallCount = 0;
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_server") return "JP";
      if (cmd === "get_use_bluestack") return false;
      if (cmd === "check_adb")
        return { connected: false, deviceName: null };
      if (cmd === "set_server") {
        setServerCallCount += 1;
        if (setServerCallCount === 2) {
          throw new Error("runner is busy");
        }
        return null;
      }
      return null;
    });

    // Silence the deliberate console.error from the rejection branch so
    // it doesn't pollute the test output.
    const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});

    const user = userEvent.setup();
    renderWithTheme(<StatusBar />);
    await user.click(screen.getByRole("button", { name: /游戏未连接/ }));

    const trigger = await screen.findByRole("combobox", { name: "服务器" });
    await user.click(trigger);
    await user.click(await screen.findByRole("option", { name: SERVER_LABELS.CN }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_server", { value: "CN" });
    });
    await waitFor(() => {
      expect(trigger).toHaveTextContent(SERVER_LABELS.CN);
    });

    // Second selection is rejected by the backend — UI must revert.
    await user.click(trigger);
    await user.click(await screen.findByRole("option", { name: SERVER_LABELS.JP }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_server", { value: "JP" });
    });
    // After the rejection settles, the trigger snaps back to the
    // previous value (CN) so the UI stays consistent with backend state.
    await waitFor(() => {
      expect(trigger).toHaveTextContent(SERVER_LABELS.CN);
    });
    expect(errSpy).toHaveBeenCalled();
    errSpy.mockRestore();
  });

  it("disables the server selector while the runner is Running", async () => {
    const automation = captureAutomationListener();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_server") return "JP";
      if (cmd === "get_use_bluestack") return false;
      if (cmd === "check_adb")
        return { connected: false, deviceName: null };
      return null;
    });

    const user = userEvent.setup();
    renderWithTheme(<StatusBar />);
    await user.click(screen.getByRole("button", { name: /游戏未连接/ }));

    const trigger = await screen.findByRole("combobox", { name: "服务器" });
    expect(trigger).not.toBeDisabled();

    // Simulate the runner transitioning into Running — the lock kicks
    // in immediately because the listener fires in the same tick.
    automation.trigger("Running");
    await waitFor(() => {
      expect(trigger).toBeDisabled();
    });

    // And the lock releases the moment a non-Running state arrives
    // (Idle, Stopped, Error all share the same heuristic — pick one).
    automation.trigger("Idle");
    await waitFor(() => {
      expect(trigger).not.toBeDisabled();
    });
  });

  it("also disables the server selector while enhancement automation is Running", async () => {
    const enhancement = captureAutomationListener("enhancement-automation-status");
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_server") return "JP";
      if (cmd === "get_use_bluestack") return false;
      if (cmd === "check_adb") return { connected: false, deviceName: null };
      return null;
    });

    const user = userEvent.setup();
    renderWithTheme(<StatusBar />);
    await user.click(screen.getByRole("button", { name: /游戏未连接/ }));

    const trigger = await screen.findByRole("combobox", { name: "服务器" });
    expect(trigger).not.toBeDisabled();

    enhancement.trigger("Running");
    await waitFor(() => {
      expect(trigger).toBeDisabled();
    });

    enhancement.trigger("Finished");
    await waitFor(() => {
      expect(trigger).not.toBeDisabled();
    });
  });

  it("cycles theme preference through light, dark, and system", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_server") return "JP";
      if (cmd === "get_use_bluestack") return false;
      if (cmd === "check_adb") return { connected: false, deviceName: null };
      return null;
    });
    const onThemeChange = vi.fn();
    const user = userEvent.setup();
    const { rerender } = renderWithTheme(
      <StatusBar theme="light" themePreference="light" onThemeChange={onThemeChange} />
    );

    await user.click(
      screen.getByRole("button", { name: "切换主题模式，当前浅色" })
    );

    expect(onThemeChange).toHaveBeenLastCalledWith("dark");

    rerender(
      <StatusBar theme="dark" themePreference="dark" onThemeChange={onThemeChange} />
    );
    await user.click(
      screen.getByRole("button", { name: "切换主题模式，当前深色" })
    );

    expect(onThemeChange).toHaveBeenLastCalledWith("system");

    rerender(
      <StatusBar theme="dark" themePreference="system" onThemeChange={onThemeChange} />
    );
    await user.click(
      screen.getByRole("button", { name: "切换主题模式，当前跟随系统" })
    );

    expect(onThemeChange).toHaveBeenLastCalledWith("light");
  });

  it("opens and renders the shared operation log panel from the status bar", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_server") return "JP";
      if (cmd === "get_use_bluestack") return false;
      if (cmd === "check_adb") return { connected: false, deviceName: null };
      return null;
    });
    const onOpenChange = vi.fn();
    const user = userEvent.setup();
    renderWithTheme(
      <StatusBar
        operationLogs={[
          { time: "12:34:56", message: "队伍就绪，点击开始任务", level: "info" },
        ]}
        operationLogOpen
        onOperationLogOpenChange={onOpenChange}
      />
    );

    expect(screen.getByText("队伍就绪，点击开始任务")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "关闭操作日志" }));

    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("calls the settings handler from the left status bar action", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_server") return "JP";
      if (cmd === "get_use_bluestack") return false;
      if (cmd === "check_adb") return { connected: false, deviceName: null };
      return null;
    });
    const onOpenSettings = vi.fn();
    const user = userEvent.setup();

    renderWithTheme(<StatusBar onOpenSettings={onOpenSettings} />);

    await user.click(screen.getByRole("button", { name: "设置" }));

    expect(onOpenSettings).toHaveBeenCalledTimes(1);
  });

  it("resets the BlueStacks ADB connection from the status popover and logs each step", async () => {
    let resetHandler: AdbResetListener | null = null;
    vi.mocked(listen).mockImplementation(async (event, cb) => {
      if (event === "adb-reset-status") {
        resetHandler = cb as AdbResetListener;
      }
      return () => {};
    });
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_server") return "JP";
      if (cmd === "get_use_bluestack") return true;
      if (cmd === "check_adb") return { connected: true, deviceName: "127.0.0.1:5555" };
      if (cmd === "reset_bluestacks_adb_connection") {
        return {
          ok: true,
          steps: [],
        };
      }
      return null;
    });
    const onLogEntry = vi.fn();
    const onOpenChange = vi.fn();
    const user = userEvent.setup();
    renderWithTheme(
      <StatusBar
        onLogEntry={onLogEntry}
        onOperationLogOpenChange={onOpenChange}
      />
    );

    await user.click(await screen.findByRole("button", { name: /游戏已连接/ }));
    await user.click(screen.getByRole("button", { name: "重置 ADB 链接" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("reset_bluestacks_adb_connection");
    });
    expect(onOpenChange).toHaveBeenCalledWith(true);

    act(() => {
      resetHandler?.({
        event: "adb-reset-status",
        id: 0,
        payload: {
          message: "开始重置 BlueStacks ADB 链接…",
          done: false,
          ok: null,
        },
      } as Event<AdbResetPayload>);
      resetHandler?.({
        event: "adb-reset-status",
        id: 1,
        payload: {
          message: "成功: adb disconnect 127.0.0.1:5555 (exit 0) - disconnected 127.0.0.1:5555",
          done: false,
          ok: null,
        },
      } as Event<AdbResetPayload>);
      resetHandler?.({
        event: "adb-reset-status",
        id: 2,
        payload: {
          message: "成功: adb -s 127.0.0.1:5555 shell echo ok (exit 0) - ok",
          done: false,
          ok: null,
        },
      } as Event<AdbResetPayload>);
      resetHandler?.({
        event: "adb-reset-status",
        id: 3,
        payload: {
          message: "ADB 链接重置完成",
          done: true,
          ok: true,
        },
      } as Event<AdbResetPayload>);
    });

    await waitFor(() => {
      expect(onLogEntry).toHaveBeenCalledWith("ADB 链接重置完成");
    });
    expect(onLogEntry).toHaveBeenCalledWith("开始重置 BlueStacks ADB 链接…");
    expect(onLogEntry).toHaveBeenCalledWith(
      expect.stringContaining("adb disconnect 127.0.0.1:5555"),
    );
    expect(onLogEntry).toHaveBeenCalledWith(
      expect.stringContaining("adb -s 127.0.0.1:5555 shell echo ok"),
    );
  });

  it("hides debug-level entries by default and reveals them via the toggle", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_server") return "JP";
      if (cmd === "get_use_bluestack") return false;
      if (cmd === "check_adb") return { connected: false, deviceName: null };
      return null;
    });
    const user = userEvent.setup();
    renderWithTheme(
      <StatusBar
        operationLogs={[
          { time: "12:34:56", message: "用户可见消息", level: "info" },
          { time: "12:34:57", message: "调试诊断输出", level: "debug" },
        ]}
        operationLogOpen
      />
    );

    // Info entry is always visible; debug entry hidden by default.
    expect(screen.getByText("用户可见消息")).toBeInTheDocument();
    expect(screen.queryByText("调试诊断输出")).not.toBeInTheDocument();

    // Trigger label counts info entries only — the debug entry must
    // not inflate the user-visible badge.
    expect(screen.getByText("操作日志 (1)")).toBeInTheDocument();

    // Flip the toggle: the debug entry should now appear.
    await user.click(screen.getByRole("checkbox", { name: /显示调试/ }));
    expect(screen.getByText("调试诊断输出")).toBeInTheDocument();
  });

  it("shows the update action in the operation log header when an update is available", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_server") return "JP";
      if (cmd === "get_use_bluestack") return false;
      if (cmd === "check_adb") return { connected: false, deviceName: null };
      return null;
    });
    const onInstallUpdate = vi.fn();
    const user = userEvent.setup();
    renderWithTheme(
      <StatusBar
        operationLogOpen
        updateAvailable
        onInstallUpdate={onInstallUpdate}
      />
    );

    await user.click(screen.getByRole("button", { name: "更新" }));

    expect(onInstallUpdate).toHaveBeenCalledTimes(1);
  });
});
