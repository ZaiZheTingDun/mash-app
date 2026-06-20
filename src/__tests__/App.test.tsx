import { act, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App";
import { renderWithTheme } from "../test/renderWithTheme";
import { createInitialProjectSlots } from "../features/team/projectSlots";
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
    expect(await screen.findByText("3 骑")).toBeInTheDocument();
    expect(await screen.findByText("5 张")).toBeInTheDocument();
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

  it("opens resource management from the menu event and returns to the team page", async () => {
    let resourceHandler: (() => void) | null = null;
    vi.mocked(listen).mockImplementation(async (event, handler) => {
      if (event === "resource-manager-requested") {
        resourceHandler = () =>
          handler({
            event: "resource-manager-requested",
            id: 0,
            payload: null,
          } as Parameters<typeof handler>[0]);
      }
      return () => {};
    });
    installAppMock("project-1");
    const user = userEvent.setup();

    renderWithTheme(
      <App theme="light" themePreference="light" onThemeChange={vi.fn()} />
    );

    expect(await screen.findByText("～ 第一套 ～")).toBeInTheDocument();
    await act(async () => {
      resourceHandler?.();
    });

    expect(await screen.findByText("CV 运行时")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "关闭设置" }));

    expect(invoke).toHaveBeenCalledWith("cancel_resource_downloads");
    expect(await screen.findByText("～ 第一套 ～")).toBeInTheDocument();
  });

  it("opens the settings dialog from the status bar and switches sections", async () => {
    installAppMock("project-1");
    const user = userEvent.setup();

    renderWithTheme(
      <App theme="light" themePreference="light" onThemeChange={vi.fn()} />
    );

    expect(await screen.findByText("～ 第一套 ～")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "设置" }));

    expect(await screen.findByRole("button", { name: "导入队伍" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "导出队伍" })).toBeInTheDocument();
    expect(screen.getByText("～ 第一套 ～")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "资源管理" }));

    expect(await screen.findByText("CV 运行时")).toBeInTheDocument();
    expect(screen.getByText("CV 运行时")).toBeInTheDocument();
  });

  it("blocks command setup when custom card rules reference removed servants", async () => {
    const user = userEvent.setup();
    const slots = createInitialProjectSlots();
    slots[0] = { ...slots[0], servantId: 1 };
    const invalidProject: Project = {
      id: "project-invalid-rule",
      name: "失效规则",
      advancedMode: true,
      slots,
      grandCardStrategy: {
        customRules: [
          {
            id: "rule_1",
            name: "自定义规则",
            slots: [
              { servantId: 999, kind: "any", color: "buster" },
              { servantId: 1, kind: "any", color: "any" },
              { servantId: 1, kind: "np", color: "buster" },
            ],
          },
        ],
      },
    };
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case "get_runtime_status":
        case "get_asset_bundle_status":
          return { installed: true };
        case "get_servants":
          return [
            {
              id: 1,
              variantKey: "1",
              name_cn: "甲",
              name_jp: "甲",
              name_en: "A",
              class: "Saber",
              rarity: 5,
              noblePhantasmCard: "buster",
            },
          ];
        case "get_craft_essences":
          return [];
        case "list_projects":
          return [invalidProject];
        case "get_active_project_id":
          return invalidProject.id;
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

    renderWithTheme(
      <App theme="light" themePreference="light" onThemeChange={vi.fn()} />
    );

    expect(await screen.findByText("～ 失效规则 ～")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "指令设置" }));

    expect(await screen.findByText("需要修复出牌规则")).toBeInTheDocument();
    expect(screen.getByText(/自定义规则 第 1 张/)).toBeInTheDocument();
    expect(screen.queryByText("主力输出")).not.toBeInTheDocument();
  });

  it("saves an adb screenshot from the menu event", async () => {
    let screenshotHandler: (() => void) | null = null;
    vi.mocked(listen).mockImplementation(async (event, handler) => {
      if (event === "save-adb-screenshot-requested") {
        screenshotHandler = () =>
          handler({
            event: "save-adb-screenshot-requested",
            id: 0,
            payload: null,
          } as Parameters<typeof handler>[0]);
      }
      return () => {};
    });
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "save_adb_screenshot") return "/tmp/mash-screenshot.png";
      switch (cmd) {
        case "get_runtime_status":
        case "get_asset_bundle_status":
          return { installed: true };
        case "get_servants":
        case "get_craft_essences":
          return [];
        case "list_projects":
          return projects;
        case "get_active_project_id":
          return "project-1";
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

    renderWithTheme(
      <App theme="light" themePreference="light" onThemeChange={vi.fn()} />
    );

    expect(await screen.findByText("～ 第一套 ～")).toBeInTheDocument();
    await act(async () => {
      screenshotHandler?.();
    });

    expect(invoke).toHaveBeenCalledWith("save_adb_screenshot");
    expect(await screen.findByText("截图已保存: /tmp/mash-screenshot.png")).toBeInTheDocument();
  });
});
