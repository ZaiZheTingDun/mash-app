import { useState } from "react";
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { SettingsDialog, type SettingsSection } from "../SettingsPage";
import type { AssetBundleImportResult, AssetBundleStatus } from "../../../types/assets";
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

function resourceAssetStatus(version: number): AssetBundleStatus {
  const updateAvailable = version < 2;
  return {
    installed: !updateAvailable,
    importedServants: true,
    importedCraftEssences: true,
    servantFiles: 12,
    craftEssenceFiles: 8,
    installDir: "/tmp/mash-assets",
    currentVersion: version,
    appAssetsVersion: 2,
    remoteLatestVersion: null,
    remoteLatestBaseVersion: null,
    targetVersion: updateAvailable ? 2 : null,
    updateAvailable,
    updateDownloadSize: 0,
    updatePlan: updateAvailable ? "pending" : "none",
    latestUrl: "https://mash.xiaotongx.com/mash/assets/latest.json",
    remoteManifestUrl: null,
    updateCheckError: null,
  };
}

describe("SettingsDialog", () => {
  afterEach(() => {
    vi.unstubAllEnvs();
  });

  beforeEach(() => {
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "get_self_check_status") return selfCheckStatus;
      if (cmd === "get_runtime_status") return { installed: true };
      if (cmd === "get_asset_bundle_status") return { installed: true };
      if (cmd === "get_battle_start_panel") return "operationLog";
      if (cmd === "get_recognition_settings") {
        return {
          noblePhantasmDetectionMode: "card",
          supportCeThreshold: 0.7,
          supportCeFullGateThreshold: 0.6,
          supportMlbIconThreshold: 0.7,
          supportBondIconThreshold: 0.7,
          stopOnBondLevelUp: false,
          stopOnBondMaxLevel: false,
          autoCaptureBondLevelUp: false,
          verifySkillActivation: false,
          unknownScreenTimeoutCount: 100,
        };
      }
      if (cmd === "get_debug_settings") {
        return {
          autoCaptureBattleResultLoot: false,
          autoCaptureUnknownScreenTimeout: false,
          autoCaptureSkillUseProbe: false,
          autoCaptureUnrecognizedCriticalChance: false,
          simulateStuckAttackSelection: false,
        };
      }
      if (cmd === "set_noble_phantasm_detection_mode") {
        return {
          noblePhantasmDetectionMode:
            argValue(args) === "gaugeBeforeAttack" ? "gaugeBeforeAttack" : "gauge",
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
      if (cmd === "set_auto_capture_bond_level_up") {
        return {
          autoCaptureBondLevelUp: Boolean(argValue(args)),
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
      if (cmd === "set_enable_extra_class_filter") {
        return {
          enableExtraClassFilter: Boolean(argValue(args)),
        };
      }
      if (cmd === "set_unknown_screen_timeout_count") {
        return {
          unknownScreenTimeoutCount: Number(argValue(args)),
        };
      }
      if (cmd === "set_auto_capture_battle_result_loot") {
        return {
          autoCaptureBattleResultLoot: Boolean(argValue(args)),
          autoCaptureUnknownScreenTimeout: false,
          autoCaptureSkillUseProbe: false,
          autoCaptureUnrecognizedCriticalChance: false,
          simulateStuckAttackSelection: false,
        };
      }
      if (cmd === "set_auto_capture_unknown_screen_timeout") {
        return {
          autoCaptureBattleResultLoot: false,
          autoCaptureUnknownScreenTimeout: Boolean(argValue(args)),
          autoCaptureSkillUseProbe: false,
          autoCaptureUnrecognizedCriticalChance: false,
          simulateStuckAttackSelection: false,
        };
      }
      if (cmd === "set_auto_capture_skill_use_probe") {
        return {
          autoCaptureBattleResultLoot: false,
          autoCaptureUnknownScreenTimeout: false,
          autoCaptureSkillUseProbe: Boolean(argValue(args)),
          autoCaptureUnrecognizedCriticalChance: false,
          simulateStuckAttackSelection: false,
        };
      }
      if (cmd === "set_auto_capture_unrecognized_critical_chance") {
        return {
          autoCaptureBattleResultLoot: false,
          autoCaptureUnknownScreenTimeout: false,
          autoCaptureSkillUseProbe: false,
          autoCaptureUnrecognizedCriticalChance: Boolean(argValue(args)),
          simulateStuckAttackSelection: false,
        };
      }
      if (cmd === "set_simulate_stuck_attack_selection") {
        return {
          autoCaptureBattleResultLoot: false,
          autoCaptureUnknownScreenTimeout: false,
          autoCaptureSkillUseProbe: false,
          autoCaptureUnrecognizedCriticalChance: false,
          simulateStuckAttackSelection: Boolean(argValue(args)),
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

  it("blocks leaving resource management until an imported old bundle is updated", async () => {
    const onOpenChange = vi.fn();
    const onSectionChange = vi.fn();
    let installedVersion = 2;
    let finishImport!: () => void;
    const importPromise = new Promise<AssetBundleImportResult>((resolve) => {
      finishImport = () => {
        installedVersion = 1;
        resolve({
          importedServants: true,
          importedCraftEssences: true,
          servantFiles: 12,
          craftEssenceFiles: 8,
          installDir: "/tmp/mash-assets",
        });
      };
    });
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_runtime_status") return { installed: true };
      if (cmd === "get_asset_bundle_status") return resourceAssetStatus(installedVersion);
      if (cmd === "pick_asset_bundle") return "/tmp/mash-assets-v1.zip";
      if (cmd === "import_asset_bundle") return importPromise;
      if (cmd === "download_asset_bundles") {
        installedVersion = 2;
        return {
          installed: true,
          installedVersion: 2,
          plan: "patch",
          servantFiles: 12,
          craftEssenceFiles: 8,
          installDir: "/tmp/mash-assets",
        };
      }
      return null;
    });
    const user = userEvent.setup();

    renderWithTheme(
      <SettingsDialog
        open
        section="resources"
        onOpenChange={onOpenChange}
        onSectionChange={onSectionChange}
      />
    );

    const close = await screen.findByRole("button", { name: "关闭设置" });
    await waitFor(() => expect(close).toBeEnabled());
    await user.click(screen.getByRole("button", { name: "从本地导入" }));

    expect(await screen.findByRole("button", { name: "导入中…" })).toBeInTheDocument();
    expect(close).toBeDisabled();
    await user.keyboard("{Escape}");
    expect(onOpenChange).not.toHaveBeenCalled();

    finishImport();
    expect(await screen.findByText("需要更新素材包 v1 → v2")).toBeInTheDocument();
    expect(close).toBeDisabled();
    expect(screen.getByRole("button", { name: "基础设置" })).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "在线更新" }));
    await waitFor(() => expect(close).toBeEnabled());
    await user.click(close);

    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(onSectionChange).not.toHaveBeenCalled();
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

  it("hides debug settings outside local development", async () => {
    vi.stubEnv("DEV", false);
    vi.resetModules();
    const { SettingsDialog: ProductionSettingsDialog } = await import("../SettingsPage");

    renderWithTheme(
      <ProductionSettingsDialog
        open
        section="debug"
        onOpenChange={vi.fn()}
        onSectionChange={vi.fn()}
      />
    );

    expect(screen.queryByRole("button", { name: "调试" })).not.toBeInTheDocument();
    expect(screen.queryByText("自动截图战利品页面")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "基础设置" })).toBeInTheDocument();
    expect(await screen.findByText("宝具识别方式")).toBeInTheDocument();
  });

  it("loads and saves debug settings from the debug page", async () => {
    const user = userEvent.setup();
    renderWithTheme(<SettingsHarness initialSection="debug" />);

    expect(await screen.findByText("自动截图战利品页面")).toBeInTheDocument();
    expect(screen.getByText("无法识别画面超时时截图")).toBeInTheDocument();
    expect(screen.getByText("保存技能确认 probe 截图")).toBeInTheDocument();
    expect(screen.getByText("暴击率无法识别时截图")).toBeInTheDocument();
    expect(screen.getByText("测试选卡卡住恢复")).toBeInTheDocument();
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

    await user.click(screen.getByRole("switch", { name: "暴击率无法识别时截图" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_auto_capture_unrecognized_critical_chance", {
        value: true,
      });
    });

    await user.click(screen.getByRole("switch", { name: "测试选卡卡住恢复" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_simulate_stuck_attack_selection", {
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
    expect(modeSelect).toHaveTextContent("指令卡识别");
    expect(screen.getByText("出现宝具识别问题可尝试切换，仍在实验中可能导致选卡速度变慢"))
      .toBeInTheDocument();

    await user.click(modeSelect);
    const options = await screen.findAllByRole("option");
    expect(options.map((option) => option.textContent?.trim())).toEqual([
      "指令卡识别",
      "宝具条识别（选卡前）",
      "宝具条识别（选卡时）",
    ]);

    await user.click(screen.getByText("宝具条识别（选卡时）"));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_noble_phantasm_detection_mode", {
        value: "gauge",
      });
    });
  });

  it("offers gauge detection before attack with a dialogue warning", async () => {
    const user = userEvent.setup();
    renderWithTheme(<SettingsHarness initialSection="basic" />);

    const modeSelect = await screen.findByRole("combobox", { name: "宝具识别方式" });
    await user.click(modeSelect);
    await user.click(await screen.findByText("宝具条识别（选卡前）"));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_noble_phantasm_detection_mode", {
        value: "gaugeBeforeAttack",
      });
    });
    expect(await screen.findByText("攻击前识别可能被从者台词遮挡，请关闭台词")).toBeInTheDocument();
    expect(
      await screen.findByRole("img", { name: "关闭战斗中宝具语音字幕的设置示例" })
    ).toBeInTheDocument();
  });

  it("loads bond auto-stop switches disabled by default", async () => {
    renderWithTheme(<SettingsHarness initialSection="basic" />);

    expect(await screen.findByRole("switch", { name: "牵绊升级自动停止" })).not.toBeChecked();
    expect(screen.getByRole("switch", { name: "牵绊满级自动停止" })).not.toBeChecked();
    expect(screen.getByRole("switch", { name: "牵绊升级时自动截图" })).not.toBeChecked();
  });

  it("saves automatic bond level-up screenshots", async () => {
    const user = userEvent.setup();
    renderWithTheme(<SettingsHarness initialSection="basic" />);

    const captureSwitch = await screen.findByRole("switch", { name: "牵绊升级时自动截图" });
    await user.click(captureSwitch);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_auto_capture_bond_level_up", { value: true });
    });
    expect(screen.getByRole("switch", { name: "牵绊升级时自动截图" })).toBeChecked();
  });

  it("opens the bond level-up screenshot folder", async () => {
    const user = userEvent.setup();
    renderWithTheme(<SettingsHarness initialSection="basic" />);

    await user.click(await screen.findByRole("button", { name: "打开截图文件夹" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("open_bond_level_up_screenshot_folder");
    });
    expect(await screen.findByText("已打开截图文件夹")).toBeInTheDocument();
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

  it("saves the global Extra class filter toggle", async () => {
    const user = userEvent.setup();
    renderWithTheme(<SettingsHarness initialSection="basic" />);

    const toggle = await screen.findByRole("switch", {
      name: "全局 Extra 职阶筛选",
    });
    expect(toggle).toBeChecked();

    await user.click(toggle);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_enable_extra_class_filter", {
        value: false,
      });
    });
    expect(
      screen.getByRole("switch", { name: "全局 Extra 职阶筛选" })
    ).not.toBeChecked();
  });

  it("auto-saves the shared timeout count and disables the limit with a toggle", async () => {
    const user = userEvent.setup();
    renderWithTheme(<SettingsHarness initialSection="basic" />);

    const timeoutInput = await screen.findByRole("spinbutton", {
      name: "识别超时次数",
    });
    expect(timeoutInput).toHaveValue(100);
    expect(timeoutInput).toHaveAttribute("min", "50");
    expect(timeoutInput).toHaveAttribute("max", "1000");
    expect(
      screen.queryByRole("button", { name: "保存识别超时" })
    ).not.toBeInTheDocument();

    await user.clear(timeoutInput);
    await user.type(timeoutInput, "120");

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_unknown_screen_timeout_count", {
        value: 120,
      });
    });

    const timeoutLimit = screen.getByRole("switch", {
      name: "识别超时限制",
    });
    expect(timeoutLimit).toBeChecked();

    await user.click(timeoutLimit);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_unknown_screen_timeout_count", {
        value: 9999,
      });
    });
    expect(timeoutInput).toBeDisabled();

    await user.click(timeoutLimit);

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("set_unknown_screen_timeout_count", {
        value: 120,
      });
    });
    expect(timeoutInput).toBeEnabled();
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
