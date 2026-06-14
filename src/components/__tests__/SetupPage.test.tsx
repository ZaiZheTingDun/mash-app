import { describe, expect, it, vi } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../test/renderWithTheme";
import { SetupPage } from "../SetupPage";
import type { AssetBundleStatus } from "../../types/assets";
import type { RuntimeStatus } from "../../types/runtime";

function runtimeStatus(installed: boolean): RuntimeStatus {
  return {
    requiredRuntimeVersion: "2026.05.08-runtime1",
    installedRuntimeVersion: installed ? "2026.05.08-runtime1" : null,
    runtimeInstalled: installed,
    requiredCodeVersion: "2026.05.08-code1",
    installedCodeVersion: installed ? "2026.05.08-code1" : null,
    codeInstalled: installed,
    installed,
    platform: "darwin-aarch64",
    runtimeDownloadUrl: "https://cdn.example.com/runtime.zip",
    runtimeExpectedSha256: "runtime-sha",
    runtimeInstallDir: "/tmp/runtime/base",
    executablePath: "/tmp/runtime/base/mash-cv-runtime/mash-cv",
    codeDownloadUrl: "https://cdn.example.com/code.zip",
    codeExpectedSha256: "code-sha",
    codeInstallDir: "/tmp/runtime/code",
    codePath: "/tmp/runtime/code/mash-cv-code",
  };
}

function assetStatus(installed: boolean): AssetBundleStatus {
  return {
    installed,
    importedServants: installed,
    importedCraftEssences: installed,
    servantFiles: installed ? 12 : 0,
    craftEssenceFiles: installed ? 8 : 0,
    installDir: "/tmp/mash-assets",
    currentVersion: installed ? 2 : null,
    appAssetsVersion: 2,
    remoteLatestVersion: null,
    remoteLatestBaseVersion: null,
    targetVersion: null,
    updateAvailable: false,
    updateDownloadSize: 0,
    updatePlan: "none",
    latestUrl: "https://mash.xiaotongx.com/mash/assets/latest.json",
    remoteManifestUrl: null,
    updateCheckError: null,
  };
}

function staleAssetStatus(): AssetBundleStatus {
  return {
    ...assetStatus(false),
    importedServants: true,
    importedCraftEssences: true,
    servantFiles: 12,
    craftEssenceFiles: 8,
    currentVersion: 1,
    targetVersion: 2,
    updateAvailable: true,
    updatePlan: "pending",
  };
}

describe("SetupPage", () => {
  it("calls onReady when runtime and assets are installed", async () => {
    const onReady = vi.fn();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_runtime_status") return runtimeStatus(true);
      if (cmd === "get_asset_bundle_status") return assetStatus(true);
      return null;
    });

    renderWithTheme(<SetupPage onReady={onReady} />);

    await waitFor(() => expect(onReady).toHaveBeenCalledTimes(1));
  });

  it("shows setup requirements when anything is missing", async () => {
    const onReady = vi.fn();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_runtime_status") return runtimeStatus(false);
      if (cmd === "get_asset_bundle_status") return assetStatus(false);
      return null;
    });

    renderWithTheme(<SetupPage onReady={onReady} />);

    expect(await screen.findByText("资源管理")).toBeInTheDocument();
    expect(screen.getByText("需要安装 runtime base 和 code 包")).toBeInTheDocument();
    expect(
      screen.getByText("需要导入包含 assets/servants 和 assets/ces 的素材包")
    ).toBeInTheDocument();
    expect(onReady).not.toHaveBeenCalled();
  });

  it("keeps setup open when assets need a configured-version update", async () => {
    const onReady = vi.fn();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_runtime_status") return runtimeStatus(true);
      if (cmd === "get_asset_bundle_status") return staleAssetStatus();
      return null;
    });

    renderWithTheme(<SetupPage onReady={onReady} />);

    expect(await screen.findByText("需要更新素材包 v1 → v2")).toBeInTheDocument();
    expect(onReady).not.toHaveBeenCalled();
  });

  it("keeps management mode open even when runtime and assets are installed", async () => {
    const onReady = vi.fn();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_runtime_status") return runtimeStatus(true);
      if (cmd === "get_asset_bundle_status") return assetStatus(true);
      return null;
    });

    renderWithTheme(<SetupPage mode="manage" onReady={onReady} />);

    expect(await screen.findByText("资源管理")).toBeInTheDocument();
    expect(screen.getByText("已安装运行时 base 和 code 包")).toBeInTheDocument();
    expect(onReady).not.toHaveBeenCalled();
  });

  it("checks each resource status once when management mode mounts", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_runtime_status") return runtimeStatus(true);
      if (cmd === "get_asset_bundle_status") return assetStatus(true);
      return null;
    });

    renderWithTheme(<SetupPage mode="manage" />);

    await screen.findByText("已安装运行时 base 和 code 包");
    expect(vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "get_runtime_status"))
      .toHaveLength(1);
    expect(vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "get_asset_bundle_status"))
      .toHaveLength(1);
  });

  it("calls onBack from the management cancel button", async () => {
    const onBack = vi.fn();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_runtime_status") return runtimeStatus(true);
      if (cmd === "get_asset_bundle_status") return assetStatus(true);
      return null;
    });
    const user = userEvent.setup();

    renderWithTheme(<SetupPage mode="manage" onBack={onBack} />);

    await user.click(await screen.findByRole("button", { name: "取消" }));

    expect(invoke).toHaveBeenCalledWith("cancel_resource_downloads");
    expect(onBack).toHaveBeenCalledTimes(1);
  });

  it("calls onBack from the management done button without cancelling downloads", async () => {
    const onBack = vi.fn();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_runtime_status") return runtimeStatus(true);
      if (cmd === "get_asset_bundle_status") return assetStatus(true);
      return null;
    });
    const user = userEvent.setup();

    renderWithTheme(<SetupPage mode="manage" onBack={onBack} />);

    await user.click(await screen.findByRole("button", { name: "完成" }));

    expect(invoke).not.toHaveBeenCalledWith("cancel_resource_downloads");
    expect(onBack).toHaveBeenCalledTimes(1);
  });

  it("disables the management done button while a resource download is running", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_runtime_status") return Promise.resolve(runtimeStatus(true));
      if (cmd === "get_asset_bundle_status") return Promise.resolve(staleAssetStatus());
      if (cmd === "download_asset_bundles") {
        return new Promise(() => {});
      }
      return Promise.resolve(null);
    });
    const user = userEvent.setup();

    renderWithTheme(<SetupPage mode="manage" onBack={vi.fn()} />);

    const done = await screen.findByRole("button", { name: "完成" });
    expect(done).toBeEnabled();
    await user.click(await screen.findByRole("button", { name: "在线更新" }));

    expect(done).toBeDisabled();
  });
});
