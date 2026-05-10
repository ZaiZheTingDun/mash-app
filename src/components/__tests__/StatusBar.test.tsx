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

  it("dispatches theme changes from the status bar toggle", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_server") return "JP";
      if (cmd === "get_use_bluestack") return false;
      if (cmd === "check_adb") return { connected: false, deviceName: null };
      return null;
    });
    const onThemeChange = vi.fn();
    const user = userEvent.setup();
    renderWithTheme(
      <StatusBar theme="light" onThemeChange={onThemeChange} />
    );

    await user.click(screen.getByRole("button", { name: "切换深色模式" }));

    expect(onThemeChange).toHaveBeenCalledWith("dark");
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
        operationLogs={[{ time: "12:34:56", message: "队伍就绪，点击开始任务" }]}
        operationLogOpen
        onOperationLogOpenChange={onOpenChange}
      />
    );

    expect(screen.getByText("队伍就绪，点击开始任务")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "关闭操作日志" }));

    expect(onOpenChange).toHaveBeenCalledWith(false);
  });
});
