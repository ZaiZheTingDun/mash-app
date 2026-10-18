import { describe, expect, it, vi } from "vitest";
import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { listen, type Event } from "@tauri-apps/api/event";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { RuntimeBundleButton } from "../RuntimeBundleButton";
import type { RuntimeDownloadProgress, RuntimeInstallResult, RuntimeStatus } from "../../../types/runtime";

function runtimeStatus(overrides: Partial<RuntimeStatus> = {}): RuntimeStatus {
  return {
    requiredRuntimeVersion: "2026.05.08-runtime1",
    installedRuntimeVersion: null,
    runtimeInstalled: false,
    installed: false,
    platform: "darwin-aarch64",
    runtimeDownloadUrl: "https://cdn.example.com/runtime.zip",
    runtimeExpectedSha256: "runtime-sha",
    runtimeInstallDir: "/tmp/runtime/base",
    executablePath: "/tmp/runtime/base/mash-cv-runtime/mash-cv",
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
        "CV 运行时未就绪：需要 2026.05.08-runtime1"
      )
    ).toBeInTheDocument();
    expect(screen.getByText("https://cdn.example.com/runtime.zip")).toBeInTheDocument();
    expect(screen.queryByText("code：https://cdn.example.com/code.zip")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "开始下载" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "从本地导入" })).toBeEnabled();
  });

  it("shows installed runtime state", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_runtime_status") {
        return runtimeStatus({
          installedRuntimeVersion: "2026.05.08-runtime1",
          runtimeInstalled: true,
          installed: true,
        });
      }
      return null;
    });

    renderWithTheme(<RuntimeBundleButton />);

    expect(
      await screen.findByText(
        "CV 运行时已安装：2026.05.08-runtime1"
      )
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重新下载" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "从本地导入" })).toBeEnabled();
  });

  it("imports a local runtime ZIP and refreshes installation state", async () => {
    const onInstalled = vi.fn();
    const onBusyChange = vi.fn();
    let installed = false;
    let finishImport!: (result: RuntimeInstallResult) => void;
    const importPromise = new Promise<RuntimeInstallResult>((resolve) => {
      finishImport = resolve;
    });
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_runtime_status") return runtimeStatus({ installed, runtimeInstalled: installed });
      if (cmd === "pick_runtime_bundle") return "/tmp/CV 运行时.zip";
      if (cmd === "import_runtime_bundle") return importPromise;
      return null;
    });
    const user = userEvent.setup();
    renderWithTheme(<RuntimeBundleButton onInstalled={onInstalled} onBusyChange={onBusyChange} />);
    await user.click(await screen.findByRole("button", { name: "从本地导入" }));

    expect(invoke).toHaveBeenCalledWith("import_runtime_bundle", { zipPath: "/tmp/CV 运行时.zip" });
    expect(screen.getByRole("button", { name: "导入中…" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "开始下载" })).toBeDisabled();
    expect(onBusyChange).toHaveBeenLastCalledWith(true);
    expect(onInstalled).not.toHaveBeenCalled();

    await act(async () => {
      installed = true;
      finishImport({
        installedKind: "runtime",
        installedVersion: "2026.05.08-runtime1",
        platform: "darwin-aarch64",
        installDir: "/tmp/runtime",
        executablePath: "/tmp/runtime/mash-cv-runtime/mash-cv",
      });
    });

    expect(await screen.findByText("导入完成：2026.05.08-runtime1")).toBeInTheDocument();
    expect(screen.getByText("CV 运行时已安装：2026.05.08-runtime1")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重新下载" })).toBeEnabled();
    expect(onInstalled).toHaveBeenCalledTimes(1);
    expect(onBusyChange).toHaveBeenLastCalledWith(false);
  });

  it("does not import or notify when file selection is cancelled", async () => {
    const onInstalled = vi.fn();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_runtime_status") return runtimeStatus();
      return null;
    });
    const user = userEvent.setup();
    renderWithTheme(<RuntimeBundleButton onInstalled={onInstalled} />);
    await user.click(await screen.findByRole("button", { name: "从本地导入" }));

    expect(invoke).toHaveBeenCalledWith("pick_runtime_bundle");
    expect(invoke).not.toHaveBeenCalledWith("import_runtime_bundle", expect.anything());
    expect(onInstalled).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "从本地导入" })).toBeEnabled();
    expect(screen.queryByText(/导入失败/)).not.toBeInTheDocument();
  });

  it.each(["pick_runtime_bundle", "import_runtime_bundle"])(
    "shows an error and restores actions when %s fails",
    async (failingCommand) => {
      const onInstalled = vi.fn();
      vi.mocked(invoke).mockImplementation(async (cmd: string) => {
        if (cmd === "get_runtime_status") return runtimeStatus();
        if (cmd === failingCommand) throw new Error("无法导入运行时");
        if (cmd === "pick_runtime_bundle") return "/tmp/runtime.zip";
        return null;
      });
      const user = userEvent.setup();
      renderWithTheme(<RuntimeBundleButton onInstalled={onInstalled} />);
      await user.click(await screen.findByRole("button", { name: "从本地导入" }));

      expect(await screen.findByText("导入失败：Error: 无法导入运行时")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "从本地导入" })).toBeEnabled();
      expect(screen.getByRole("button", { name: "开始下载" })).toBeEnabled();
      expect(onInstalled).not.toHaveBeenCalled();
    }
  );

  it("blocks download and import while the file picker is open", async () => {
    let closePicker!: (path: null) => void;
    const pickerPromise = new Promise<null>((resolve) => { closePicker = resolve; });
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_runtime_status") return runtimeStatus();
      if (cmd === "pick_runtime_bundle") return pickerPromise;
      return null;
    });
    const user = userEvent.setup();
    renderWithTheme(<RuntimeBundleButton />);
    await user.click(await screen.findByRole("button", { name: "从本地导入" }));

    expect(screen.getByRole("button", { name: "开始下载" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "导入中…" })).toBeDisabled();
    await act(async () => { closePicker(null); });
    await waitFor(() => expect(screen.getByRole("button", { name: "从本地导入" })).toBeEnabled());
  });

  it("downloads missing runtime bundles and refreshes status", async () => {
    const onInstalled = vi.fn();
    let installed = false;
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_runtime_status") {
        return runtimeStatus({
          installedRuntimeVersion: installed ? "2026.05.08-runtime1" : null,
          runtimeInstalled: installed,
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
      await screen.findByText("安装完成：2026.05.08-runtime1")
    ).toBeInTheDocument();
    expect(
      screen.getByText("CV 运行时已安装：2026.05.08-runtime1")
    ).toBeInTheDocument();
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("download_runtime_bundles", { force: false });
    expect(onInstalled).toHaveBeenCalledTimes(1);
  });

  it("force-downloads installed runtime bundles when re-downloading", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_runtime_status") {
        return runtimeStatus({
          installedRuntimeVersion: "2026.05.08-runtime1",
          runtimeInstalled: true,
          installed: true,
        });
      }
      if (cmd === "download_runtime_bundles") {
        return {
          installed: [
            {
              installedKind: "runtime",
              installedVersion: "2026.05.08-runtime1",
              platform: "darwin-aarch64",
              installDir: "/tmp/runtime",
              executablePath: "/tmp/runtime/mash-cv/mash-cv",
            },
          ],
        };
      }
      return null;
    });
    const user = userEvent.setup();

    renderWithTheme(<RuntimeBundleButton />);

    await user.click(await screen.findByRole("button", { name: "重新下载" }));

    expect(vi.mocked(invoke)).toHaveBeenCalledWith("download_runtime_bundles", { force: true });
    expect(
      await screen.findByText("重新下载完成：2026.05.08-runtime1")
    ).toBeInTheDocument();
  });

  it("clears progress after a successful runtime download", async () => {
    const progressHandlers: Array<(event: Event<RuntimeDownloadProgress>) => void> = [];
    vi.mocked(listen).mockImplementation(async (_event, handler) => {
      progressHandlers.push(handler as (event: Event<RuntimeDownloadProgress>) => void);
      return () => {};
    });
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_runtime_status") {
        return runtimeStatus({
          installedRuntimeVersion: "2026.05.08-runtime1",
          runtimeInstalled: true,
          installed: true,
        });
      }
      if (cmd === "download_runtime_bundles") {
        progressHandlers[0]({
          event: "runtime-download-progress",
          id: 1,
          payload: {
            kind: "runtime",
            phase: "installed",
            downloadedBytes: 0,
            totalBytes: null,
            bytesPerSecond: null,
            etaSeconds: null,
          },
        });
        return {
          installed: [
            {
              installedKind: "runtime",
              installedVersion: "2026.05.08-runtime1",
              platform: "darwin-aarch64",
              installDir: "/tmp/runtime",
              executablePath: "/tmp/runtime/mash-cv/mash-cv",
            },
          ],
        };
      }
      return null;
    });
    const user = userEvent.setup();

    renderWithTheme(<RuntimeBundleButton />);

    await user.click(await screen.findByRole("button", { name: "重新下载" }));

    expect(
      await screen.findByText("重新下载完成：2026.05.08-runtime1")
    ).toBeInTheDocument();
    expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
    expect(screen.queryByText("CV 运行时 安装完成")).not.toBeInTheDocument();
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
      await screen.findByText("正在下载 CV 运行时：5.0 MB / 20.0 MB · 2.5 MB/s · 预计剩余 6秒")
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
});
