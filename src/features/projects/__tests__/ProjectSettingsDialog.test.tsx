import { describe, expect, it, vi, beforeEach } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { createInitialProjectSlots } from "../../team/projectSlots";
import { ProjectSettingsDialog } from "../ProjectSettingsDialog";
import type { Project } from "../../../types/project";

function makeProject(overrides?: Partial<Project>): Project {
  return {
    id: "project-1",
    name: "项目甲",
    advancedMode: false,
    supportServantId: null,
    slots: createInitialProjectSlots(),
    repeatMission: false,
    ...overrides,
  };
}

describe("ProjectSettingsDialog", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_recognition_settings") {
        return {
          noblePhantasmDetectionMode: "card",
          supportCeThreshold: 0.71,
          supportCeFullGateThreshold: 0.61,
          supportMlbIconThreshold: 0.72,
          supportBondIconThreshold: 0.73,
          stopOnBondLevelUp: false,
          stopOnBondMaxLevel: false,
        };
      }
      return null;
    });
  });

  it("loads global thresholds when the project has no override", async () => {
    renderWithTheme(
      <ProjectSettingsDialog
        open
        project={makeProject()}
        onOpenChange={vi.fn()}
        onUpdateProject={vi.fn()}
      />
    );

    expect(await screen.findByText("游戏")).toBeInTheDocument();
    expect(await screen.findByText("项目甲")).toBeInTheDocument();
    expect(screen.queryByText("当前队伍：项目甲")).not.toBeInTheDocument();
    expect(screen.getByText("低数值更容易命中，高数值更不容易误选；默认 0.71")).toBeInTheDocument();
    expect(
      screen.getByRole("spinbutton", { name: "助战礼装匹配阈值数值" })
    ).toHaveValue(0.71);
  });

  it("saves a project recognition threshold through update_project state", async () => {
    const user = userEvent.setup();
    const onUpdateProject = vi.fn().mockResolvedValue(undefined);
    const project = makeProject();
    renderWithTheme(
      <ProjectSettingsDialog
        open
        project={project}
        onOpenChange={vi.fn()}
        onUpdateProject={onUpdateProject}
      />
    );

    const input = await screen.findByRole("spinbutton", {
      name: "助战礼装匹配阈值数值",
    });
    await user.clear(input);
    await user.type(input, "0.66");
    await user.click(screen.getAllByRole("button", { name: "保存" })[0]);

    await waitFor(() => {
      expect(onUpdateProject).toHaveBeenCalledWith({
        ...project,
        recognitionSettings: {
          supportCeThreshold: 0.66,
        },
      });
    });
    expect(await screen.findByText("已保存")).toBeInTheDocument();
  });

  it("restores a project threshold by deleting its override", async () => {
    const user = userEvent.setup();
    const onUpdateProject = vi.fn().mockResolvedValue(undefined);
    const project = makeProject({
      recognitionSettings: {
        supportCeThreshold: 0.66,
        supportMlbIconThreshold: 0.76,
      },
    });
    renderWithTheme(
      <ProjectSettingsDialog
        open
        project={project}
        onOpenChange={vi.fn()}
        onUpdateProject={onUpdateProject}
      />
    );

    expect(
      await screen.findByRole("spinbutton", { name: "助战礼装匹配阈值数值" })
    ).toHaveValue(0.66);

    await user.click(screen.getAllByRole("button", { name: "恢复默认" })[0]);

    await waitFor(() => {
      expect(onUpdateProject).toHaveBeenCalledWith({
        ...project,
        recognitionSettings: {
          supportMlbIconThreshold: 0.76,
        },
      });
    });
    expect(screen.getByRole("spinbutton", { name: "助战礼装匹配阈值数值" })).toHaveValue(0.71);
  });
});
