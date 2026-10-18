import { describe, expect, it, vi } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { EmulatorSelector } from "../EmulatorSelector";
import type { EmulatorMode, EmulatorOptions } from "../../../types/emulator";

describe("EmulatorSelector", () => {
  it("shows discovered roots and saves a manual selection through the backend", async () => {
    let mode: EmulatorMode = "auto";
    const root = "C:\\Program Files\\Netease\\MuMu";
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "supports_emulator_selection") return true;
      if (command === "select_emulator") {
        mode = (args as { mode: EmulatorMode }).mode;
        return null;
      }
      if (command === "get_emulator_options") {
        return {
          supported: true,
          mode,
          selectedPath: mode === "mumu" ? root : null,
          installations: [root],
          scanError: null,
          otherOverrideReady: false,
        } satisfies EmulatorOptions;
      }
      return null;
    });
    renderWithTheme(<EmulatorSelector disabled={false} open onOpenChange={() => {}} onSupportChange={() => {}} />);
    await userEvent.click(await screen.findByRole("radio", { name: `MuMu ${root}` }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("select_emulator", { mode: "mumu", path: root }));
    await waitFor(() => expect(screen.getByRole("radio", { name: `MuMu ${root}` })).toHaveAttribute("aria-checked", "true"));
  });

  it("offers a one-time override only when Other is selected", async () => {
    let otherOverrideReady = false;
    vi.mocked(invoke).mockImplementation(async (command) => {
      if (command === "supports_emulator_selection") return true;
      if (command === "allow_other_emulator_once") {
        otherOverrideReady = true;
        return null;
      }
      if (command === "get_emulator_options") return {
        supported: true, mode: "other", selectedPath: null, installations: [], scanError: null, otherOverrideReady,
      } satisfies EmulatorOptions;
      return null;
    });
    renderWithTheme(<EmulatorSelector disabled={false} open onOpenChange={() => {}} onSupportChange={() => {}} />);
    await userEvent.click(await screen.findByRole("button", { name: "本次跳过 MuMu 检查" }));
    expect(invoke).toHaveBeenCalledWith("allow_other_emulator_once");
    expect(await screen.findByText("已允许下一次启动；请重新启动任务")).toBeInTheDocument();
  });
});
