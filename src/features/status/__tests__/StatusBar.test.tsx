import { describe, it, expect, vi, beforeEach } from "vitest";
import { screen, waitFor, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { listen, type Event } from "@tauri-apps/api/event";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { StatusBar } from "../StatusBar";
import { SERVER_LABELS } from "../../../types/server";
import type { Servant } from "../../../types/servant";
import type { AutomationStatus } from "../../../types/automation";

// The default `invoke` mock in `setup.ts` returns "JP" for `get_server`
// and `false` for `get_use_bluestack`; individual tests below override
// `set_server` (and friends) per-call to assert specific behaviour
// without leaking into other tests.

// Capture the handler so tests can drive automation events through the
// component the same way the runners do.
type AutomationPayload = { status: AutomationStatus };
type AutomationListener = (event: Event<AutomationPayload>) => void;
type AdbResetPayload = {
  message: string;
  done: boolean;
  ok?: boolean | null;
};
type AdbResetListener = (event: Event<AdbResetPayload>) => void;

const LOG_SERVANTS: Servant[] = [
  {
    id: 284,
    variantKey: "284",
    faceId: 800284,
    name_cn: "从者二八四",
    name_jp: "",
    name_en: "",
    class: "caster",
    rarity: 5,
  },
  {
    id: 16,
    variantKey: "16",
    faceId: 800016,
    name_cn: "从者十六",
    name_jp: "",
    name_en: "",
    class: "saber",
    rarity: 4,
  },
  {
    id: 309,
    variantKey: "309",
    faceId: 800309,
    name_cn: "从者三零九",
    name_jp: "",
    name_en: "",
    class: "pretender",
    rarity: 5,
  },
];

function captureAutomationListener(
  targetEvent = "automation-status"
): { trigger: (status: AutomationStatus) => void } {
  const ref: { current: AutomationListener | null } = { current: null };
  vi.mocked(listen).mockImplementation(async (event, cb) => {
    if (event === targetEvent) {
      ref.current = cb as AutomationListener;
    }
    return () => {};
  });
  return {
    trigger: (status: AutomationStatus) => {
      // The mock unsubscribe returns a no-op so we just invoke the
      // captured callback with the same shape Tauri's `emit` produces.
      // act() is required so React flushes the resulting state update
      // before the test reads from the DOM.
      act(() => {
        ref.current?.({
          event: targetEvent,
          id: 0,
          payload: { status },
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

  it("shows an empty run-status panel and closes it", async () => {
    const onOpenChange = vi.fn();
    const user = userEvent.setup();
    renderWithTheme(
      <StatusBar
        battleRunStatusOpen
        onBattleRunStatusOpenChange={onOpenChange}
      />
    );

    expect(screen.getByText("尚无运行记录")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "关闭运行状态" }));
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("renders completed runs, frozen duration, estimate and non-zero recovery items", () => {
    renderWithTheme(
      <StatusBar
        battleRunStatusOpen
        battleRunStatus={{
          phase: "finished",
          startedAtMs: 0,
          endedAtMs: 6_800_000,
          lastCompletedAtMs: 6_800_000,
          completedRuns: 30,
          maxRuns: 30,
          apRecoveryUsage: {
            gold: 1,
            silver: 2,
            bronze: 0,
            copper: 0,
            rainbow: 0,
          },
        }}
      />
    );

    expect(screen.getByText("30 次 / 30 次")).toBeInTheDocument();
    expect(screen.getByText("1 小时 53 分钟 20 秒")).toBeInTheDocument();
    expect(screen.getByText("已完成")).toBeInTheDocument();
    expect(screen.getByAltText("黄金果实")).toBeInTheDocument();
    expect(screen.getByAltText("白银果实")).toBeInTheDocument();
    expect(screen.queryByAltText("青铜果实")).not.toBeInTheDocument();
    expect(screen.getByText("× 1")).toBeInTheDocument();
    expect(screen.getByText("× 2")).toBeInTheDocument();
    expect(screen.getByText("运行状态（30 次）")).toBeInTheDocument();
  });

  it("keeps the run-status trigger styling aligned with the operation-log trigger", () => {
    renderWithTheme(<StatusBar />);

    expect(screen.getByRole("button", { name: "操作日志" })).toHaveClass("status-log-btn");
    expect(screen.getByRole("button", { name: "运行状态" })).toHaveClass("status-run-btn");
  });

  it("shows persisted daily battle totals as compact rows", () => {
    renderWithTheme(
      <StatusBar
        battleRunStatusOpen
        battleDailyStatistics={{
          dayStartMs: 0,
          calculatedAtMs: 20_000,
          completedRuns: 15,
          durationMs: 6_800_000,
          apRecoveryUsage: {
            gold: 1,
            silver: 0,
            bronze: 0,
            copper: 2,
            rainbow: 0,
          },
        }}
      />,
    );

    expect(screen.getByText("今日运行：")).toBeInTheDocument();
    expect(screen.getByText("15 次")).toBeInTheDocument();
    expect(screen.getByText("今日时间：")).toBeInTheDocument();
    expect(screen.getByText("1 小时 53 分钟 20 秒")).toBeInTheDocument();
    expect(screen.getByText("今日道具：")).toBeInTheDocument();
    expect(screen.getByAltText("黄金果实")).toBeInTheDocument();
    expect(screen.getByAltText("赤铜果实")).toBeInTheDocument();
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

    // Simulate the runner transitioning into running — the lock kicks
    // in immediately because the listener fires in the same tick.
    automation.trigger("running");
    await waitFor(() => {
      expect(trigger).toBeDisabled();
    });

    // And the lock releases the moment a terminal state arrives.
    automation.trigger("idle");
    await waitFor(() => {
      expect(trigger).not.toBeDisabled();
    });
  });

  it("also disables the server selector while enhancement automation is running", async () => {
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

    enhancement.trigger("running");
    await waitFor(() => {
      expect(trigger).toBeDisabled();
    });

    enhancement.trigger("finished");
    await waitFor(() => {
      expect(trigger).not.toBeDisabled();
    });
  });

  it("locks the server selector for CE enhancement and friend point summon", async () => {
    const listeners = new Map<string, AutomationListener>();
    vi.mocked(listen).mockImplementation(async (event, callback) => {
      if (
        event === "craft-essence-enhancement-automation-status" ||
        event === "friend-point-summon-automation-status"
      ) {
        listeners.set(event, callback as AutomationListener);
      }
      return () => {};
    });
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_server") return "CN";
      if (cmd === "get_use_bluestack") return false;
      if (cmd === "check_adb") return { connected: false, deviceName: null };
      return null;
    });

    const user = userEvent.setup();
    renderWithTheme(<StatusBar />);
    await user.click(screen.getByRole("button", { name: /游戏未连接/ }));
    const trigger = await screen.findByRole("combobox", { name: "服务器" });

    for (const event of [
      "craft-essence-enhancement-automation-status",
      "friend-point-summon-automation-status",
    ]) {
      act(() => {
        listeners.get(event)?.({
          event,
          id: 0,
          payload: { status: "running" },
        } as Event<AutomationPayload>);
      });
      await waitFor(() => expect(trigger).toBeDisabled());

      act(() => {
        listeners.get(event)?.({
          event,
          id: 1,
          payload: { status: "finished" },
        } as Event<AutomationPayload>);
      });
      await waitFor(() => expect(trigger).not.toBeDisabled());
    }
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

  it("opens the author support dialog with WeChat and Alipay images", async () => {
    const user = userEvent.setup();
    renderWithTheme(<StatusBar onOpenDebug={() => {}} />);

    const supportButton = screen.getByRole("button", { name: "支持作者" });
    const debugButton = screen.getByRole("button", { name: "CV 调试" });
    expect(
      supportButton.compareDocumentPosition(debugButton) & Node.DOCUMENT_POSITION_FOLLOWING
    ).toBeTruthy();

    await user.click(supportButton);

    expect(screen.getByRole("dialog", { name: "支持作者" })).toBeInTheDocument();
    expect(screen.getByText(/Mash 仍在持续开发中/)).toBeInTheDocument();
    expect(screen.getByAltText("微信支持作者二维码")).toBeInTheDocument();
    expect(screen.getByAltText("支付宝支持作者二维码")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "关闭支持作者" }));
    expect(screen.queryByRole("dialog", { name: "支持作者" })).not.toBeInTheDocument();
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

  it("renders attack operation logs with compact servant faces and card colors", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "get_server") return "JP";
      if (cmd === "get_use_bluestack") return false;
      if (cmd === "check_adb") return { connected: false, deviceName: null };
      if (cmd === "get_servant_face_path") {
        const { servantId } = args as { servantId: number };
        return `/tmp/face-${servantId}.png`;
      }
      return null;
    });

    const { container } = renderWithTheme(
      <StatusBar
        servants={LOG_SERVANTS}
        operationLogOpen
        operationLogs={[
          {
            time: "19:53:25",
            message: "指令卡候选从者: [284, 16, 309]",
            level: "info",
            attack: {
              frontServantIds: [284, 16, 309],
              candidateServantIds: [284, 16, 309],
            },
          },
          {
            time: "19:53:25",
            message: "指令卡: C1=q/S3:309 C2=b/S3:309 C3=a/S2:16",
            level: "info",
            attack: {
              frontServantIds: [284, 16, 309],
              commandCards: [
                { slot: 0, suit: "q", servantId: 309, isSupport: false, isStunned: false },
                { slot: 1, suit: "b", servantId: 309, isSupport: false, isStunned: true },
                { slot: 2, suit: "a", servantId: 16, isSupport: false, isStunned: false },
              ],
            },
          },
        ]}
      />
    );

    expect(screen.getByText("指令卡候选从者:")).toBeInTheDocument();
    expect(screen.getByLabelText("从者二八四")).toBeInTheDocument();
    expect(screen.getAllByLabelText("从者三零九")).toHaveLength(2);
    expect(screen.getByLabelText("无法行动：从者三零九")).toHaveClass(
      "operation-log-face--stunned"
    );
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("get_servant_face_path", {
        servantId: 309,
        faceId: 800309,
      });
    });
    expect(container.querySelector(".operation-log-suit-dot--q")).not.toBeNull();
    expect(container.querySelector(".operation-log-suit-dot--b")).not.toBeNull();
    expect(container.querySelector(".operation-log-suit-dot--a")).not.toBeNull();
  });

  it("renders servant skills, targets, and order changes with icons", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "get_server") return "JP";
      if (cmd === "get_use_bluestack") return false;
      if (cmd === "check_adb") return { connected: false, deviceName: null };
      if (cmd === "get_servant_face_path") {
        const { servantId } = args as { servantId: number };
        return `/tmp/face-${servantId}.png`;
      }
      if (cmd === "get_skill_icon_paths") {
        return [
          { path: "/tmp/skill-1.png", name: "一技能" },
          { path: "/tmp/skill-2.png", name: "二技能" },
          { path: "/tmp/skill-3.png", name: "三技能" },
        ];
      }
      return null;
    });

    renderWithTheme(
      <StatusBar
        servants={LOG_SERVANTS}
        operationLogOpen
        operationLogs={[
          {
            time: "19:53:25",
            message: "从者技能: servant_3 使用 skill_2",
            level: "info",
            action: {
              kind: "servantSkill",
              servantId: 309,
              skillIndex: 1,
              targetServantId: 16,
            },
          },
          {
            time: "19:53:26",
            message: "Order Change 选择从者: servant_1 ↔ servant_4",
            level: "info",
            action: {
              kind: "orderChange",
              frontServantId: 284,
              backServantId: 309,
            },
          },
        ]}
      />
    );

    expect(screen.queryByText(/servant_3|skill_2/)).not.toBeInTheDocument();
    expect(screen.getByText("从者技能:")).toBeInTheDocument();
    expect(await screen.findByLabelText("二技能")).toBeInTheDocument();
    expect(screen.getByText("Order Change:")).toBeInTheDocument();
    expect(screen.getAllByLabelText("从者三零九")).toHaveLength(2);
    expect(screen.getByLabelText("从者十六")).toBeInTheDocument();
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("get_skill_icon_paths", {
        servantId: 309,
        variantKey: "309",
      });
    });
  });

  it("renders ready NPs with muted faces and highlights ready slots", () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_server") return "JP";
      if (cmd === "get_use_bluestack") return false;
      if (cmd === "check_adb") return { connected: false, deviceName: null };
      if (cmd === "get_servant_face_path") return null;
      return null;
    });

    const { container } = renderWithTheme(
      <StatusBar
        servants={LOG_SERVANTS}
        operationLogOpen
        operationLogs={[
          {
            time: "19:57:35",
            message: "宝具就绪: NP3",
            level: "info",
            attack: {
              frontServantIds: [284, 16, 309],
              readyNpSlots: [2],
            },
          },
        ]}
      />
    );

    expect(screen.getByText("宝具就绪:")).toBeInTheDocument();
    expect(container.querySelectorAll(".operation-log-face--muted")).toHaveLength(2);
    expect(container.querySelectorAll(".operation-log-face--highlighted")).toHaveLength(1);
    expect(screen.getByLabelText("从者三零九")).toHaveClass("operation-log-face--highlighted");
  });

  it("renders selected NP and command-card picks with fallback faces", () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_server") return "JP";
      if (cmd === "get_use_bluestack") return false;
      if (cmd === "check_adb") return { connected: false, deviceName: null };
      if (cmd === "get_servant_face_path") return null;
      return null;
    });

    const { container } = renderWithTheme(
      <StatusBar
        servants={LOG_SERVANTS}
        operationLogOpen
        operationLogs={[
          {
            time: "19:57:35",
            message: "1/3 选择 servant_3_np → NP3",
            level: "info",
            attack: {
              frontServantIds: [284, 16, 309],
              selectedPick: {
                step: 1,
                total: 3,
                fromPriority: "servant_3_np",
                kind: "np",
                slot: 2,
                servantId: 309,
              },
            },
          },
          {
            time: "19:57:35",
            message: "2/3 选择 servant_3_all → C3 (q/309)",
            level: "info",
            attack: {
              frontServantIds: [284, 16, 309],
              selectedPick: {
                step: 2,
                total: 3,
                fromPriority: "servant_3_all",
                kind: "card",
                slot: 2,
                suit: "q",
                servantId: 309,
              },
            },
          },
        ]}
      />
    );

    expect(screen.getAllByText(/选择/)).toHaveLength(2);
    expect(screen.queryByText(/servant_3_/)).not.toBeInTheDocument();
    expect(screen.getAllByText("宝具")).toHaveLength(2);
    expect(screen.getByText("任意卡")).toBeInTheDocument();
    expect(screen.getByText("指令卡三")).toBeInTheDocument();
    expect(container.querySelector(".operation-log-suit-dot--q")).not.toBeNull();
    expect(container.querySelectorAll(".operation-log-face")).toHaveLength(4);
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

  it("resets the ADB connection from the status popover and logs each step", async () => {
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
    await user.click(screen.getByRole("button", { name: "重置 ADB" }));

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

  it("opens the device selection dialog with a loading state before showing previews", async () => {
    let resolveDevices: (value: unknown) => void = () => {};
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_server") return "JP";
      if (cmd === "check_adb") return { connected: false, deviceName: null };
      if (cmd === "refresh_adb_devices_with_previews") {
        return new Promise((resolve) => {
          resolveDevices = resolve;
        });
      }
      return null;
    });
    const user = userEvent.setup();
    renderWithTheme(<StatusBar />);

    await user.click(screen.getByRole("button", { name: /游戏未连接/ }));
    await user.click(screen.getByRole("button", { name: "选择设备" }));

    expect(screen.getByText("正在读取现有 ADB 设备…")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("refresh_adb_devices_with_previews", { refresh: false });

    act(() => {
      resolveDevices([
        {
          serial: "127.0.0.1:5555",
          description: "product:bluestacks",
          previewPath: "/tmp/device.png",
          selected: true,
        },
      ]);
    });

    expect(await screen.findByText("127.0.0.1:5555")).toBeInTheDocument();
    expect(screen.getByText("已自动选择：127.0.0.1:5555")).toBeInTheDocument();
  });

  it("selects an adb device from the preview grid", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "get_server") return "JP";
      if (cmd === "check_adb") return { connected: false, deviceName: null };
      if (cmd === "refresh_adb_devices_with_previews") {
        return [
          {
            serial: "emulator-5554",
            description: "model:Pixel",
            previewPath: "/tmp/emulator.png",
            selected: false,
          },
        ];
      }
      if (cmd === "select_adb_device") {
        const serial = typeof args === "object" && args != null && "serial" in args
          ? (args as { serial: string }).serial
          : null;
        return { connected: true, deviceName: serial };
      }
      return null;
    });
    const user = userEvent.setup();
    renderWithTheme(<StatusBar />);

    await user.click(screen.getByRole("button", { name: /游戏未连接/ }));
    await user.click(screen.getByRole("button", { name: "选择设备" }));
    await user.click(await screen.findByRole("button", { name: /emulator-5554/ }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("select_adb_device", { serial: "emulator-5554" });
    });
    expect(await screen.findByText("游戏已连接")).toBeInTheDocument();
  });

  it("shows an empty adb device state with a rescan action", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_server") return "JP";
      if (cmd === "check_adb") return { connected: false, deviceName: null };
      if (cmd === "refresh_adb_devices_with_previews") return [];
      return null;
    });
    const user = userEvent.setup();
    renderWithTheme(<StatusBar />);

    await user.click(screen.getByRole("button", { name: /游戏未连接/ }));
    await user.click(screen.getByRole("button", { name: "选择设备" }));

    expect(await screen.findByText("未检测到可用 ADB 设备")).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "重新扫描" }).length).toBeGreaterThan(0);
  });

  it("refreshes adb connections only when the rescan action is clicked", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_server") return "JP";
      if (cmd === "check_adb") return { connected: false, deviceName: null };
      if (cmd === "refresh_adb_devices_with_previews") return [];
      return null;
    });
    const user = userEvent.setup();
    renderWithTheme(<StatusBar />);

    await user.click(screen.getByRole("button", { name: /游戏未连接/ }));
    await user.click(screen.getByRole("button", { name: "选择设备" }));
    await screen.findByText("未检测到可用 ADB 设备");

    expect(invoke).toHaveBeenCalledWith("refresh_adb_devices_with_previews", { refresh: false });

    await user.click(screen.getAllByRole("button", { name: "重新扫描" })[0]);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("refresh_adb_devices_with_previews", { refresh: true });
    });
  });

  it("connects a manually entered adb port and rereads the current device list", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_server") return "JP";
      if (cmd === "check_adb") return { connected: false, deviceName: null };
      if (cmd === "connect_adb_port") return "127.0.0.1:5565";
      if (cmd === "refresh_adb_devices_with_previews") return [];
      return null;
    });
    const user = userEvent.setup();
    renderWithTheme(<StatusBar />);

    await user.click(screen.getByRole("button", { name: /游戏未连接/ }));
    await user.click(screen.getByRole("button", { name: "选择设备" }));
    await screen.findByText("未检测到可用 ADB 设备");
    await user.click(screen.getByRole("button", { name: "手动添加" }));

    expect(await screen.findByRole("dialog", { name: "手动添加 ADB 设备" })).toBeInTheDocument();
    const input = screen.getByPlaceholderText("5555");
    await user.clear(input);
    await user.type(input, "5565");
    await user.click(screen.getByRole("button", { name: "添加" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("connect_adb_port", { port: 5565 });
    });
    expect(invoke).toHaveBeenCalledWith("refresh_adb_devices_with_previews", { refresh: false });
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

  it("shows local debug entries only after enabling debug logs in dev builds", async () => {
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
          { time: "12:34:57", message: "本地滚动诊断", level: "localDebug" },
        ]}
        operationLogOpen
      />
    );

    expect(screen.getByText("用户可见消息")).toBeInTheDocument();
    expect(screen.queryByText("本地滚动诊断")).not.toBeInTheDocument();
    expect(screen.getByText("操作日志 (1)")).toBeInTheDocument();

    await user.click(screen.getByRole("checkbox", { name: /显示调试/ }));
    expect(screen.getByText("本地滚动诊断")).toBeInTheDocument();
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
