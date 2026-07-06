import { useState } from "react";
import { describe, expect, it, vi, beforeEach } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { SettingsDialog, type SettingsSection } from "../SettingsPage";
import type { SelfCheckStatus } from "../../../types/selfCheck";
import type { Project } from "../../../types/project";

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

const importedProject: Project = {
  id: "imported-1",
  name: "导入配置",
  advancedMode: false,
  slots: [],
};

function SettingsHarness({
  initialSection = "selfCheck",
  onProjectsImported = vi.fn(),
}: {
  initialSection?: SettingsSection;
  onProjectsImported?: (projects: Project[]) => void;
}) {
  const [section, setSection] = useState<SettingsSection>(initialSection);
  return (
    <SettingsDialog
      open
      section={section}
      onOpenChange={vi.fn()}
      onSectionChange={setSection}
      onProjectsImported={onProjectsImported}
    />
  );
}

function argValue(args: unknown) {
  return typeof args === "object" && args != null && "value" in args
    ? (args as { value?: unknown }).value
    : undefined;
}

describe("SettingsDialog", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "get_self_check_status") return selfCheckStatus;
      if (cmd === "get_runtime_status") return { installed: true };
      if (cmd === "get_asset_bundle_status") return { installed: true };
      if (cmd === "get_recognition_settings") {
        return {
          noblePhantasmDetectionMode: "card",
          supportCeThreshold: 0.7,
          supportCeFullGateThreshold: 0.6,
          supportMlbIconThreshold: 0.7,
          supportBondIconThreshold: 0.7,
          stopOnBondLevelUp: false,
          stopOnBondMaxLevel: false,
          verifySkillActivation: false,
        };
      }
      if (cmd === "get_debug_settings") {
        return {
          autoCaptureBattleResultLoot: false,
          autoCaptureUnknownScreenTimeout: false,
          autoCaptureSkillUseProbe: false,
        };
      }
      if (cmd === "set_noble_phantasm_detection_mode") {
        return {
          noblePhantasmDetectionMode: "gauge",
          supportCeThreshold: 0.7,
          supportCeFullGateThreshold: 0.6,
          supportMlbIconThreshold: 0.7,
          supportBondIconThreshold: 0.7,
          stopOnBondLevelUp: false,
          stopOnBondMaxLevel: false,
          verifySkillActivation: false,
        };
      }
      if (cmd === "set_support_ce_threshold") {
        return {
          noblePhantasmDetectionMode: "card",
          supportCeThreshold: 0.65,
          supportCeFullGateThreshold: 0.6,
          supportMlbIconThreshold: 0.7,
          supportBondIconThreshold: 0.7,
          stopOnBondLevelUp: false,
          stopOnBondMaxLevel: false,
          verifySkillActivation: false,
        };
      }
      if (cmd === "set_support_ce_full_gate_threshold") {
        return {
          noblePhantasmDetectionMode: "card",
          supportCeThreshold: 0.7,
          supportCeFullGateThreshold: 0.55,
          supportMlbIconThreshold: 0.7,
          supportBondIconThreshold: 0.7,
          stopOnBondLevelUp: false,
          stopOnBondMaxLevel: false,
          verifySkillActivation: false,
        };
      }
      if (cmd === "set_support_mlb_icon_threshold") {
        return {
          noblePhantasmDetectionMode: "card",
          supportCeThreshold: 0.7,
          supportCeFullGateThreshold: 0.6,
          supportMlbIconThreshold: 0.76,
          supportBondIconThreshold: 0.7,
          stopOnBondLevelUp: false,
          stopOnBondMaxLevel: false,
          verifySkillActivation: false,
        };
      }
      if (cmd === "set_support_bond_icon_threshold") {
        return {
          noblePhantasmDetectionMode: "card",
          supportCeThreshold: 0.7,
          supportCeFullGateThreshold: 0.6,
          supportMlbIconThreshold: 0.7,
          supportBondIconThreshold: 0.78,
          stopOnBondLevelUp: false,
          stopOnBondMaxLevel: false,
          verifySkillActivation: false,
        };
      }
      if (cmd === "set_stop_on_bond_level_up") {
        return {
          noblePhantasmDetectionMode: "card",
          supportCeThreshold: 0.7,
          supportCeFullGateThreshold: 0.6,
          supportMlbIconThreshold: 0.7,
          supportBondIconThreshold: 0.7,
          stopOnBondLevelUp: Boolean(argValue(args)),
          stopOnBondMaxLevel: false,
          verifySkillActivation: false,
        };
      }
      if (cmd === "set_stop_on_bond_max_level") {
        return {
          noblePhantasmDetectionMode: "card",
          supportCeThreshold: 0.7,
          supportCeFullGateThreshold: 0.6,
          supportMlbIconThreshold: 0.7,
          supportBondIconThreshold: 0.7,
          stopOnBondLevelUp: false,
          stopOnBondMaxLevel: Boolean(argValue(args)),
          verifySkillActivation: false,
        };
      }
      if (cmd === "set_verify_skill_activation") {
        return {
          noblePhantasmDetectionMode: "card",
          supportCeThreshold: 0.7,
          supportCeFullGateThreshold: 0.6,
          supportMlbIconThreshold: 0.7,
          supportBondIconThreshold: 0.7,
          stopOnBondLevelUp: false,
          stopOnBondMaxLevel: false,
          verifySkillActivation: Boolean(argValue(args)),
        };
      }
      if (cmd === "set_auto_capture_battle_result_loot") {
        return {
          autoCaptureBattleResultLoot: Boolean(argValue(args)),
          autoCaptureUnknownScreenTimeout: false,
          autoCaptureSkillUseProbe: false,
        };
      }
      if (cmd === "set_auto_capture_unknown_screen_timeout") {
        return {
          autoCaptureBattleResultLoot: false,
          autoCaptureUnknownScreenTimeout: Boolean(argValue(args)),
          autoCaptureSkillUseProbe: false,
        };
      }
      if (cmd === "set_auto_capture_skill_use_probe") {
        return {
          autoCaptureBattleResultLoot: false,
          autoCaptureUnknownScreenTimeout: false,
          autoCaptureSkillUseProbe: Boolean(argValue(args)),
        };
      }
      return null;
    });
  });

  it("runs self-check on entry and can run it again", async () => {
    const user = userEvent.setup();
    renderWithTheme(<SettingsHarness />);

    expect(await screen.findByText("0.5.4")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("get_self_check_status");

    await user.click(screen.getByRole("button", { name: "重新自检" }));

    await waitFor(() => {
      expect(vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "get_self_check_status"))
        .toHaveLength(2);
    });
  });

  it("switches to resource management from the left navigation", async () => {
    const user = userEvent.setup();
    renderWithTheme(<SettingsHarness />);

    await user.click(screen.getByRole("button", { name: "资源管理" }));

    expect(await screen.findByText("CV 运行时")).toBeInTheDocument();
    expect(screen.getByText("素材包")).toBeInTheDocument();
  });

  it("groups settings navigation and switches to data management", async () => {
    const user = userEvent.setup();
    renderWithTheme(<SettingsHarness />);

    expect(screen.getByText("游戏")).toBeInTheDocument();
    expect(screen.getByText("应用")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "基础设置" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "阈值设置" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "调试" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "队伍管理" }));

    expect(await screen.findByRole("button", { name: "导入队伍" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "导出队伍" })).toBeInTheDocument();
  });

  it("loads and saves debug settings from the debug page", async () => {
    const user = userEvent.setup();
    renderWithTheme(<SettingsHarness initialSection="debug" />);

    expect(await screen.findByText("自动截图战利品页面")).toBeInTheDocument();
    expect(screen.getByText("无法识别画面超时时截图")).toBeInTheDocument();
    expect(screen.getByText("保存技能确认 probe 截图")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("get_debug_settings");

    await user.click(screen.getByRole("switch", { name: "自动截图战利品页面" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_auto_capture_battle_result_loot", {
        value: true,
      });
    });

    await user.click(screen.getByRole("switch", { name: "无法识别画面超时时截图" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_auto_capture_unknown_screen_timeout", {
        value: true,
      });
    });

    await user.click(screen.getByRole("switch", { name: "保存技能确认 probe 截图" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_auto_capture_skill_use_probe", {
        value: true,
      });
    });
    expect(await screen.findByText("已保存")).toBeInTheDocument();
  });

  it("loads and saves the support CE recognition threshold", async () => {
    const user = userEvent.setup();
    renderWithTheme(<SettingsHarness initialSection="recognition" />);

    expect(
      await screen.findByText("此处为全局设置；若只想针对某个队伍进行设置，请前往队伍设置页面。")
    ).toBeInTheDocument();

    const input = await screen.findByRole("spinbutton", {
      name: "助战礼装匹配阈值数值",
    });
    expect(input).toHaveValue(0.7);

    await user.clear(input);
    await user.type(input, "0.65");
    await user.click(screen.getAllByRole("button", { name: "保存" })[0]);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_support_ce_threshold", {
        value: 0.65,
      });
    });
    expect(await screen.findByText("已保存")).toBeInTheDocument();
  });

  it("keeps NP-card detection as the default and can opt into gauge detection", async () => {
    const user = userEvent.setup();
    renderWithTheme(<SettingsHarness initialSection="basic" />);

    const modeSelect = await screen.findByRole("combobox", { name: "宝具识别方式" });
    expect(modeSelect).toHaveTextContent("宝具指令卡识别");
    expect(screen.getByText("出现宝具识别问题可尝试切换，仍在实验中可能导致选卡速度变慢"))
      .toBeInTheDocument();

    await user.click(modeSelect);
    await user.click(await screen.findByText("底部宝具条识别（实验性）"));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_noble_phantasm_detection_mode", {
        value: "gauge",
      });
    });
  });

  it("loads bond auto-stop switches disabled by default", async () => {
    renderWithTheme(<SettingsHarness initialSection="basic" />);

    expect(await screen.findByRole("switch", { name: "牵绊升级自动停止" })).not.toBeChecked();
    expect(screen.getByRole("switch", { name: "牵绊满级自动停止" })).not.toBeChecked();
  });

  it("enabling bond max auto-stop disables bond level-up auto-stop with a tooltip", async () => {
    const user = userEvent.setup();
    renderWithTheme(<SettingsHarness initialSection="basic" />);

    await screen.findByRole("switch", { name: "牵绊升级自动停止" });
    const maxSwitch = screen.getByRole("switch", { name: "牵绊满级自动停止" });

    await user.click(maxSwitch);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_stop_on_bond_max_level", {
        value: true,
      });
    });
    const levelUpSwitch = screen.getByRole("switch", { name: "牵绊升级自动停止" });
    expect(levelUpSwitch).not.toBeChecked();
    expect(levelUpSwitch).toBeDisabled();
    expect(screen.getByRole("switch", { name: "牵绊满级自动停止" })).toBeChecked();

    await user.hover(levelUpSwitch.parentElement ?? levelUpSwitch);

    expect(
      await screen.findAllByText("牵绊满级自动停止已开启；关闭满级开关后可修改此项")
    ).not.toHaveLength(0);
  });

  it("saves bond level-up auto-stop without changing bond max auto-stop", async () => {
    const user = userEvent.setup();
    renderWithTheme(<SettingsHarness initialSection="basic" />);

    const levelUpSwitch = await screen.findByRole("switch", { name: "牵绊升级自动停止" });
    const maxSwitch = screen.getByRole("switch", { name: "牵绊满级自动停止" });
    expect(levelUpSwitch).not.toBeChecked();
    expect(maxSwitch).not.toBeChecked();

    await user.click(levelUpSwitch);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_stop_on_bond_level_up", {
        value: true,
      });
    });
    expect(screen.getByRole("switch", { name: "牵绊升级自动停止" })).toBeChecked();
    expect(screen.getByRole("switch", { name: "牵绊满级自动停止" })).not.toBeChecked();
  });

  it("saves support CE icon recognition thresholds", async () => {
    const user = userEvent.setup();
    renderWithTheme(<SettingsHarness initialSection="recognition" />);

    const mlbInput = await screen.findByRole("spinbutton", {
      name: "满破图标匹配阈值数值",
    });
    await user.clear(mlbInput);
    await user.type(mlbInput, "0.76");
    await user.click(screen.getAllByRole("button", { name: "保存" })[2]);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_support_mlb_icon_threshold", {
        value: 0.76,
      });
    });

    const bondInput = screen.getByRole("spinbutton", {
      name: "牵绊图标匹配阈值数值",
    });
    await user.clear(bondInput);
    await user.type(bondInput, "0.78");
    await user.click(screen.getAllByRole("button", { name: "保存" })[3]);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_support_bond_icon_threshold", {
        value: 0.78,
      });
    });
  });

  it("saves the support CE full-match gate threshold", async () => {
    const user = userEvent.setup();
    renderWithTheme(<SettingsHarness initialSection="recognition" />);

    const input = await screen.findByRole("spinbutton", {
      name: "完整匹配兜底阈值数值",
    });
    expect(input).toHaveValue(0.6);

    await user.clear(input);
    await user.type(input, "0.55");
    await user.click(screen.getAllByRole("button", { name: "保存" })[1]);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_support_ce_full_gate_threshold", {
        value: 0.55,
      });
    });
  });

  it("saves the global skill activation verification toggle", async () => {
    const user = userEvent.setup();
    renderWithTheme(<SettingsHarness initialSection="basic" />);

    const toggle = await screen.findByRole("switch", { name: "技能使用确认" });
    expect(toggle).not.toBeChecked();

    await user.click(toggle);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_verify_skill_activation", {
        value: true,
      });
    });
    expect(screen.getByRole("switch", { name: "技能使用确认" })).toBeChecked();
  });

  it("shows export configs and disables export after deselecting all", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "list_exportable_configs") {
        return [
          {
            id: "project-1",
            name: "第一套",
            advancedMode: false,
            battleSceneCount: 1,
            advancedBattleSceneCount: 0,
          },
          {
            id: "project-2",
            name: "第二套",
            advancedMode: true,
            battleSceneCount: 0,
            advancedBattleSceneCount: 2,
          },
        ];
      }
      return null;
    });
    renderWithTheme(<SettingsHarness initialSection="dataManagement" />);

    await user.click(screen.getByRole("button", { name: "导出队伍" }));

    expect(await screen.findByText("第一套")).toBeInTheDocument();
    expect(screen.getByText("第二套")).toBeInTheDocument();
    expect(screen.queryByText(/普通指令/)).not.toBeInTheDocument();
    expect(screen.queryByText(/高级指令/)).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "导出" })).toBeEnabled();

    await user.click(screen.getByRole("checkbox", { name: "全选" }));

    expect(screen.getByRole("button", { name: "导出" })).toBeDisabled();
  });

  it("previews import results and imports only selected configs", async () => {
    const user = userEvent.setup();
    const onProjectsImported = vi.fn();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "pick_config_import_file") return "/tmp/config.mashconfig.json";
      if (cmd === "preview_config_import") {
        return {
          fileName: "config.mashconfig.json",
          validConfigs: [
            {
              importKey: "0",
              sourceName: "第一套",
              targetName: "第一套（导入）",
              advancedMode: false,
              battleSceneCount: 1,
              advancedBattleSceneCount: 0,
            },
            {
              importKey: "1",
              sourceName: "第二套",
              targetName: "第二套（导入）",
              advancedMode: true,
              battleSceneCount: 0,
              advancedBattleSceneCount: 2,
            },
          ],
          invalidItems: [{ label: "第 2 项", reason: "配置结构无法识别" }],
        };
      }
      if (cmd === "import_configurations") {
        return { importedProjects: [importedProject] };
      }
      return null;
    });
    renderWithTheme(
      <SettingsHarness
        initialSection="dataManagement"
        onProjectsImported={onProjectsImported}
      />,
    );

    await user.click(screen.getByRole("button", { name: "导入队伍" }));

    expect(await screen.findByText("第一套 → 第一套（导入）")).toBeInTheDocument();
    expect(screen.getByText("第二套 → 第二套（导入）")).toBeInTheDocument();
    expect(screen.queryByText(/普通指令/)).not.toBeInTheDocument();
    expect(screen.queryByText(/高级指令/)).not.toBeInTheDocument();
    expect(screen.getByText("配置结构无法识别")).toBeInTheDocument();

    await user.click(screen.getByRole("checkbox", { name: /第二套/ }));
    await user.click(screen.getByRole("button", { name: "导入" }));

    await waitFor(() => {
      expect(onProjectsImported).toHaveBeenCalledWith([importedProject]);
    });
    expect(screen.queryByRole("button", { name: "导入" })).not.toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("import_configurations", {
      filePath: "/tmp/config.mashconfig.json",
      importKeys: ["0"],
    });
  });

  it("disables import when preview has no valid configs", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "pick_config_import_file") return "/tmp/config.mashconfig.json";
      if (cmd === "preview_config_import") {
        return {
          fileName: "config.mashconfig.json",
          validConfigs: [],
          invalidItems: [{ label: "第 1 项", reason: "配置结构无法识别" }],
        };
      }
      return null;
    });
    renderWithTheme(<SettingsHarness initialSection="dataManagement" />);

    await user.click(screen.getByRole("button", { name: "导入队伍" }));

    expect(await screen.findByText("没有可导入的配置。")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "导入" })).toBeDisabled();
  });
});
