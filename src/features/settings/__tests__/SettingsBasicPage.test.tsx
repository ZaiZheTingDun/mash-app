import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { invoke } from "../../../tauri";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { SettingsBasicPage } from "../SettingsBasicPage";
import type { RecognitionSettings } from "../../../types/recognition";

vi.mock("../../../tauri", () => ({ invoke: vi.fn() }));

const SETTINGS: RecognitionSettings = {
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
});
