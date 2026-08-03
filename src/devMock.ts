import { createInitialProjectSlots } from "./features/team/projectSlots";
import type { AdvancedBattleScene, BattleScene } from "./types/command";
import type { CraftEssence } from "./types/craftEssence";
import type { Project } from "./types/project";
import type { Servant } from "./types/servant";
import type { Server } from "./types/server";

type InvokeArgs = Record<string, unknown>;

const servants: Servant[] = [
  {
    id: 1,
    variantKey: "1:1",
    faceId: 1,
    name_cn: "玛修·基列莱特",
    name_jp: "マシュ・キリエライト",
    name_en: "Mash Kyrielight",
    class: "Shielder",
    rarity: 4,
    noblePhantasmName: "已然遥远的理想之城",
  },
  {
    id: 2,
    variantKey: "2:1",
    faceId: 1,
    name_cn: "阿尔托莉雅·潘德拉贡",
    name_jp: "アルトリア・ペンドラゴン",
    name_en: "Altria Pendragon",
    class: "Saber",
    rarity: 5,
    noblePhantasmName: "誓约胜利之剑",
  },
  {
    id: 3,
    variantKey: "3:1",
    faceId: 1,
    name_cn: "诸葛孔明〔埃尔梅罗二世〕",
    name_jp: "諸葛孔明〔エルメロイII世〕",
    name_en: "Zhuge Liang",
    class: "Caster",
    rarity: 5,
    noblePhantasmName: "石兵八阵",
  },
  {
    id: 4,
    variantKey: "4:1",
    faceId: 1,
    name_cn: "斯卡哈·斯卡蒂",
    name_jp: "スカサハ＝スカディ",
    name_en: "Scathach-Skadi",
    class: "Caster",
    rarity: 5,
    noblePhantasmName: "死亡满溢的魔境之门",
  },
  {
    id: 5,
    variantKey: "5:1",
    faceId: 1,
    name_cn: "阿拉什",
    name_jp: "アーラシュ",
    name_en: "Arash",
    class: "Archer",
    rarity: 1,
    noblePhantasmName: "流星一条",
  },
  {
    id: 6,
    variantKey: "6:1",
    faceId: 1,
    name_cn: "陈宫",
    name_jp: "陳宮",
    name_en: "Chen Gong",
    class: "Caster",
    rarity: 2,
    noblePhantasmName: "掎角一阵",
  },
];

const craftEssences: CraftEssence[] = [
  { id: 1001, name: "万华镜" },
  { id: 1002, name: "黑之圣杯" },
  { id: 1003, name: "迦勒底午餐时光" },
  { id: 1004, name: "虚数魔术" },
];

let selectedAdbSerial: string | null = null;
let server: Server = "JP";
let supportCeThreshold = 0.7;
let supportCeFullGateThreshold = 0.6;
let supportMlbIconThreshold = 0.7;
let supportBondIconThreshold = 0.7;
let noblePhantasmDetectionMode: "card" | "gauge" = "card";
let stopOnBondLevelUp = false;
let stopOnBondMaxLevel = false;
let verifySkillActivation = false;
let enableExtraClassFilter = true;
let autoCaptureBattleResultLoot = false;
let autoCaptureUnknownScreenTimeout = false;
let autoCaptureSkillUseProbe = false;
let nextProjectNumber = 2;
let activeProjectId: string | null = "dev-project-1";
let appTheme: "light" | "dark" | "system" | null = null;

let projects: Project[] = [
  {
    id: "dev-project-1",
    name: "模拟队伍",
    advancedMode: false,
    supportServantId: 4,
    supportServantVariantKey: "4:1",
    supportGrandMode: false,
    supportGrandCraftEssenceIds: [null, null, null],
    supportGrandCraftEssenceMlbRequired: [true, true, true],
    supportGrandBondCeMode: "any",
    grandServants: [],
    repeatMission: true,
    slots: createInitialProjectSlots().map((slot, index) => {
      const servant = [servants[1], servants[2], null, servants[4], servants[5], null][index];
      return {
        ...slot,
        servantId: servant?.id ?? null,
        servantVariantKey: servant?.variantKey ?? null,
        craftEssenceId: index === 2 ? 1003 : index === 0 ? 1001 : null,
      };
    }),
  },
];

