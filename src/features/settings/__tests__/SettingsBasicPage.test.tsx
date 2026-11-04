import { useState } from "react";
import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { invoke } from "../../../tauri";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { SettingsBasicPage } from "../SettingsBasicPage";
import { SettingsPage } from "../SettingsPage";
import type { RecognitionSettings } from "../../../types/recognition";

vi.mock("../../../tauri", () => ({ invoke: vi.fn() }));

const SETTINGS: RecognitionSettings = {
  autoFriendRequest: false,
  noblePhantasmDetectionMode: "card",
  supportCeThreshold: 0.7,
  supportCeFullGateThreshold: 0.6,
  supportMlbIconThreshold: 0.7,
  supportBondIconThreshold: 0.7,
  stopOnBondLevelUp: false,
  stopOnBondMaxLevel: false,
  autoCaptureBondLevelUp: false,
  verifySkillActivation: false,
  enableExtraClassFilter: true,
  supportFullListOcrFallback: false,
  unknownScreenTimeoutCount: 100,
  apRecoveryConfirmWaitMs: 1000,
};

function EscapeSettingsHarness() {
  const [open, setOpen] = useState(true);
  return <>
    <button onClick={() => setOpen(true)}>打开设置</button>
    <SettingsPage open={open} section="basic" onOpenChange={setOpen} onSectionChange={vi.fn()} />
  </>;
}

