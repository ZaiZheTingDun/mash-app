import { useState } from "react";
import { describe, expect, it, vi, beforeEach } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
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

function StatefulSettings({ initialProject, onSave }: {
  initialProject: Project;
  onSave: (project: Project) => void;
}) {
  const [project, setProject] = useState(initialProject);
  return <ProjectSettingsDialog
    open project={project} onOpenChange={vi.fn()}
    onUpdateProject={async (next) => { onSave(next); setProject(next); }}
  />;
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
          verifySkillActivation: false,
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
    await userEvent.setup().click(await screen.findByRole("button", { name: "阈值设置" }));
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

    await user.click(await screen.findByRole("button", { name: "阈值设置" }));
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

    await user.click(await screen.findByRole("button", { name: "阈值设置" }));
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

  it("saves and clears the project skill activation verification override", async () => {
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

    await user.click(await screen.findByRole("button", { name: "基础设置" }));

    const trigger = await screen.findByRole("combobox", {
      name: "队伍技能使用确认",
    });
    expect(trigger).toHaveTextContent("继承全局（关闭）");

    await user.click(trigger);
    await user.click(await screen.findByRole("option", { name: "开启" }));

    await waitFor(() => {
      expect(onUpdateProject).toHaveBeenCalledWith({
        ...project,
        recognitionSettings: {
          verifySkillActivation: true,
        },
      });
    });
  });

  it("saves the project auto skill target recognition override", async () => {
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

    const trigger = await screen.findByRole("combobox", {
      name: "关闭自动技能目标识别",
    });
    expect(trigger).toHaveTextContent("关闭");

    await user.click(trigger);
    await user.click(await screen.findByRole("option", { name: "开启" }));

    await waitFor(() => {
      expect(onUpdateProject).toHaveBeenCalledWith({
        ...project,
        disableAutoSkillTargetRecognition: true,
      });
    });
  });

  it("persists the higher critical chance command-card preference", async () => {
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

    const preference = await screen.findByRole("switch", {
      name: "优先选择暴击率更高的指令卡",
    });
    expect(preference).not.toBeChecked();

    await user.click(preference);

    await waitFor(() => {
      expect(onUpdateProject).toHaveBeenCalledWith({
        ...project,
        preferHigherCriticalChance: true,
      });
    });
  });

  it("enables Extra class filtering by default and persists the switch", async () => {
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

    const extraFilter = await screen.findByRole("switch", {
      name: "Extra 职阶筛选",
    });
    expect(extraFilter).toBeChecked();

    await user.click(extraFilter);

    await waitFor(() => {
      expect(onUpdateProject).toHaveBeenCalledWith({
        ...project,
        recognitionSettings: {
          enableExtraClassFilter: false,
        },
      });
    });
  });

  it("inherits the global Extra class filter setting before a project override", async () => {
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
          verifySkillActivation: false,
          enableExtraClassFilter: false,
          unknownScreenTimeoutCount: 100,
        };
      }
      return null;
    });

    renderWithTheme(
      <ProjectSettingsDialog
        open
        project={makeProject()}
        onOpenChange={vi.fn()}
        onUpdateProject={vi.fn()}
      />
    );

    expect(
      await screen.findByRole("switch", { name: "Extra 职阶筛选" })
    ).not.toBeChecked();
    expect(screen.getAllByText("全局当前：关闭")).not.toHaveLength(0);
  });
  it("places run settings between basic and threshold settings with legacy defaults", async () => {
    const user = userEvent.setup();
    renderWithTheme(<StatefulSettings initialProject={makeProject()} onSave={vi.fn()} />);
    const navigation = screen.getByRole("navigation", { name: "队伍设置导航" });
    expect(within(navigation).getAllByRole("button").map((button) => button.textContent)).toEqual([
      "基础设置", "运行设置", "阈值设置",
    ]);
    await user.click(within(navigation).getByRole("button", { name: "运行设置" }));
    expect(screen.getByRole("combobox", { name: "助战筛选设置" })).toHaveTextContent("只筛选对应职阶");
    expect(screen.getByRole("switch", { name: "五星礼装掉落自动停止" })).not.toBeChecked();
    expect(screen.queryByRole("spinbutton", { name: "五星礼装掉落个数" })).not.toBeInTheDocument();
  });

  it.each([
    ["allClasses", "只筛选全职阶"],
    ["both", "同时筛选"],
    ["correspondingClass", "只筛选对应职阶"],
  ] as const)("saves support mode %s while preserving existing team settings", async (mode, label) => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    const project = makeProject({ recognitionSettings: {
      supportClassFilterMode: "both", supportCeThreshold: 0.8,
      stopOnFiveStarCeDrop: true, fiveStarCeDropTargetCount: 3,
    } });
    renderWithTheme(<StatefulSettings initialProject={project} onSave={onSave} />);
    await user.click(screen.getByRole("button", { name: "运行设置" }));
    // Begin from a different option so every case exercises a mutation.
    await user.click(screen.getByRole("combobox", { name: "助战筛选设置" }));
    await user.click(await screen.findByRole("option", { name: mode === "both" ? "只筛选全职阶" : label }));
    if (mode === "both") {
      await user.click(screen.getByRole("combobox", { name: "助战筛选设置" }));
      await user.click(await screen.findByRole("option", { name: label }));
    }
    await waitFor(() => expect(onSave).toHaveBeenLastCalledWith({
      ...project, recognitionSettings: { ...project.recognitionSettings, supportClassFilterMode: mode },
    }));
    expect(screen.getByRole("combobox", { name: "助战筛选设置" })).toHaveTextContent(label);
    expect(screen.getByRole("spinbutton", { name: "五星礼装掉落个数" })).toHaveValue(3);
  });

  it("edits migrated drop settings and preserves the target when toggled off and back on", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    const project = makeProject({ recognitionSettings: {
      stopOnFiveStarCeDrop: true, fiveStarCeDropTargetCount: 3, supportCeThreshold: 0.8,
    } });
    renderWithTheme(<StatefulSettings initialProject={project} onSave={onSave} />);
    await user.click(screen.getByRole("button", { name: "运行设置" }));
    expect(screen.getByRole("spinbutton", { name: "五星礼装掉落个数" })).toHaveValue(3);
    await user.click(screen.getByRole("button", { name: "增加五星礼装掉落个数" }));
    expect(screen.getByRole("spinbutton", { name: "五星礼装掉落个数" })).toHaveValue(4);
    await user.click(screen.getByRole("button", { name: "减少五星礼装掉落个数" }));
    await user.click(screen.getByRole("switch", { name: "五星礼装掉落自动停止" }));
    expect(screen.queryByRole("spinbutton", { name: "五星礼装掉落个数" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("switch", { name: "五星礼装掉落自动停止" }));
    expect(screen.getByRole("spinbutton", { name: "五星礼装掉落个数" })).toHaveValue(3);
    expect(onSave).toHaveBeenLastCalledWith(project);
  });

  it("enables drop stop with a default target of one", async () => {
    const user = userEvent.setup();
    renderWithTheme(<StatefulSettings initialProject={makeProject()} onSave={vi.fn()} />);
    await user.click(screen.getByRole("button", { name: "运行设置" }));
    await user.click(screen.getByRole("switch", { name: "五星礼装掉落自动停止" }));
    expect(screen.getByRole("spinbutton", { name: "五星礼装掉落个数" })).toHaveValue(1);
    expect(screen.getByRole("button", { name: "减少五星礼装掉落个数" })).toBeDisabled();
  });

  it("keeps the saved mode and shows an error when saving fails", async () => {
    const user = userEvent.setup();
    renderWithTheme(<ProjectSettingsDialog open project={makeProject()} onOpenChange={vi.fn()}
      onUpdateProject={vi.fn().mockRejectedValue(new Error("保存失败"))} />);
    await user.click(screen.getByRole("button", { name: "运行设置" }));
    await user.click(screen.getByRole("combobox", { name: "助战筛选设置" }));
    await user.click(await screen.findByRole("option", { name: "同时筛选" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("保存失败");
    expect(screen.getByRole("combobox", { name: "助战筛选设置" })).toHaveTextContent("只筛选对应职阶");
  });

});
