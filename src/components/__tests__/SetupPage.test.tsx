import { describe, expect, it, vi } from "vitest";
import { screen, waitFor } from "@testing-library/react";
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

    expect(await screen.findByText("初始化 mash")).toBeInTheDocument();
    expect(screen.getByText("需要安装 runtime base 和 code 包")).toBeInTheDocument();
    expect(
      screen.getByText("需要导入包含 assets/servants 和 assets/ces 的素材包")
    ).toBeInTheDocument();
    expect(onReady).not.toHaveBeenCalled();
  });
});
