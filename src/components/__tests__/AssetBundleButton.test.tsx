import { describe, expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../test/renderWithTheme";
import { AssetBundleButton } from "../AssetBundleButton";
import type { AssetBundleStatus } from "../../types/assets";

function assetStatus(overrides: Partial<AssetBundleStatus> = {}): AssetBundleStatus {
  return {
    installed: false,
    importedServants: true,
    importedCraftEssences: true,
    servantFiles: 12,
    craftEssenceFiles: 8,
    installDir: "/tmp/mash-assets",
    currentVersion: 1,
    appAssetsVersion: 2,
    remoteLatestVersion: null,
    remoteLatestBaseVersion: null,
    targetVersion: 2,
    updateAvailable: true,
    updateDownloadSize: 0,
    updatePlan: "pending",
    latestUrl: "https://mash.xiaotongx.com/mash/assets/latest.json",
    remoteManifestUrl: null,
    updateCheckError: null,
    ...overrides,
  };
}

describe("AssetBundleButton", () => {
  it("does not import when the picker is cancelled", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_asset_bundle_status") return assetStatus();
      if (cmd === "pick_asset_bundle") return null;
      return null;
    });
    const user = userEvent.setup();

    renderWithTheme(<AssetBundleButton onImported={vi.fn()} />);

    await user.click(await screen.findByRole("button", { name: "手动导入" }));

    expect(invoke).toHaveBeenCalledWith("pick_asset_bundle");
    expect(invoke).not.toHaveBeenCalledWith("import_asset_bundle", expect.anything());
  });

  it("imports the selected zip after confirmation", async () => {
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    const onImported = vi.fn();
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "get_asset_bundle_status") return assetStatus();
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

    await user.click(await screen.findByRole("button", { name: "手动导入" }));

    expect(confirmSpy).toHaveBeenCalled();
    expect(await screen.findByText("导入完成：从者 12 个文件，礼装 8 个文件")).toBeInTheDocument();
    expect(onImported).toHaveBeenCalledTimes(1);
    confirmSpy.mockRestore();
  });

  it("does not import when the confirmation is rejected", async () => {
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(false);
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_asset_bundle_status") return assetStatus();
      if (cmd === "pick_asset_bundle") {
        return "/tmp/mash-assets.zip";
      }
      return null;
    });
    const user = userEvent.setup();

    renderWithTheme(<AssetBundleButton onImported={vi.fn()} />);

    await user.click(await screen.findByRole("button", { name: "手动导入" }));

    expect(confirmSpy).toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalledWith("import_asset_bundle", expect.anything());
    confirmSpy.mockRestore();
  });

  it("shows local manifest target version and downloads remote asset bundles", async () => {
    const onImported = vi.fn();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_asset_bundle_status") return assetStatus();
      if (cmd === "download_asset_bundles") {
        return {
          installed: true,
          installedVersion: 2,
          plan: "patch",
          servantFiles: 12,
          craftEssenceFiles: 8,
          installDir: "/tmp/mash-assets",
        };
      }
      return null;
    });
    const user = userEvent.setup();

    renderWithTheme(<AssetBundleButton onImported={onImported} />);

    expect(await screen.findByText("素材包需要更新：v1 → v2")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "在线更新" }));

    expect(invoke).toHaveBeenCalledWith("download_asset_bundles");
    expect(await screen.findByText("安装完成：素材包 v2")).toBeInTheDocument();
    expect(onImported).toHaveBeenCalledTimes(1);
  });
});