describe("SettingsBasicPage", () => {
  it("saves the recovery delay while typing and retains it after Escape closes settings", async () => {
    let waitMs = 1000;
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_recognition_settings") {
        return { ...SETTINGS, apRecoveryConfirmWaitMs: waitMs };
      }
      if (command === "set_ap_recovery_confirm_wait_ms") {
        waitMs = (args as { value: number }).value;
        return { ...SETTINGS, apRecoveryConfirmWaitMs: waitMs };
      }
      return null;
    });
    const user = userEvent.setup();
    renderWithTheme(<EscapeSettingsHarness />);
    const input = await screen.findByRole("spinbutton", { name: "回复道具确认等待时长" });
    await waitFor(() => expect(input).toHaveValue(1000));
    await user.clear(input);
    await user.type(input, "1750");
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_ap_recovery_confirm_wait_ms", { value: 1750 });
      expect(input).not.toBeDisabled();
    });
    expect(input).toHaveFocus();
    expect(await screen.findByText("已自动保存")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("spinbutton", { name: "回复道具确认等待时长" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "打开设置" }));
    await waitFor(() => expect(screen.getByRole("spinbutton", {
      name: "回复道具确认等待时长",
    })).toHaveValue(1750));
  });

  it("reports a failed delay save and allows retrying with the 500 ms minimum", async () => {
    let rejectSave = true;
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_recognition_settings") return SETTINGS;
      if (command === "set_ap_recovery_confirm_wait_ms") {
        if (rejectSave) throw new Error("无法保存设置");
        return { ...SETTINGS, apRecoveryConfirmWaitMs: (args as { value: number }).value };
      }
      return null;
    });
    const user = userEvent.setup();
    renderWithTheme(<SettingsBasicPage active />);
    const input = await screen.findByRole("spinbutton", { name: "回复道具确认等待时长" });
    await waitFor(() => expect(input).toHaveValue(1000));
    await user.clear(input);
    await user.type(input, "2000");
    expect(await screen.findByText("Error: 无法保存设置")).toBeInTheDocument();
    expect(input).not.toBeDisabled();
    rejectSave = false;
    await user.clear(input);
    await user.type(input, "500");
    expect(await screen.findByText("已自动保存")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("set_ap_recovery_confirm_wait_ms", { value: 500 });
    expect(input).toHaveValue(500);
  });

  it("keeps incomplete or invalid delay drafts out of persisted settings", async () => {
    vi.mocked(invoke).mockImplementation(async (command) =>
      command === "get_recognition_settings" ? SETTINGS : null
    );
    const user = userEvent.setup();
    renderWithTheme(<SettingsBasicPage active />);
    const input = await screen.findByRole("spinbutton", { name: "回复道具确认等待时长" });
    await waitFor(() => expect(input).toHaveValue(1000));
    expect(input).toHaveAttribute("min", "500");
    for (const value of ["", "0", "499", "60001", "500.5"]) {
      fireEvent.change(input, { target: { value } });
    }
    await user.tab();
    expect(vi.mocked(invoke).mock.calls.filter(([command]) =>
      command === "set_ap_recovery_confirm_wait_ms"
    )).toEqual([]);
  });

  it("serializes saves without disabling typing or overwriting a newer draft", async () => {
    const saves: Array<{ value: number; resolve: (settings: RecognitionSettings) => void }> = [];
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_recognition_settings") return SETTINGS;
      if (command === "set_ap_recovery_confirm_wait_ms") {
        return new Promise<RecognitionSettings>((resolve) => {
          saves.push({ value: (args as { value: number }).value, resolve });
        });
      }
      return null;
    });
    renderWithTheme(<SettingsBasicPage active />);
    const input = await screen.findByRole("spinbutton", { name: "回复道具确认等待时长" });
    await waitFor(() => expect(input).toHaveValue(1000));
    fireEvent.change(input, { target: { value: "1500" } });
    await waitFor(() => expect(saves).toHaveLength(1));
    expect(input).not.toBeDisabled();
    fireEvent.change(input, { target: { value: "1750" } });
    fireEvent.change(input, { target: { value: "2000" } });
    expect(saves).toHaveLength(1);
    await act(async () => saves[0].resolve({ ...SETTINGS, apRecoveryConfirmWaitMs: 1500 }));
    expect(input).toHaveValue(2000);
    expect(saves[1].value).toBe(1750);
    await act(async () => saves[1].resolve({ ...SETTINGS, apRecoveryConfirmWaitMs: 1750 }));
    expect(input).toHaveValue(2000);
    expect(saves[2].value).toBe(2000);
    await act(async () => saves[2].resolve({ ...SETTINGS, apRecoveryConfirmWaitMs: 2000 }));
    expect(await screen.findByText("已自动保存")).toBeInTheDocument();
  });

  it("loads the saved skill layout and only updates the app after a successful backend save", async () => {
    let layout = "guided";
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_recognition_settings") return SETTINGS;
      if (command === "get_skill_editor_layout") return layout;
      if (command === "set_skill_editor_layout") { layout = (args as {value: string}).value; return layout; }
      return null;
    });
    const change = vi.fn(); const user = userEvent.setup();
    const { unmount } = renderWithTheme(<SettingsBasicPage active onSkillEditorLayoutChange={change} />);
    await user.click(await screen.findByRole("combobox", { name: "技能设置布局" }));
    await user.click(screen.getByRole("option", { name: "简易设置" }));
    await waitFor(() => expect(change).toHaveBeenCalledWith("simple"));
    expect(invoke).toHaveBeenCalledWith("set_skill_editor_layout", { value: "simple" });
    unmount();
    renderWithTheme(<SettingsBasicPage active onSkillEditorLayoutChange={change} />);
    await waitFor(() => expect(screen.getByRole("combobox", { name: "技能设置布局" })).toHaveTextContent("简易设置"));
    vi.mocked(invoke).mockImplementation(async () => { throw new Error("保存失败"); });
    await user.click(screen.getByRole("combobox", { name: "技能设置布局" }));
    await user.click(screen.getByRole("option", { name: "逐步设置" }));
    expect(await screen.findByText("Error: 保存失败")).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "技能设置布局" })).toHaveTextContent("简易设置");
    expect(change).toHaveBeenCalledTimes(1);
  });

  it("shows a failed read with a retry action instead of an endless loading placeholder", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => command === "get_recognition_settings" ? SETTINGS : null);
    const retry = vi.fn().mockResolvedValue(undefined);
    const user = userEvent.setup();
    renderWithTheme(<SettingsBasicPage active colorThemeError="主题配色读取失败，请重试。" onColorThemeReload={retry} />);
    expect(screen.getByRole("combobox", { name: "主题配色" })).toHaveTextContent("读取失败");
    expect(screen.getByRole("alert")).toHaveTextContent("主题配色读取失败，请重试。");
    expect(screen.queryByText("正在读取…")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "重试" }));
    expect(retry).toHaveBeenCalledOnce();
  });

  it.each([
    ["blue", "蓝色"], ["violet", "紫色"], ["gold", "香槟金"],
  ] as const)("offers the %s theme in settings", async (value, label) => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_recognition_settings") return SETTINGS;
      return null;
    });
    const change = vi.fn().mockResolvedValue(undefined);
    const user = userEvent.setup();
    renderWithTheme(<SettingsBasicPage active colorTheme="violet" onColorThemeChange={change} />);
    await user.click(screen.getByRole("combobox", { name: "主题配色" }));
    await user.click(screen.getByRole("option", { name: label }));
    if (value !== "violet") {
      expect(change).toHaveBeenCalledWith(value);
      expect(await screen.findByText("已保存")).toBeInTheDocument();
    } else {
      expect(screen.getByRole("combobox", { name: "主题配色" })).toHaveTextContent("紫色");
    }
  });

  it("blocks repeated changes during saving and retains the selected theme after failure", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => command === "get_recognition_settings" ? SETTINGS : null);
    let fail!: (error: Error) => void;
    const change = vi.fn(() => new Promise<void>((_, reject) => { fail = reject; }));
    const user = userEvent.setup();
    renderWithTheme(<SettingsBasicPage active colorTheme="violet" onColorThemeChange={change} />);
    const select = screen.getByRole("combobox", { name: "主题配色" });
    await user.click(select);
    await user.click(screen.getByRole("option", { name: "香槟金" }));
    expect(select).toBeDisabled();
    await act(async () => { fail(new Error("无法保存界面设置")); });
    expect(await screen.findByText("Error: 无法保存界面设置")).toBeInTheDocument();
    expect(select).not.toBeDisabled();
    expect(select).toHaveTextContent("紫色");
  });

  it("keeps the Mystic Code gender visible when automatic friend requests are gated off", async () => {
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "get_recognition_settings") return SETTINGS;
      if (command === "get_mystic_code_gender") return "female";
      if (command === "get_battle_start_panel") return "operationLog";
      return null;
    });

    renderWithTheme(<SettingsBasicPage active />);

    expect(await screen.findByRole("combobox", {
      name: "御主礼装显示性别",
    })).toBeInTheDocument();
    expect(screen.queryByRole("switch", {
      name: "自动申请好友",
    })).not.toBeInTheDocument();
  });

  it("persists the full-list support OCR fallback toggle", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_recognition_settings") return SETTINGS;
      if (command === "set_support_full_list_ocr_fallback") {
        expect(args).toEqual({ value: true });
        return { ...SETTINGS, supportFullListOcrFallback: true };
      }
      return SETTINGS;
    });

    renderWithTheme(<SettingsBasicPage active />);

    const fallback = await screen.findByRole("switch", {
      name: "使用全列表 OCR",
    });
    expect(fallback).not.toBeChecked();

    await user.click(fallback);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_support_full_list_ocr_fallback", {
        value: true,
      });
    });
    expect(fallback).toBeChecked();
  });

  it("persists the panel opened after battle automation starts", async () => {
    const user = userEvent.setup();
    const onBattleStartPanelChange = vi.fn();
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_recognition_settings") return SETTINGS;
      if (command === "get_battle_start_panel") return "operationLog";
      if (command === "set_battle_start_panel") {
        expect(args).toEqual({ value: "runStatus" });
        return "runStatus";
      }
      return null;
    });

    renderWithTheme(
      <SettingsBasicPage
        active
        onBattleStartPanelChange={onBattleStartPanelChange}
      />
    );

    const panelSelect = await screen.findByRole("combobox", {
      name: "开始后展开",
    });
    expect(panelSelect).toHaveTextContent("操作日志");

    await user.click(panelSelect);
    await user.click(await screen.findByText("运行状态"));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_battle_start_panel", {
        value: "runStatus",
      });
    });
    expect(onBattleStartPanelChange).toHaveBeenCalledWith("runStatus");
  });
});
