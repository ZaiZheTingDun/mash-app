import { describe, it, expect, vi } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { CommandSpellOptionButtons } from "../CommandSpellOptionButtons";
import { convertFileSrc, invoke } from "../../../tauri";

vi.mock("../../../tauri", () => ({
  invoke: vi.fn(),
  convertFileSrc: vi.fn((path: string) => `asset://${path}`),
}));

describe("CommandSpellOptionButtons", () => {
  it("loads the resource icons and selects the corresponding command spell", async () => {
    vi.mocked(invoke).mockResolvedValue([
      { path: "/resources/icons/skill_00601.png", name: "宝具解放" },
      { path: "/resources/icons/skill_00600.png", name: "灵基修复" },
    ]);
    const onSelect = vi.fn();
    renderWithTheme(<CommandSpellOptionButtons onSelect={onSelect} />);
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("get_command_spell_icon_paths");
      expect(convertFileSrc).toHaveBeenCalledWith("/resources/icons/skill_00601.png");
      expect(convertFileSrc).toHaveBeenCalledWith("/resources/icons/skill_00600.png");
    });
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "宝具解放" }));
    await user.click(screen.getByRole("button", { name: "灵基修复" }));
    expect(onSelect.mock.calls).toEqual([["np_release"], ["restore"]]);
  });

  it.each(["missing", "unavailable"])("keeps both skills usable when icons are %s", async status => {
    if (status === "missing") {
      vi.mocked(invoke).mockResolvedValue([
        { path: null, name: "宝具解放" },
        { path: null, name: "灵基修复" },
      ]);
    } else {
      vi.mocked(invoke).mockRejectedValue(new Error("Resources unavailable"));
    }
    const onSelect = vi.fn();
    renderWithTheme(<CommandSpellOptionButtons onSelect={onSelect} />);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "宝具解放" }));
    await user.click(screen.getByRole("button", { name: "灵基修复" }));
    expect(onSelect.mock.calls).toEqual([["np_release"], ["restore"]]);
    expect(convertFileSrc).not.toHaveBeenCalled();
  });
});
