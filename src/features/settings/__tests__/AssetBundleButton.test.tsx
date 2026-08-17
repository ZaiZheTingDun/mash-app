import { describe, expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { listen, type Event } from "@tauri-apps/api/event";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { AssetBundleButton } from "../AssetBundleButton";
import type {
  AssetBundleImportResult,
  AssetBundleStatus,
  AssetDownloadProgress,
} from "../../../types/assets";

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
  it("imports a local asset bundle", async () => {
    const onImported = vi.fn();
    let finishImport!: () => void;
    const importPromise = new Promise<AssetBundleImportResult>((resolve) => {
      finishImport = () => resolve({
        importedServants: true,
        importedCraftEssences: true,
        servantFiles: 12,
        craftEssenceFiles: 8,
        installDir: "/tmp/mash-assets",
      });
    });
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_asset_bundle_status") return assetStatus();
      if (cmd === "pick_asset_bundle") return "/tmp/mash-assets-v2.zip";
      if (cmd === "import_asset_bundle") return importPromise;
      return null;
    });
    const user = userEvent.setup();

    renderWithTheme(<AssetBundleButton onImported={onImported} />);

    expect(await screen.findByRole("button", { name: "在线更新" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "从本地导入" }));

    expect(invoke).toHaveBeenCalledWith("pick_asset_bundle");
    expect(invoke).toHaveBeenCalledWith("import_asset_bundle", {
      zipPath: "/tmp/mash-assets-v2.zip",
    });
    expect(await screen.findByRole("button", { name: "导入中…" })).toBeInTheDocument();
    expect(screen.queryByText("下载中…")).not.toBeInTheDocument();

    finishImport();
    expect(await screen.findByText("导入完成：素材包已安装。")).toBeInTheDocument();
    expect(onImported).toHaveBeenCalledTimes(1);
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

  it("cancels an online asset update", async () => {
    let rejectDownload!: (reason: unknown) => void;
    const downloadPromise = new Promise<never>((_resolve, reject) => {
      rejectDownload = reject;
    });
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_asset_bundle_status") return assetStatus();
      if (cmd === "download_asset_bundles") return downloadPromise;
      if (cmd === "cancel_asset_operation") {
        rejectDownload("下载已取消");
        return null;
      }
      return null;
    });
    const user = userEvent.setup();

    renderWithTheme(<AssetBundleButton />);

    await user.click(await screen.findByRole("button", { name: "在线更新" }));
    expect(await screen.findByRole("button", { name: "取消在线更新" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "取消在线更新" }));

    expect(invoke).toHaveBeenCalledWith("cancel_asset_operation");
    expect(await screen.findByText("在线更新已取消。")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "在线更新" })).toBeEnabled();
  });

  it("cancels a local asset import", async () => {
    let rejectImport!: (reason: unknown) => void;
    const importPromise = new Promise<never>((_resolve, reject) => {
      rejectImport = reject;
    });
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_asset_bundle_status") return assetStatus();
      if (cmd === "pick_asset_bundle") return "/tmp/mash-assets-v2.zip";
      if (cmd === "import_asset_bundle") return importPromise;
      if (cmd === "cancel_asset_operation") {
        rejectImport("导入已取消");
        return null;
      }
      return null;
    });
    const user = userEvent.setup();

    renderWithTheme(<AssetBundleButton />);

    await user.click(await screen.findByRole("button", { name: "从本地导入" }));
    expect(await screen.findByRole("button", { name: "取消导入" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "取消导入" }));

    expect(invoke).toHaveBeenCalledWith("cancel_asset_operation");
    expect(await screen.findByText("导入已取消。")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "从本地导入" })).toBeEnabled();
  });

  it("force-downloads the base bundle when reinstalling current assets", async () => {
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    const onImported = vi.fn();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_asset_bundle_status") {
        return assetStatus({
          installed: true,
          currentVersion: 2,
          targetVersion: null,
          updateAvailable: false,
          updatePlan: "none",
        });
      }
      if (cmd === "download_asset_bundles") {
        return {
          installed: true,
          installedVersion: 2,
          plan: "force-base",
          servantFiles: 12,
          craftEssenceFiles: 8,
          installDir: "/tmp/mash-assets",
        };
      }
      return null;
    });
    const user = userEvent.setup();

    renderWithTheme(<AssetBundleButton onImported={onImported} />);

    await user.click(await screen.findByRole("button", { name: "重新下载" }));

    expect(confirmSpy).toHaveBeenCalled();
    expect(invoke).toHaveBeenCalledWith("download_asset_bundles", { forceBase: true });
    expect(await screen.findByText("重新下载完成：素材包 v2")).toBeInTheDocument();
    expect(onImported).toHaveBeenCalledTimes(1);
    confirmSpy.mockRestore();
  });

  it("does not reinstall current assets when the confirmation is rejected", async () => {
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(false);
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_asset_bundle_status") {
        return assetStatus({
          installed: true,
          currentVersion: 2,
          targetVersion: null,
          updateAvailable: false,
          updatePlan: "none",
        });
      }
      return null;
    });
    const user = userEvent.setup();

    renderWithTheme(<AssetBundleButton onImported={vi.fn()} />);

    await user.click(await screen.findByRole("button", { name: "重新下载" }));

    expect(confirmSpy).toHaveBeenCalled();
    expect(invoke).not.toHaveBeenCalledWith(
      "download_asset_bundles",
      expect.anything()
    );
    confirmSpy.mockRestore();
  });

  it("keeps the progress bar determinate when asset download finishes", async () => {
    const progressHandlers: Array<(event: Event<AssetDownloadProgress>) => void> = [];
    vi.mocked(listen).mockImplementation(async (_event, handler) => {
      progressHandlers.push(handler as (event: Event<AssetDownloadProgress>) => void);
      return () => {};
    });
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_asset_bundle_status") return assetStatus();
      return null;
    });

    renderWithTheme(<AssetBundleButton />);

    await screen.findByRole("button", { name: "在线更新" });
    progressHandlers[0]({
      event: "asset-download-progress",
      id: 1,
      payload: {
        kind: "base",
        phase: "downloaded",
        downloadedBytes: 0,
        totalBytes: null,
        bytesPerSecond: null,
        etaSeconds: null,
      },
    });

    expect(await screen.findByRole("progressbar")).toHaveAttribute("value", "100");
  });
});
