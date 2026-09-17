import { screen, waitFor } from "@testing-library/react";
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