const battleScenesByProject = new Map<string, BattleScene[]>([
  [
    "dev-project-1",
    [
      {
        id: "dev-scene-1",
        turns: [
          {
            id: "dev-scene-1-turn-1",
            preparationActions: [],
            servantActions: [],
            equipmentActions: [],
            commandSpellActions: [],
            attackPriority: [],
          },
        ],
      },
    ],
  ],
]);

const advancedBattleScenesByProject = new Map<string, AdvancedBattleScene[]>();

function clone<T>(value: T): T {
  return structuredClone(value);
}

function createProject(name: string, advancedMode = false): Project {
  const id = `dev-project-${nextProjectNumber++}`;
  return {
    id,
    name,
    advancedMode,
    supportServantId: null,
    supportServantVariantKey: null,
    supportGrandMode: advancedMode,
    supportGrandCraftEssenceIds: [null, null, null],
    supportGrandCraftEssenceMlbRequired: [true, true, true],
    supportGrandBondCeMode: "any",
    grandServants: [],
    repeatMission: false,
    repeatMode: "single",
    repeatCount: null,
    apRecoveryItems: [],
    slots: createInitialProjectSlots(),
  };
}

function recognitionSettings() {
  return {
    noblePhantasmDetectionMode,
    supportCeThreshold,
    supportCeFullGateThreshold,
    supportMlbIconThreshold,
    supportBondIconThreshold,
    stopOnBondLevelUp,
    stopOnBondMaxLevel,
    verifySkillActivation,
    enableExtraClassFilter,
  };
}

function debugSettings() {
  return {
    autoCaptureBattleResultLoot,
    autoCaptureUnknownScreenTimeout,
    autoCaptureSkillUseProbe,
  };
}

