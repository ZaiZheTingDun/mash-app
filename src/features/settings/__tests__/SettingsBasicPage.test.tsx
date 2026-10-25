import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { invoke } from "../../../tauri";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { SettingsBasicPage } from "../SettingsBasicPage";
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
};

describe("SettingsBasicPage", () => {
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
