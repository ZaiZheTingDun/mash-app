import { screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App";
import { renderWithTheme } from "../test/renderWithTheme";
import { createInitialProjectSlots } from "../components/projectSlots";
import type { Project } from "../types/project";

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

function installAppMock(savedActiveProjectId: string | null) {
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
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
});