export async function invokeDevMock<T>(cmd: string, args: InvokeArgs = {}): Promise<T> {
  switch (cmd) {
    case "run_startup_migration":
      return { migrated: false, from: null, to: "/dev/mash-app-data" } as T;
    case "get_servants":
      return clone(servants) as T;
    case "get_craft_essences":
      return clone(craftEssences) as T;
    case "list_projects":
      return clone(projects) as T;
    case "get_grand_class_definitions":
      return [
        { id: "saber", label: "剑阶冠位", servantClass: "Saber", roles: [{ role: "main", label: "主", required: true }, { role: "deputy", label: "副", required: false }], cardPriorityEnabled: true, autoOrderChangeRoles: ["main"], validationMessage: "戴冠战需要选择 1 到 2 名冠位从者" },
        { id: "lancer", label: "枪阶冠位", servantClass: "Lancer", roles: [{ role: "single", label: "单体", required: true }, { role: "aoe", label: "光炮", required: true }], cardPriorityEnabled: false, autoOrderChangeRoles: ["single", "aoe"], validationMessage: "枪阶戴冠战需要分别选择单体和光炮从者" },
        { id: "berserker", label: "狂阶冠位", servantClass: "Berserker", roles: [{ role: "main", label: "主", required: true }, { role: "deputy", label: "副", required: false }], cardPriorityEnabled: true, autoOrderChangeRoles: ["main"], validationMessage: "戴冠战需要选择 1 到 2 名冠位从者" },
        { id: "extra1Fire", label: "Extra1 · 火", servantClass: "Extra1", selectionGroup: "extra1", selectionGroupLabel: "额外职阶 Ⅰ 冠位", selectionOptionLabel: "火", roles: [{ role: "main", label: "主", required: true }, { role: "deputy", label: "副", required: false }], cardPriorityEnabled: false, autoOrderChangeRoles: ["main"], validationMessage: "Extra1 · 火戴冠战需要选择主冠位，副冠位可选" },
        { id: "extra1Earth", label: "Extra1 · 地", servantClass: "Extra1", selectionGroup: "extra1", selectionGroupLabel: "额外职阶 Ⅰ 冠位", selectionOptionLabel: "地", roles: [{ role: "aoe", label: "光炮", required: true }, { role: "single", label: "单体", required: false }], cardPriorityEnabled: false, autoOrderChangeRoles: ["aoe", "single"], validationMessage: "Extra1 · 地戴冠战需要选择光炮从者，单体从者可选" },
        { id: "extra2Wind", label: "Extra2 · 风", servantClass: "Extra2", selectionGroup: "extra2", selectionGroupLabel: "额外职阶 Ⅱ 冠位", selectionOptionLabel: "风", roles: [{ role: "main", label: "主", required: true }, { role: "deputy", label: "副", required: false }], cardPriorityEnabled: false, autoOrderChangeRoles: ["main"], validationMessage: "Extra2 · 风戴冠战需要选择主冠位，副冠位可选" },
        { id: "extra2Water", label: "Extra2 · 水", servantClass: "Extra2", selectionGroup: "extra2", selectionGroupLabel: "额外职阶 Ⅱ 冠位", selectionOptionLabel: "水", roles: [{ role: "aoe", label: "光炮", required: true }, { role: "single", label: "单体", required: false }], cardPriorityEnabled: false, autoOrderChangeRoles: ["aoe", "single"], validationMessage: "Extra2 · 水戴冠战需要选择光炮从者，单体从者可选" },
      ] as T;
    case "get_active_project_id":
      return activeProjectId as T;
    case "set_active_project_id":
      activeProjectId =
        typeof args.activeProjectId === "string" ? args.activeProjectId : null;
      return null as T;
    case "get_app_theme":
      return appTheme as T;
    case "set_app_theme":
      if (args.theme === "light" || args.theme === "dark" || args.theme === "system") {
        appTheme = args.theme;
      }
      return null as T;
    case "create_project": {
      const project = createProject(
        String(args.name ?? `模拟队伍 ${nextProjectNumber}`),
        args.advancedMode === true
      );
      projects = [...projects, project];
      activeProjectId = project.id;
      return clone(project) as T;
    }
    case "duplicate_project": {
      const source = projects.find((project) => project.id === args.sourceId);
      if (!source) {
        return null as T;
      }
      const project: Project = {
        ...clone(source),
        id: `dev-project-${nextProjectNumber++}`,
        name: String(args.name ?? `${source.name} 副本`),
      };
      projects = [...projects, project];
      activeProjectId = project.id;
      battleScenesByProject.set(
        project.id,
        clone(battleScenesByProject.get(source.id) ?? [])
      );
      advancedBattleScenesByProject.set(
        project.id,
        clone(advancedBattleScenesByProject.get(source.id) ?? [])
      );
      return clone(project) as T;
    }
    case "update_project": {
      const project = args.project as Project;
      projects = projects.map((item) => (item.id === project.id ? clone(project) : item));
      return clone(project) as T;
    }
    case "delete_project":
      projects = projects.filter((project) => project.id !== args.id);
      if (activeProjectId === args.id) {
        activeProjectId = projects[0]?.id ?? null;
      }
      battleScenesByProject.delete(String(args.id));
      advancedBattleScenesByProject.delete(String(args.id));
      return null as T;
    case "load_battle_scenes":
      return clone(battleScenesByProject.get(String(args.projectId)) ?? []) as T;
    case "save_battle_scenes":
      battleScenesByProject.set(String(args.projectId), clone(args.scenes as BattleScene[]));
      return null as T;
    case "load_advanced_battle_scenes":
      return clone(advancedBattleScenesByProject.get(String(args.projectId)) ?? []) as T;
    case "save_advanced_battle_scenes":
      advancedBattleScenesByProject.set(
        String(args.projectId),
        clone(args.scenes as AdvancedBattleScene[])
      );
      return null as T;
    case "list_exportable_configs":
      return projects.map((project) => ({
        id: project.id,
        name: project.name,
        advancedMode: project.advancedMode === true,
        battleSceneCount: battleScenesByProject.get(project.id)?.length ?? 0,
        advancedBattleSceneCount: advancedBattleScenesByProject.get(project.id)?.length ?? 0,
      })) as T;
    case "export_configs":
      return {
        filePath: "/tmp/mash-config-dev.mashconfig.zip",
        exportedCount: Array.isArray(args.projectIds) ? args.projectIds.length : 0,
      } as T;
    case "pick_config_import_file":
      return "/tmp/mash-config-dev.mashconfig.json" as T;
    case "preview_config_import":
      return {
        fileName: "mash-config-dev.mashconfig.json",
        validConfigs: [
          {
            importKey: "0",
            sourceName: "示例配置",
            targetName: "示例配置（导入）",
            advancedMode: false,
            battleSceneCount: 1,
            advancedBattleSceneCount: 0,
          },
        ],
        invalidItems: [],
      } as T;
    case "import_configurations":
      return { importedProjects: [] } as T;
    case "check_adb":
      return { connected: selectedAdbSerial != null, deviceName: selectedAdbSerial } as T;
    case "get_selected_adb_device":
      return selectedAdbSerial as T;
    case "refresh_adb_devices_with_previews":
      if (selectedAdbSerial == null) selectedAdbSerial = "127.0.0.1:5555";
      return [
        {
          serial: "127.0.0.1:5555",
          description: "product:bluestacks model:dev",
          previewPath: "/tmp/mash-dev-adb-preview.png",
          selected: selectedAdbSerial === "127.0.0.1:5555",
        },
      ] as T;
    case "select_adb_device":
      selectedAdbSerial = String(args.serial);
      return { connected: true, deviceName: selectedAdbSerial } as T;
    case "connect_adb_port":
      selectedAdbSerial = `127.0.0.1:${Number(args.port)}`;
      return selectedAdbSerial as T;
    case "reset_bluestacks_adb_connection":
      return {
        ok: true,
        steps: [
          {
            command: "adb disconnect 127.0.0.1:5555",
            success: true,
            status: 0,
            stdout: "disconnected 127.0.0.1:5555",
            stderr: "",
          },
          {
            command: "adb kill-server",
            success: true,
            status: 0,
            stdout: "",
            stderr: "",
          },
          {
            command: "adb start-server",
            success: true,
            status: 0,
            stdout: "",
            stderr: "",
          },
          {
            command: "adb connect 127.0.0.1:5555",
            success: true,
            status: 0,
            stdout: "connected to 127.0.0.1:5555",
            stderr: "",
          },
          {
            command: "adb devices -l",
            success: true,
            status: 0,
            stdout: "List of devices attached\n127.0.0.1:5555 device",
            stderr: "",
          },
          {
            command: "adb -s 127.0.0.1:5555 shell echo ok",
            success: true,
            status: 0,
            stdout: "ok",
            stderr: "",
          },
        ],
      } as T;
    case "get_use_bluestack":
      return false as T;
    case "set_use_bluestack":
      return null as T;
    case "get_server":
      return server as T;
    case "set_server":
      if (args.value === "JP" || args.value === "CN") {
        server = args.value;
      }
      return null as T;
    case "get_recognition_settings":
      return recognitionSettings() as T;
    case "get_debug_settings":
      return debugSettings() as T;
    case "set_noble_phantasm_detection_mode":
      if (args.value === "card" || args.value === "gauge") {
        noblePhantasmDetectionMode = args.value;
      }
      return recognitionSettings() as T;
    case "set_support_ce_threshold":
      supportCeThreshold =
        typeof args.value === "number"
          ? Math.min(0.85, Math.max(0.6, args.value))
          : supportCeThreshold;
      return recognitionSettings() as T;
    case "set_support_ce_full_gate_threshold":
      supportCeFullGateThreshold =
        typeof args.value === "number"
          ? Math.min(0.7, Math.max(0.4, args.value))
          : supportCeFullGateThreshold;
      return recognitionSettings() as T;
    case "set_support_mlb_icon_threshold":
      supportMlbIconThreshold =
        typeof args.value === "number"
          ? Math.min(0.85, Math.max(0.6, args.value))
          : supportMlbIconThreshold;
      return recognitionSettings() as T;
    case "set_support_bond_icon_threshold":
      supportBondIconThreshold =
        typeof args.value === "number"
          ? Math.min(0.85, Math.max(0.6, args.value))
          : supportBondIconThreshold;
      return recognitionSettings() as T;
    case "set_stop_on_bond_level_up":
      stopOnBondLevelUp = Boolean(args.value);
      return recognitionSettings() as T;
    case "set_stop_on_bond_max_level":
      stopOnBondMaxLevel = Boolean(args.value);
      if (stopOnBondMaxLevel) {
        stopOnBondLevelUp = false;
      }
      return recognitionSettings() as T;
    case "set_verify_skill_activation":
      verifySkillActivation = Boolean(args.value);
      return recognitionSettings() as T;
    case "set_enable_extra_class_filter":
      enableExtraClassFilter = Boolean(args.value);
      return recognitionSettings() as T;
    case "set_auto_capture_battle_result_loot":
      autoCaptureBattleResultLoot = Boolean(args.value);
      return debugSettings() as T;
    case "set_auto_capture_unknown_screen_timeout":
      autoCaptureUnknownScreenTimeout = Boolean(args.value);
      return debugSettings() as T;
    case "set_auto_capture_skill_use_probe":
      autoCaptureSkillUseProbe = Boolean(args.value);
      return debugSettings() as T;
    case "pick_asset_bundle":
    case "pick_runtime_bundle":
    case "get_servant_portrait_path":
    case "get_servant_face_path":
    case "get_craft_essence_card_path":
    case "get_template_asset_path":
      return null as T;
    case "get_skill_icon_paths":
      return [{ path: null, name: "" }, { path: null, name: "" }, { path: null, name: "" }] as T;
    case "get_runtime_status":
      return {
        requiredRuntimeVersion: "2026.05.08-runtime1",
        installedRuntimeVersion: null,
        runtimeInstalled: false,
        requiredCodeVersion: "2026.05.08-code1",
        installedCodeVersion: null,
        codeInstalled: false,
        installed: false,
        platform: "darwin-aarch64",
        runtimeDownloadUrl:
          "https://cdn.example.com/mash-cv-runtime-darwin-aarch64-v2026.05.08-runtime1.zip",
        runtimeExpectedSha256:
          "0000000000000000000000000000000000000000000000000000000000000000",
        runtimeInstallDir: "/dev/runtime/mash-cv/runtime/2026.05.08-runtime1",
        executablePath:
          "/dev/runtime/mash-cv/runtime/2026.05.08-runtime1/mash-cv-runtime/mash-cv",
        codeDownloadUrl: "https://cdn.example.com/mash-cv-code-v2026.05.08-code1.zip",
        codeExpectedSha256:
          "0000000000000000000000000000000000000000000000000000000000000000",
        codeInstallDir: "/dev/runtime/mash-cv/code/2026.05.08-code1",
        codePath: "/dev/runtime/mash-cv/code/2026.05.08-code1/mash-cv-code",
      } as T;
    case "get_asset_bundle_status":
      return {
        installed: false,
        importedServants: false,
        importedCraftEssences: false,
        servantFiles: 0,
        craftEssenceFiles: 0,
        installDir: "/dev/mash-assets",
        currentVersion: null,
        appAssetsVersion: 2,
        remoteLatestVersion: null,
        remoteLatestBaseVersion: null,
        targetVersion: 2,
        updateAvailable: true,
        updateDownloadSize: 0,
        updatePlan: "pending",
        latestUrl: "https://mash.xiaotongx.com/mash/assets/latest.json",
        remoteManifestUrl: null,
        updateCheckError: null,
      } as T;
    case "get_self_check_status":
      return {
        appVersion: "0.5.4",
        cvRuntimeVersion: "2026.05.08-runtime1",
        cvRuntimeInstalled: false,
        cvCodeVersion: "2026.05.08-code1",
        cvCodeInstalled: false,
        assetVersion: null,
        appAssetsVersion: 2,
        servants: { entries: 0, hasImage: false, hasJson: false },
        ces: { entries: 0, hasImage: false, hasJson: false },
      } as T;
    case "import_asset_bundle":
      return {
        importedServants: false,
        importedCraftEssences: false,
        servantFiles: 0,
        craftEssenceFiles: 0,
        installDir: "/dev/mash-assets",
      } as T;
    case "download_asset_bundles":
      return {
        installed: true,
        installedVersion: 2,
        plan: "base",
        servantFiles: 0,
        craftEssenceFiles: 0,
        installDir: "/dev/mash-assets",
      } as T;
    case "import_runtime_bundle":
      return {
        installedKind: "runtime",
        installedVersion: "2026.05.08-runtime1",
        platform: "darwin-aarch64",
        installDir: "/dev/runtime/mash-cv/runtime/2026.05.08-runtime1",
        executablePath:
          "/dev/runtime/mash-cv/runtime/2026.05.08-runtime1/mash-cv-runtime/mash-cv",
        codePath: null,
      } as T;
    case "download_runtime_bundles":
      return {
        installed: [
          {
            installedKind: "runtime",
            installedVersion: "2026.05.08-runtime1",
            platform: "darwin-aarch64",
            installDir: "/dev/runtime/mash-cv/runtime/2026.05.08-runtime1",
            executablePath:
              "/dev/runtime/mash-cv/runtime/2026.05.08-runtime1/mash-cv-runtime/mash-cv",
            codePath: null,
          },
          {
            installedKind: "code",
            installedVersion: "2026.05.08-code1",
            platform: "darwin-aarch64",
            installDir: "/dev/runtime/mash-cv/code/2026.05.08-code1",
            executablePath: null,
            codePath: "/dev/runtime/mash-cv/code/2026.05.08-code1/mash-cv-code",
          },
        ],
      } as T;
    case "start_automation":
    case "stop_automation":
    case "stop_automation_after_current":
    case "start_enhancement_automation":
    case "stop_enhancement_automation":
    case "start_craft_essence_enhancement_automation":
    case "stop_craft_essence_enhancement_automation":
    case "debug_shutdown":
    case "debug_reload_sidecar":
      return null as T;
    case "debug_get_cv_config":
      return { screens: {} } as T;
    case "debug_list_templates":
      return [] as T;
    case "debug_list_servant_assets":
      return servants.map((servant) => servant.id) as T;
    case "debug_runner_coordinates":
      return { groups: [] } as T;
    case "get_servant_metadata": {
      const servant =
        servants.find(
          (item) =>
            item.id === args.servantId && item.variantKey === args.variantKey
        ) ?? servants.find((item) => item.id === args.servantId);
      return {
        id: servant?.id ?? args.servantId,
        name: servant?.name_cn ?? "模拟从者",
        names: servant ? [servant.name_cn] : ["模拟从者"],
        npNames: servant?.noblePhantasmName ? [servant.noblePhantasmName] : [],
      } as T;
    }
    default:
      throw new Error(`开发模式暂未模拟 Tauri 命令：${cmd}`);
  }
}
