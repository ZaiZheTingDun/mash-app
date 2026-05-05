import { describe, expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../test/renderWithTheme";
import { AssetBundleButton } from "../AssetBundleButton";

describe("AssetBundleButton", () => {
  it("does not import when the picker is cancelled", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "pick_asset_bundle") return null;
      return null;
    });
    const user = userEvent.setup();

    renderWithTheme(<AssetBundleButton onImported={vi.fn()} />);

    await user.click(screen.getByRole("button", { name: "导入素材包" }));

    expect(invoke).toHaveBeenCalledWith("pick_asset_bundle");
    expect(invoke).not.toHaveBeenCalledWith("import_asset_bundle", expect.anything());
  });

  it("imports the selected zip after confirmation", async () => {
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    const onImported = vi.fn();
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "pick_asset_bundle") {
        return "/tmp/mash-assets.zip";
      }
      if (cmd === "import_asset_bundle") {
        expect(args).toEqual({ zipPath: "/tmp/mash-assets.zip" });
        return {
          importedServants: true,
          importedCraftEssences: true,
          servantFiles: 12,
          craftEssenceFiles: 8,
          installDir: "/tmp/assets",
        };
      }
      return null;
    });
    const user = userEvent.setup();

    renderWithTheme(<AssetBundleButton onImported={onImported} />);

    await user.click(screen.getByRole("button", { name: "导入素材包" }));

    expect(confirmSpy).toHaveBeenCalled();
    expect(await screen.findByText("导入完成：从者 12 个文件，礼装 8 个文件")).toBeInTheDocument();
    expect(onImported).toHaveBeenCalledTimes(1);
    confirmSpy.mockRestore();
  });

  it("does not import when the confirmation is rejected", async () => {
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(false);
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "pick_asset_bundle") {
        return "/tmp/mash-assets.zip";
      }
      return null;
    });
    const user = userEvent.setup();

    renderWithTheme(<AssetBundleButton onImported={vi.fn()} />);

    await user.click(screen.getByRole("button", { name: "导入素材包" }));

    expect(confirmSpy).toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalledWith("import_asset_bundle", expect.anything());
    confirmSpy.mockRestore();
  });
});
