import { createInitialProjectSlots } from "./components/projectSlots";
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

let useBluestack = false;
let server: Server = "JP";
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
    supportGrandMode: false,
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
      return { connected: false, deviceName: null } as T;
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
      return useBluestack as T;
    case "set_use_bluestack":
      useBluestack = Boolean(args.value);
      return null as T;
    case "get_server":
      return server as T;
    case "set_server":
      if (args.value === "JP" || args.value === "CN") {
        server = args.value;
      }
      return null as T;
    case "pick_asset_bundle":
    case "pick_runtime_bundle":
    case "get_servant_portrait_path":
    case "get_servant_face_path":
    case "get_craft_essence_card_path":
    case "get_template_asset_path":
      return null as T;
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
      const servant = servants.find((item) => item.id === args.servantId);
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
