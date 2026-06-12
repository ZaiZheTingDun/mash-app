import { act, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App";
import { renderWithTheme } from "../test/renderWithTheme";
import { createInitialProjectSlots } from "../components/projectSlots";
import type { Project } from "../types/project";
import type { SelfCheckStatus } from "../types/selfCheck";

const projects: Project[] = [
  {
    id: "project-1",
    name: "第一套",
    slots: createInitialProjectSlots(),
  },
  {
    id: "project-2",
    name: "第二套",
    slots: createInitialProjectSlots(),
  },
];

const selfCheckStatus: SelfCheckStatus = {
  appVersion: "0.5.4",
  cvRuntimeVersion: "2026.05.08-runtime1",
  cvRuntimeInstalled: true,
  cvCodeVersion: "v0.3.5",
  cvCodeInstalled: true,
  assetVersion: 2,
  appAssetsVersion: 2,
  servants: { entries: 3, hasImage: true, hasJson: true },
  ces: { entries: 5, hasImage: true, hasJson: false },
};

function installAppMock(
  savedActiveProjectId: string | null,
  options: { selfCheckError?: string } = {},
) {
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case "get_runtime_status":
      case "get_asset_bundle_status":
        return { installed: true };
      case "get_self_check_status":
        if (options.selfCheckError) {
          throw new Error(options.selfCheckError);
        }
        return selfCheckStatus;
      case "get_servants":
      case "get_craft_essences":
        return [];
      case "list_projects":
        return projects;
      case "get_active_project_id":
        return savedActiveProjectId;
      case "check_adb":
        return { connected: false, deviceName: null };
      case "get_server":
        return "JP";
      case "should_check_updates_today":
        return false;
      default:
        return null;
    }
  });
}

describe("App active project restore", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(listen).mockReset();
    vi.mocked(listen).mockResolvedValue(() => {});
  });

  it("opens the last selected project on startup", async () => {
    installAppMock("project-2");

    renderWithTheme(
      <App theme="light" themePreference="light" onThemeChange={vi.fn()} />
    );

    expect(await screen.findByText("～ 第二套 ～")).toBeInTheDocument();
  });

  it("falls back to the first project when the saved project no longer exists", async () => {
    installAppMock("missing-project");

    renderWithTheme(
      <App theme="light" themePreference="light" onThemeChange={vi.fn()} />
    );

    expect(await screen.findByText("～ 第一套 ～")).toBeInTheDocument();
  });

  it("opens the self-check dialog from the menu event", async () => {
    let selfCheckHandler: (() => void) | null = null;
    vi.mocked(listen).mockImplementation(async (event, handler) => {
      if (event === "self-check-requested") {
        selfCheckHandler = () =>
          handler({
            event: "self-check-requested",
            id: 0,
            payload: null,
          } as Parameters<typeof handler>[0]);
      }
      return () => {};
    });
    installAppMock("project-1");

    renderWithTheme(
      <App theme="light" themePreference="light" onThemeChange={vi.fn()} />
    );

    expect(await screen.findByText("～ 第一套 ～")).toBeInTheDocument();
    await act(async () => {
      selfCheckHandler?.();
    });

    expect(invoke).toHaveBeenCalledWith("get_self_check_status");
    expect(await screen.findByText("自检结果")).toBeInTheDocument();
    expect(screen.getByText("0.5.4")).toBeInTheDocument();
    expect(screen.getByText("2026.05.08-runtime1")).toBeInTheDocument();
    expect(screen.getByText("v2 / 目标 v2")).toBeInTheDocument();
    expect(screen.getByText("3 个")).toBeInTheDocument();
    expect(screen.getByText("5 个")).toBeInTheDocument();
  });

  it("shows self-check failures in the dialog", async () => {
    let selfCheckHandler: (() => void) | null = null;
    vi.mocked(listen).mockImplementation(async (event, handler) => {
      if (event === "self-check-requested") {
        selfCheckHandler = () =>
          handler({
            event: "self-check-requested",
            id: 0,
            payload: null,
          } as Parameters<typeof handler>[0]);
      }
      return () => {};
    });
    installAppMock("project-1", { selfCheckError: "boom" });

    renderWithTheme(
      <App theme="light" themePreference="light" onThemeChange={vi.fn()} />
    );

    expect(await screen.findByText("～ 第一套 ～")).toBeInTheDocument();
    await act(async () => {
      selfCheckHandler?.();
    });

    expect(await screen.findByText(/自检失败：Error: boom/)).toBeInTheDocument();
  });
});
