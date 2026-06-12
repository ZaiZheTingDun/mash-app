import { describe, expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { listen, type Event } from "@tauri-apps/api/event";
import { renderWithTheme } from "../../test/renderWithTheme";
import { RuntimeBundleButton } from "../RuntimeBundleButton";
import type { RuntimeDownloadProgress, RuntimeStatus } from "../../types/runtime";

function runtimeStatus(overrides: Partial<RuntimeStatus> = {}): RuntimeStatus {
  return {
    requiredRuntimeVersion: "2026.05.08-runtime1",
    installedRuntimeVersion: null,
    runtimeInstalled: false,
    requiredCodeVersion: "2026.05.08-code1",
    installedCodeVersion: null,
    codeInstalled: false,
    installed: false,
    platform: "darwin-aarch64",
    runtimeDownloadUrl: "https://cdn.example.com/runtime.zip",
    runtimeExpectedSha256: "runtime-sha",
    runtimeInstallDir: "/tmp/runtime/base",
    executablePath: "/tmp/runtime/base/mash-cv-runtime/mash-cv",
    codeDownloadUrl: "https://cdn.example.com/code.zip",
    codeExpectedSha256: "code-sha",
    codeInstallDir: "/tmp/runtime/code",
    codePath: "/tmp/runtime/code/mash-cv-code",
    ...overrides,
  };
}

describe("RuntimeBundleButton", () => {
  it("shows missing runtime and code state with download urls", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_runtime_status") {
        return runtimeStatus();
      }
      return null;
    });

    renderWithTheme(<RuntimeBundleButton />);

    expect(
      await screen.findByText(
        "CV 运行时未就绪：需要 base 2026.05.08-runtime1，code 2026.05.08-code1"
      )
    ).toBeInTheDocument();
    expect(screen.getByText("base：https://cdn.example.com/runtime.zip")).toBeInTheDocument();
    expect(screen.getByText("code：https://cdn.example.com/code.zip")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "开始下载" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "手动上传" })).toBeInTheDocument();
  });

  it("shows installed runtime state", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_runtime_status") {
        return runtimeStatus({
          installedRuntimeVersion: "2026.05.08-runtime1",
          runtimeInstalled: true,
          installedCodeVersion: "2026.05.08-code1",
          codeInstalled: true,
          installed: true,
        });
      }
      return null;
    });

    renderWithTheme(<RuntimeBundleButton />);

    expect(
      await screen.findByText(
        "CV 运行时已安装：base 2026.05.08-runtime1 / code 2026.05.08-code1"
      )
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重新下载" })).toBeInTheDocument();
  });

  it("downloads missing runtime bundles and refreshes status", async () => {
    const onInstalled = vi.fn();
    let installed = false;
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_runtime_status") {
        return runtimeStatus({
          installedRuntimeVersion: installed ? "2026.05.08-runtime1" : null,
          runtimeInstalled: installed,
          installedCodeVersion: installed ? "2026.05.08-code1" : null,
          codeInstalled: installed,
          installed,
        });
      }
      if (cmd === "download_runtime_bundles") {
        installed = true;
        return {
          installed: [
            {
              installedKind: "runtime",
              installedVersion: "2026.05.08-runtime1",
              platform: "darwin-aarch64",
              installDir: "/tmp/runtime",
              executablePath: "/tmp/runtime/mash-cv/mash-cv",
              codePath: null,
            },
            {
              installedKind: "code",
              installedVersion: "2026.05.08-code1",
              platform: "darwin-aarch64",
              installDir: "/tmp/code",
              executablePath: null,
              codePath: "/tmp/code/mash-cv-code",
            },
          ],
        };
      }
      return null;
    });
    const user = userEvent.setup();

    renderWithTheme(<RuntimeBundleButton onInstalled={onInstalled} />);

    await user.click(await screen.findByRole("button", { name: "开始下载" }));

    expect(
      await screen.findByText("安装完成：base 2026.05.08-runtime1 / code 2026.05.08-code1")
    ).toBeInTheDocument();
    expect(
      screen.getByText("CV 运行时已安装：base 2026.05.08-runtime1 / code 2026.05.08-code1")
    ).toBeInTheDocument();
    expect(onInstalled).toHaveBeenCalledTimes(1);
  });

  it("shows download progress speed and eta from runtime events", async () => {
    const progressHandlers: Array<(event: Event<RuntimeDownloadProgress>) => void> = [];
    vi.mocked(listen).mockImplementation(async (_event, handler) => {
      progressHandlers.push(handler as (event: Event<RuntimeDownloadProgress>) => void);
      return () => {};
    });
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_runtime_status") {
        return runtimeStatus();
      }
      return null;
    });

    renderWithTheme(<RuntimeBundleButton />);

    await screen.findByRole("button", { name: "开始下载" });
    expect(progressHandlers).toHaveLength(1);
    progressHandlers[0]({
      event: "runtime-download-progress",
      id: 1,
      payload: {
        kind: "runtime",
        phase: "downloading",
        downloadedBytes: 5 * 1024 * 1024,
        totalBytes: 20 * 1024 * 1024,
        bytesPerSecond: 2.5 * 1024 * 1024,
        etaSeconds: 6,
      },
    });

    expect(
      await screen.findByText("正在下载 base：5.0 MB / 20.0 MB · 2.5 MB/s · 预计剩余 6秒")
    ).toBeInTheDocument();
  });

  it("keeps the progress bar determinate when runtime download finishes", async () => {
    const progressHandlers: Array<(event: Event<RuntimeDownloadProgress>) => void> = [];
    vi.mocked(listen).mockImplementation(async (_event, handler) => {
      progressHandlers.push(handler as (event: Event<RuntimeDownloadProgress>) => void);
      return () => {};
    });
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_runtime_status") {
        return runtimeStatus();
      }
      return null;
    });

    renderWithTheme(<RuntimeBundleButton />);

    await screen.findByRole("button", { name: "开始下载" });
    progressHandlers[0]({
      event: "runtime-download-progress",
      id: 1,
      payload: {
        kind: "runtime",
        phase: "downloaded",
        downloadedBytes: 0,
        totalBytes: null,
        bytesPerSecond: null,
        etaSeconds: null,
      },
    });

    expect(await screen.findByRole("progressbar")).toHaveAttribute("value", "100");
  });

  it("imports a selected runtime zip from manual upload and refreshes status", async () => {
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    const onInstalled = vi.fn();
    let installed = false;
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "get_runtime_status") {
        return runtimeStatus({
          installedRuntimeVersion: installed ? "2026.05.08-runtime1" : null,
          runtimeInstalled: installed,
          installedCodeVersion: installed ? "2026.05.08-code1" : null,
          codeInstalled: installed,
          installed,
        });
      }
      if (cmd === "pick_runtime_bundle") {
        return "/tmp/mash-cv.zip";
      }
      if (cmd === "import_runtime_bundle") {
        expect(args).toEqual({ zipPath: "/tmp/mash-cv.zip" });
        installed = true;
        return {
          installedKind: "runtime",
          installedVersion: "2026.05.08-runtime1",
          platform: "darwin-aarch64",
          installDir: "/tmp/runtime",
          executablePath: "/tmp/runtime/mash-cv/mash-cv",
          codePath: null,
        };
      }
      return null;
    });
    const user = userEvent.setup();

    renderWithTheme(<RuntimeBundleButton onInstalled={onInstalled} />);

    await user.click(await screen.findByRole("button", { name: "手动上传" }));

    expect(confirmSpy).toHaveBeenCalled();
    expect(await screen.findByText("安装完成：base 2026.05.08-runtime1")).toBeInTheDocument();
    expect(
      screen.getByText("CV 运行时已安装：base 2026.05.08-runtime1 / code 2026.05.08-code1")
    ).toBeInTheDocument();
    expect(onInstalled).toHaveBeenCalledTimes(1);
    confirmSpy.mockRestore();
  });

  it("shows import errors", async () => {
    const confirmSpy = vi.spyOn(window, "confirm").mockReturnValue(true);
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_runtime_status") {
        return runtimeStatus();
      }
      if (cmd === "pick_runtime_bundle") {
        return "/tmp/mash-cv.zip";
      }
      if (cmd === "import_runtime_bundle") {
        throw new Error("sha256 不匹配");
      }
      return null;
    });
    const user = userEvent.setup();

    renderWithTheme(<RuntimeBundleButton />);

    await user.click(await screen.findByRole("button", { name: "手动上传" }));

    expect(await screen.findByText("安装失败：Error: sha256 不匹配")).toBeInTheDocument();
    confirmSpy.mockRestore();
  });
});
