import { useState, useEffect, useMemo, useCallback, useRef } from "react";
import { Box, Button, Flex, Text, Spinner } from "@radix-ui/themes";
import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
import { invoke, listen } from "./tauri";
import { ArrowLeftIcon } from "@radix-ui/react-icons";
import { ContentGrid } from "./features/team/ContentGrid";
import {
  derivePartyMembers,
  derivePartyLineup,
  derivePartyServants,
  relocateAdvancedBattleSceneMembers,
  relocateBattleSceneMembers,
} from "./features/team/partyServants";
import { CommandEditor } from "./features/battle/CommandEditor";
import { BattlePage } from "./features/battle/BattlePage";
import { EnhancementPage } from "./features/enhancement/EnhancementPage";
import { CraftEssenceEnhancementPage } from "./features/craft-essence-enhancement/CraftEssenceEnhancementPage";
import { FriendPointSummonPage } from "./features/friend-point-summon/FriendPointSummonPage";
import { DebugPage } from "./features/debug/DebugPage";
import { StatusBar } from "./features/status/StatusBar";
import { ProjectBar } from "./features/projects/ProjectBar";
import { ProjectSettingsDialog } from "./features/projects/ProjectSettingsDialog";
import { SetupPage } from "./features/setup/SetupPage";
import { SettingsDialog, type SettingsSection } from "./features/settings/SettingsPage";
import { SelfCheckDialog } from "./features/settings/SelfCheckDialog";
import { SoftwareUpdateDialog } from "./features/settings/SoftwareUpdateDialog";
import { createInitialProjectSlots } from "./features/team/projectSlots";
import { relocateGrandCardStrategySlots, relocateGrandServants } from "./features/advanced/grandRuleSlots";
import { featureToggles } from "./featureToggles";
import type { SlotItem } from "./features/team/ContentGrid";
import type { Servant } from "./types/servant";
import type { CraftEssence } from "./types/craftEssence";
import type {
  GrandClass,
  GrandClassDefinition,
  Project,
  ProjectCatalog,
} from "./types/project";
import type { AssetBundleStatus } from "./types/assets";
import type { RuntimeStatus } from "./types/runtime";
import type { SelfCheckStatus } from "./types/selfCheck";
import type { AppTheme, AppThemePreference } from "./types/theme";
import {
  DEFAULT_BATTLE_START_PANEL,
  normalizeBattleStartPanel,
  type BattleStartPanel,
} from "./types/appUiSettings";
import type { AdvancedBattleScene, BattleScene } from "./types/command";
import type { AutomationStatus } from "./types/automation";
import {
  EMPTY_AP_RECOVERY_USAGE,
  localBattleDayBounds,
  type BattleDailyStatistics,
  type BattleRunProgressEvent,
  type BattleRunStatus,
} from "./types/battleRunStatus";
import {
  appendCoalescedOperationLog,
  type ActionLogMeta,
  type AttackLogMeta,
  type LogLevel,
  type OperationLogEntry,
} from "./operationLog";
// Linear flow: 队伍设置 → 指令设置 → 开始任务. Each forward step is
// triggered by the bottom-right primary button on the previous page;
// `debug` is reached out-of-band from the sidebar. Replaces the older
// horizontal `StageNavigator` (queue/support/command tabs).
type View =
  | "team"
  | "command"
  | "battle"
  | "enhancement"
  | "craftEssenceEnhancement"
  | "friendPointSummon"
  | "debug";

interface AutomationEvent {
  state: string;
  status: AutomationStatus;
  currentScreen: string;
  message: string;
  level: LogLevel;
  attack?: AttackLogMeta | null;
  action?: ActionLogMeta | null;
}

interface OperationDebugEvent {
  message: string;
}

interface AppProps {
  theme: AppTheme;
  themePreference: AppThemePreference;
  onThemeChange: (theme: AppThemePreference) => void;
  startupReady?: boolean;
}

function localDateKey(date: Date): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function App({ theme, themePreference, onThemeChange, startupReady = true }: AppProps) {
  const [view, setView] = useState<View>("team");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [projectSettingsOpen, setProjectSettingsOpen] = useState(false);
  const [settingsSection, setSettingsSection] = useState<SettingsSection>("basic");
  const [activeProjectId, setActiveProjectId] = useState<string | null>(null);
  const [projects, setProjects] = useState<Project[]>([]);
  const [projectCatalog, setProjectCatalog] = useState<ProjectCatalog>({
    schemaVersion: 1,
    groups: [],
    ungroupedProjectIds: [],
  });
  const [grandClassDefinitions, setGrandClassDefinitions] = useState<GrandClassDefinition[]>([]);
  const [servants, setServants] = useState<Servant[]>([]);
  const [craftEssences, setCraftEssences] = useState<CraftEssence[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [setupChecking, setSetupChecking] = useState(true);
  const [setupReady, setSetupReady] = useState(false);
  const [operationLogs, setOperationLogs] = useState<OperationLogEntry[]>([]);
  const [operationLogOpen, setOperationLogOpen] = useState(false);
  const [battleRunStatus, setBattleRunStatus] = useState<BattleRunStatus | null>(null);
  const [battleDailyStatistics, setBattleDailyStatistics] =
    useState<BattleDailyStatistics | null>(null);
  const [battleRunStatusOpen, setBattleRunStatusOpen] = useState(false);
  const [battleStartPanel, setBattleStartPanel] = useState<BattleStartPanel>(
    DEFAULT_BATTLE_START_PANEL
  );
  const [availableUpdate, setAvailableUpdate] = useState<Update | null>(null);
  const [softwareUpdateOpen, setSoftwareUpdateOpen] = useState(false);
  const [updateChecking, setUpdateChecking] = useState(false);
  const [updateInstalling, setUpdateInstalling] = useState(false);
  const [updateProgressText, setUpdateProgressText] = useState<string | null>(null);
  const [updateInstallError, setUpdateInstallError] = useState<string | null>(null);
  const [selfCheckOpen, setSelfCheckOpen] = useState(false);
  const [selfCheckLoading, setSelfCheckLoading] = useState(false);
  const [selfCheckStatus, setSelfCheckStatus] = useState<SelfCheckStatus | null>(null);
  const [selfCheckError, setSelfCheckError] = useState<string | null>(null);
  const automationListenerGenerationRef = useRef(0);

  const appendOperationLog = useCallback(
    (
      message: string,
      level: LogLevel = "info",
      attack?: AttackLogMeta | null,
      action?: ActionLogMeta | null,
    ) => {
      const d = new Date();
      const time = [d.getHours(), d.getMinutes(), d.getSeconds()]
        .map((n) => String(n).padStart(2, "0"))
        .join(":");
      setOperationLogs((prev) =>
        appendCoalescedOperationLog(prev, { time, message, level, attack, action })
      );
    },
    [],
  );

  const refreshBattleDailyStatistics = useCallback(async () => {
    if (!startupReady) return;
    const { dayStartMs, dayEndMs } = localBattleDayBounds();
    try {
      const statistics = await invoke<BattleDailyStatistics>(
        "get_battle_daily_statistics",
        { dayStartMs, dayEndMs },
      );
      setBattleDailyStatistics(statistics);
    } catch (err) {
      console.error("failed to load daily battle statistics", err);
    }
  }, [startupReady]);

  const handleAutomationStart = useCallback(() => {
    setOperationLogs([]);
    setBattleRunStatusOpen(false);
    setOperationLogOpen(true);
  }, []);

  const handleBattleAutomationStart = useCallback(
    (maxRuns: number | null) => {
      const startedAtMs = Date.now();
      setBattleRunStatus({
        phase: "starting",
        startedAtMs,
        endedAtMs: null,
        lastCompletedAtMs: null,
        completedRuns: 0,
        maxRuns,
        apRecoveryUsage: { ...EMPTY_AP_RECOVERY_USAGE },
      });
      setOperationLogs([]);
      setOperationLogOpen(battleStartPanel === "operationLog");
      setBattleRunStatusOpen(battleStartPanel === "runStatus");
    },
    [battleStartPanel]
  );

  const handleBattleAutomationStartFailed = useCallback(() => {
    setBattleRunStatus((current) =>
      current == null || current.endedAtMs != null
        ? current
        : { ...current, phase: "error", endedAtMs: Date.now() }
    );
  }, []);

  const handleOperationLogOpenChange = useCallback((open: boolean) => {
    setOperationLogOpen(open);
    if (open) setBattleRunStatusOpen(false);
  }, []);

  const handleBattleRunStatusOpenChange = useCallback((open: boolean) => {
    setBattleRunStatusOpen(open);
    if (open) setOperationLogOpen(false);
  }, []);

  const handleStandaloneAutomationStart = useCallback(() => {
    setOperationLogs([]);
    setBattleRunStatusOpen(false);
    setOperationLogOpen(false);
  }, []);

  const checkForUpdates = useCallback(async (manual: boolean) => {
    setUpdateChecking(true);
    setUpdateProgressText(null);
    if (manual) {
      handleOperationLogOpenChange(true);
    }
    appendOperationLog("正在检查更新…");
    try {
      const update = await check();
      if (update) {
        setAvailableUpdate(update);
        handleOperationLogOpenChange(true);
        if (manual) {
          setUpdateInstallError(null);
          setSoftwareUpdateOpen(true);
        }
        appendOperationLog(
          `发现新版本 ${update.version}（当前 ${update.currentVersion}）`
        );
      } else {
        setAvailableUpdate(null);
        appendOperationLog("当前已是最新版本");
      }
    } catch (err) {
      appendOperationLog(`检查更新失败: ${String(err)}`);
    } finally {
      setUpdateChecking(false);
    }
  }, [appendOperationLog, handleOperationLogOpenChange]);

  const runSelfCheck = useCallback(async () => {
    setSelfCheckOpen(true);
    setSelfCheckLoading(true);
    setSelfCheckError(null);
    setSelfCheckStatus(null);
    try {
      const next = await invoke<SelfCheckStatus>("get_self_check_status");
      setSelfCheckStatus(next);
    } catch (err) {
      setSelfCheckError(String(err));
    } finally {
      setSelfCheckLoading(false);
    }
  }, []);

  const saveAdbScreenshot = useCallback(async () => {
    handleOperationLogOpenChange(true);
    appendOperationLog("正在通过 ADB 截取原始截图...");
    try {
      const savedPath = await invoke<string | null>("save_adb_screenshot");
      if (savedPath) {
        appendOperationLog(`截图已保存: ${savedPath}`);
      } else {
        appendOperationLog("已取消保存截图");
      }
    } catch (err) {
      appendOperationLog(`截图失败: ${String(err)}`);
    }
  }, [appendOperationLog, handleOperationLogOpenChange]);

  const handleInstallUpdate = useCallback(async () => {
    if (!availableUpdate || updateInstalling) return;
    setUpdateInstalling(true);
    setUpdateInstallError(null);
    handleOperationLogOpenChange(true);
    setUpdateProgressText("准备下载");
    appendOperationLog(`开始下载更新 ${availableUpdate.version}…`);

    let downloaded = 0;
    const formatProgress = (event: DownloadEvent) => {
      if (event.event === "Started") {
        downloaded = 0;
        const total = event.data.contentLength;
        setUpdateProgressText(total ? `0 / ${(total / 1024 / 1024).toFixed(1)} MB` : "开始下载");
        return;
      }
      if (event.event === "Progress") {
        downloaded += event.data.chunkLength;
        setUpdateProgressText(`${(downloaded / 1024 / 1024).toFixed(1)} MB`);
        return;
      }
      setUpdateProgressText("正在安装");
    };

    try {
      await availableUpdate.downloadAndInstall(formatProgress);
      appendOperationLog("更新已安装，重启软件后生效");
      setAvailableUpdate(null);
      setUpdateProgressText(null);
      setSoftwareUpdateOpen(false);
    } catch (err) {
      appendOperationLog(`安装更新失败: ${String(err)}`);
      setUpdateInstallError(String(err));
    } finally {
      setUpdateInstalling(false);
    }
  }, [appendOperationLog, availableUpdate, handleOperationLogOpenChange, updateInstalling]);

  useEffect(() => {
    const listenerGeneration = ++automationListenerGenerationRef.current;
    const isCurrentListenerGeneration = () =>
      automationListenerGenerationRef.current === listenerGeneration;
    const unlistenBattle = listen<AutomationEvent>("automation-status", (event) => {
      if (!isCurrentListenerGeneration()) return;
      appendOperationLog(
        event.payload.message,
        event.payload.level ?? "info",
        event.payload.attack ?? null,
        event.payload.action ?? null,
      );
      setBattleRunStatus((current) => {
        if (current == null) return current;
        const { status } = event.payload;
        if (status === "starting" || status === "running") {
          return { ...current, phase: status, endedAtMs: null };
        }
        if (current.endedAtMs != null) return current;
        return {
          ...current,
          phase:
            status === "finished"
              ? "finished"
              : status === "error"
                ? "error"
                : "stopped",
          endedAtMs: Date.now(),
        };
      });
    });
    const unlistenBattleProgress = listen<BattleRunProgressEvent>(
      "battle-run-progress",
      (event) => {
        if (!isCurrentListenerGeneration()) return;
        const receivedAtMs = Date.now();
        setBattleRunStatus((current) => {
          if (current == null) return current;
          const completedAdvanced = event.payload.completedRuns > current.completedRuns;
          return {
            ...current,
            completedRuns: event.payload.completedRuns,
            maxRuns: event.payload.maxRuns,
            apRecoveryUsage: event.payload.apRecoveryUsage,
            lastCompletedAtMs: completedAdvanced
              ? receivedAtMs
              : current.lastCompletedAtMs,
          };
        });
      }
    );
    const unlistenEnhancement = listen<AutomationEvent>(
      "enhancement-automation-status",
      (event) => {
        if (!isCurrentListenerGeneration()) return;
        appendOperationLog(event.payload.message, event.payload.level ?? "info");
      }
    );
    const unlistenCraftEssenceEnhancement = listen<AutomationEvent>(
      "craft-essence-enhancement-automation-status",
      (event) => {
        if (!isCurrentListenerGeneration()) return;
        appendOperationLog(event.payload.message, event.payload.level ?? "info");
      }
    );
    const unlistenFriendPointSummon = listen<AutomationEvent>(
      "friend-point-summon-automation-status",
      (event) => {
        if (!isCurrentListenerGeneration()) return;
        appendOperationLog(event.payload.message, event.payload.level ?? "info");
      }
    );
    const unlistenOperationDebug = listen<OperationDebugEvent>(
      "operation-debug-log",
      (event) => {
        if (!isCurrentListenerGeneration()) return;
        appendOperationLog(event.payload.message, "debug");
      }
    );
    return () => {
      if (automationListenerGenerationRef.current === listenerGeneration) {
        automationListenerGenerationRef.current += 1;
      }
      unlistenBattle.then((fn) => fn());
      unlistenBattleProgress.then((fn) => fn());
      unlistenEnhancement.then((fn) => fn());
      unlistenCraftEssenceEnhancement.then((fn) => fn());
      unlistenFriendPointSummon.then((fn) => fn());
      unlistenOperationDebug.then((fn) => fn());
    };
  }, [appendOperationLog]);

  useEffect(() => {
    if (!startupReady) return;
    let cancelled = false;
    void refreshBattleDailyStatistics();
    const interval = window.setInterval(() => {
      if (!cancelled) void refreshBattleDailyStatistics();
    }, 60_000);
    const unlistenHistory = listen("battle-run-history-updated", () => {
      if (!cancelled) void refreshBattleDailyStatistics();
    });
    return () => {
      cancelled = true;
      window.clearInterval(interval);
      unlistenHistory.then((fn) => fn());
    };
  }, [refreshBattleDailyStatistics, startupReady]);

  useEffect(() => {
    let cancelled = false;
    const today = localDateKey(new Date());
    invoke<boolean>("should_check_updates_today", { today })
      .then((shouldCheck) => {
        if (!shouldCheck || cancelled) return;
        return checkForUpdates(false).finally(() => {
          if (!cancelled) {
            void invoke("mark_update_checked_today", { date: today });
          }
        });
      })
      .catch((err) => {
        console.error("daily update check failed", err);
      });

    const unlistenMenu = listen("updater-check-requested", () => {
      void checkForUpdates(true);
    });
    const unlistenSelfCheck = listen("self-check-requested", () => {
      void runSelfCheck();
    });
    const unlistenResourceManager = listen("resource-manager-requested", () => {
      setSettingsSection("resources");
      setSettingsOpen(true);
    });
    const unlistenSaveAdbScreenshot = listen("save-adb-screenshot-requested", () => {
      void saveAdbScreenshot();
    });

    return () => {
      cancelled = true;
      unlistenMenu.then((fn) => fn());
      unlistenSelfCheck.then((fn) => fn());
      unlistenResourceManager.then((fn) => fn());
      unlistenSaveAdbScreenshot.then((fn) => fn());
    };
  }, [checkForUpdates, runSelfCheck, saveAdbScreenshot]);

  useEffect(() => {
    let cancelled = false;
    Promise.all([
      invoke<RuntimeStatus>("get_runtime_status"),
      invoke<AssetBundleStatus>("get_asset_bundle_status"),
    ])
      .then(([runtime, assets]) => {
        if (cancelled) return;
        setSetupReady(runtime.installed && assets.installed);
      })
      .catch((err) => {
        if (cancelled) return;
        setError(String(err));
        setSetupReady(false);
      })
      .finally(() => {
        if (!cancelled) {
          setSetupChecking(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // Load both static catalogs in parallel. The CE catalog is small (just
  // id/name) and shared across all projects, so caching it on the App
  // component keeps the team-builder picker instant.
  useEffect(() => {
    Promise.all([
      invoke<Servant[]>("get_servants"),
      invoke<CraftEssence[]>("get_craft_essences"),
    ])
      .then(([s, ce]) => {
        setServants(s);
        setCraftEssences(ce);
      })
      .catch((err) => setError(String(err)))
      .finally(() => setLoading(false));
  }, []);

  const persistActiveProjectId = useCallback((id: string | null) => {
    void invoke("set_active_project_id", { activeProjectId: id }).catch(console.error);
  }, []);

  const handleProjectSelect = useCallback(
    (id: string) => {
      setActiveProjectId(id);
      persistActiveProjectId(id);
    },
    [persistActiveProjectId]
  );

  const refreshProjects = useCallback(async () => {
    const [list, catalog, savedActiveProjectId, definitions] = await Promise.all([
      invoke<Project[]>("list_projects"),
      invoke<ProjectCatalog>("get_project_catalog"),
      invoke<string | null>("get_active_project_id"),
      invoke<GrandClassDefinition[]>("get_grand_class_definitions"),
    ]);
    setProjects(list);
    setProjectCatalog(
      catalog ?? {
        schemaVersion: 1,
        groups: [],
        ungroupedProjectIds: list.map((project) => project.id),
      },
    );
    setGrandClassDefinitions(definitions ?? []);
    if (list.length > 0) {
      const savedProject = list.find((project) => project.id === savedActiveProjectId);
      const nextActiveProjectId = savedProject?.id ?? list[0].id;
      setActiveProjectId(nextActiveProjectId);
      if (nextActiveProjectId !== savedActiveProjectId) {
        persistActiveProjectId(nextActiveProjectId);
      }
    } else {
      setActiveProjectId(null);
      if (savedActiveProjectId != null) {
        persistActiveProjectId(null);
      }
    }
  }, [persistActiveProjectId]);

  // Load the project list once at startup so the team-builder can read/write
  // `supportServantId` directly off the active project. The active project id
  // is also backend-owned, so app restarts reopen the last chosen lineup.
  useEffect(() => {
    void refreshProjects().catch(console.error);
  }, [refreshProjects]);

  useEffect(() => {
    void invoke<BattleStartPanel>("get_battle_start_panel")
      .then((value) => setBattleStartPanel(normalizeBattleStartPanel(value)))
      .catch(console.error);
  }, []);

  const activeProject = useMemo(
    () => projects.find((p) => p.id === activeProjectId) ?? null,
    [projects, activeProjectId]
  );

  // Persist a project mutation through the backend and refresh local state.
  // The team-builder support slot and party slots both flow through this.
  const handleUpdateProject = useCallback(async (next: Project) => {
    const saved = await invoke<Project>("update_project", { project: next });
    setProjects((prev) => prev.map((p) => (p.id === saved.id ? saved : p)));
  }, []);

  // Derive the SlotItem array shown by ContentGrid from the active project.
  // The backend owns slot order + servant ids; here we just rehydrate the
  // referenced Servant objects so the UI can render names/classes/rarity.
  // When no project is active we fall back to the empty default layout so
  // the grid still renders (selections are no-ops in that case).
  const slots = useMemo<SlotItem[]>(() => {
    const raw = activeProject?.slots ?? createInitialProjectSlots();
    return raw.map((s) => {
      const craftEssenceIds = Array.from(
        new Set(
          (s.craftEssenceIds?.length
            ? s.craftEssenceIds
            : s.craftEssenceId != null
              ? [s.craftEssenceId]
              : []
          ).slice(0, 10)
        )
      );
      const slotCraftEssences = craftEssenceIds
        .map((id) => craftEssences.find((ce) => ce.id === id) ?? null)
        .filter((ce): ce is CraftEssence => ce != null);
      return {
        id: s.id,
        type: s.type,
        servant:
          s.servantId != null
            ? (servants.find((sv) => sv.variantKey === s.servantVariantKey) ??
              servants.find((sv) => sv.id === s.servantId) ??
              null)
            : null,
        craftEssence: slotCraftEssences[0] ?? null,
        craftEssences: slotCraftEssences,
        craftEssenceMultiSelect: s.craftEssenceMultiSelect ?? false,
        craftEssenceMlbRequired: s.craftEssenceMlbRequired ?? true,
      };
    });
  }, [activeProject, servants, craftEssences]);

  // Persist any slot mutation (drag-reorder or selection from the dialog)
  // back onto the project. ContentGrid still receives a synchronous-looking
  // setter so its DnD/select code stays unchanged.
  const handleSlotsChange = useCallback(
    (next: SlotItem[]) => {
      if (!activeProject) return;
      const projectSlots = next.map((s) => {
        const craftEssenceIds = Array.from(
          new Set(
            (s.craftEssences?.length
              ? s.craftEssences.map((ce) => ce.id)
              : s.craftEssence
                ? [s.craftEssence.id]
                : []
            ).slice(0, 10)
          )
        );
        return {
          id: s.id,
          type: s.type,
          servantId: s.servant?.id ?? null,
          servantVariantKey: s.servant?.variantKey ?? null,
          craftEssenceId: craftEssenceIds[0] ?? null,
          craftEssenceIds,
          craftEssenceMultiSelect:
            s.type === "support" && (s.craftEssenceMultiSelect ?? false),
          craftEssenceMlbRequired: s.craftEssenceMlbRequired ?? true,
        };
      });
      const grandCardStrategy = activeProject.grandCardStrategy
        ? relocateGrandCardStrategySlots(
            activeProject.grandCardStrategy,
            next,
            activeProject.supportServantId
          )
        : activeProject.grandCardStrategy;
      const grandServants = relocateGrandServants(
        activeProject.grandServants,
        next,
        activeProject.supportServantId
      );
      void handleUpdateProject({ ...activeProject, slots: projectSlots, grandCardStrategy, grandServants });
      const previousPartyMembers = derivePartyMembers(slots, activeProject, servants);
      const nextPartyMembers = derivePartyMembers(
        next,
        { ...activeProject, slots: projectSlots },
        servants
      );
      void invoke<AdvancedBattleScene[]>("load_advanced_battle_scenes", {
        projectId: activeProject.id,
      })
        .then((scenes) => {
          if (scenes.length === 0) return;
          const relocatedScenes = scenes.map((scene) =>
            relocateAdvancedBattleSceneMembers(scene, previousPartyMembers, nextPartyMembers)
          );
          return invoke("save_advanced_battle_scenes", {
            projectId: activeProject.id,
            scenes: relocatedScenes,
          });
        })
        .catch(console.error);
      void invoke<BattleScene[]>("load_battle_scenes", {
        projectId: activeProject.id,
      })
        .then((scenes) => {
          if (scenes.length === 0) return;
          const relocatedScenes = scenes.map((scene) =>
            relocateBattleSceneMembers(scene, previousPartyMembers, nextPartyMembers)
          );
          return invoke("save_battle_scenes", {
            projectId: activeProject.id,
            scenes: relocatedScenes,
          });
        })
        .catch(console.error);
    },
    [activeProject, handleUpdateProject, servants, slots]
  );

  const partyServants = useMemo(
    () => derivePartyServants(slots, activeProject, servants),
    [slots, activeProject, servants]
  );
  const partyLineup = useMemo(
    () => derivePartyLineup(slots, activeProject, servants),
    [slots, activeProject, servants]
  );
  const partyMembers = useMemo(
    () => derivePartyMembers(slots, activeProject, servants),
    [slots, activeProject, servants]
  );

  // Stable list of front-line servant ids (deduped, drops nulls). Fed to
  // the Debug page so its "候选从者 id" input pre-fills with the same
  // candidate set the runner would use during `handle_attack`, instead of
  // making the user manually paste ids before the face-matcher will do
  // anything.
  const partyServantIds = useMemo(() => {
    const seen = new Set<number>();
    const ids: number[] = [];
    for (const s of partyServants) {
      if (s && !seen.has(s.id)) {
        seen.add(s.id);
        ids.push(s.id);
      }
    }
    return ids;
  }, [partyServants]);

  // Project create/delete used to live inside `Sidebar`; with the
  // sidebar now reduced to action buttons, the picker moves to the
  // `<ProjectBar/>` ribbon above the team grid and the mutations live
  // here so both `App` and `ProjectBar` mutate the same lifted state.
  const refreshProjectCatalog = useCallback(async () => {
    const catalog = await invoke<ProjectCatalog>("get_project_catalog");
    setProjectCatalog(catalog);
  }, []);

  const handleCreateProject = useCallback((
    name: string,
    advancedMode = false,
    grandClass: GrandClass = "saber",
    groupId: string | null = null,
  ) => {
    invoke<Project>("create_project", { name, advancedMode, grandClass, groupId })
      .then((p) => {
        setProjects((prev) => [...prev, p]);
        setActiveProjectId(p.id);
        persistActiveProjectId(p.id);
        void refreshProjectCatalog();
      })
      .catch(console.error);
  }, [persistActiveProjectId, refreshProjectCatalog]);

  const handleRenameProject = useCallback(
    (id: string, name: string) => {
      const project = projects.find((p) => p.id === id);
      if (!project) return;
      void handleUpdateProject({ ...project, name });
    },
    [handleUpdateProject, projects]
  );

  const handleDuplicateProject = useCallback((id: string, name: string) => {
    invoke<Project>("duplicate_project", { sourceId: id, name })
      .then((p) => {
        setProjects((prev) => [...prev, p]);
        setActiveProjectId(p.id);
        persistActiveProjectId(p.id);
        void refreshProjectCatalog();
      })
      .catch(console.error);
  }, [persistActiveProjectId, refreshProjectCatalog]);

  const handleDeleteProject = useCallback(
    (id: string) => {
      invoke("delete_project", { id })
        .then(() => {
          setProjects((prev) => {
            const next = prev.filter((p) => p.id !== id);
            if (activeProjectId === id) {
              const nextActiveProjectId = next.length > 0 ? next[0].id : null;
              setActiveProjectId(nextActiveProjectId);
              persistActiveProjectId(nextActiveProjectId);
            }
            return next;
          });
          void refreshProjectCatalog();
        })
        .catch(console.error);
    },
    [activeProjectId, persistActiveProjectId, refreshProjectCatalog]
  );

  const handleCreateProjectGroup = useCallback(async (name: string) => {
    const catalog = await invoke<ProjectCatalog>("create_project_group", { name });
    setProjectCatalog(catalog);
  }, []);

  const handleRenameProjectGroup = useCallback(async (groupId: string, name: string) => {
    const catalog = await invoke<ProjectCatalog>("rename_project_group", { groupId, name });
    setProjectCatalog(catalog);
  }, []);

  const handleDeleteProjectGroup = useCallback(async (groupId: string) => {
    const catalog = await invoke<ProjectCatalog>("delete_project_group", { groupId });
    setProjectCatalog(catalog);
  }, []);

  const handleMoveProjectToGroup = useCallback(
    async (projectId: string, groupId: string | null) => {
      const catalog = await invoke<ProjectCatalog>("move_project_to_group", {
        projectId,
        groupId,
      });
      setProjectCatalog(catalog);
    },
    [],
  );

  const handleReorderProjectGroups = useCallback(async (groupIds: string[]) => {
    const catalog = await invoke<ProjectCatalog>("reorder_project_groups", { groupIds });
    setProjectCatalog(catalog);
  }, []);

  const handleReorderProjectsInGroup = useCallback(
    async (groupId: string | null, projectIds: string[]) => {
      const catalog = await invoke<ProjectCatalog>("reorder_projects_in_group", {
        groupId,
        projectIds,
      });
      setProjectCatalog(catalog);
    },
    [],
  );

  const handleStartRun = useCallback(() => {
    setView("battle");
  }, []);

  const handleProjectsImported = useCallback((importedProjects: Project[]) => {
    void refreshProjects()
      .then(() => {
        const firstImported = importedProjects[0];
        if (firstImported) {
          setActiveProjectId(firstImported.id);
          persistActiveProjectId(firstImported.id);
        }
      })
      .catch(console.error);
  }, [persistActiveProjectId, refreshProjects]);

  const handleOpenDebug = useCallback(() => {
    if (!featureToggles.cvDebug) return;
    setView("debug");
  }, []);

  const handleOpenEnhancement = useCallback(() => {
    if (!featureToggles.servantEnhancement) return;
    setView("enhancement");
  }, []);

  const handleOpenCraftEssenceEnhancement = useCallback(() => {
    if (!featureToggles.craftEssenceEnhancement) return;
    setView("craftEssenceEnhancement");
  }, []);

  const handleOpenFriendPointSummon = useCallback(() => {
    if (!featureToggles.friendPointSummon) return;
    setView("friendPointSummon");
  }, []);

  const handleOpenSettings = useCallback(() => {
    setSettingsSection("basic");
    setSettingsOpen(true);
  }, []);

  const handleOpenProjectSettings = useCallback(() => {
    if (!activeProject) return;
    setProjectSettingsOpen(true);
  }, [activeProject]);

  // After the runner exits we return to the team page (the start of
  // the linear flow) rather than to "config", which no longer exists.
  const handleBackToConfig = useCallback(() => {
    setView("team");
  }, []);

  const handleGotoCommand = useCallback(() => {
    setView("command");
  }, []);

  const handleBackToTeam = useCallback(() => {
    setView("team");
  }, []);

  if (setupChecking) {
    return (
      <Flex direction="column" className="app-root" data-theme={theme}>
        <Flex align="center" justify="center" style={{ flex: 1 }}>
          <Spinner size="3" />
        </Flex>
        <SelfCheckDialog
          open={selfCheckOpen}
          loading={selfCheckLoading}
          status={selfCheckStatus}
          error={selfCheckError}
          onOpenChange={setSelfCheckOpen}
        />
      </Flex>
    );
  }

  if (!setupReady) {
    return (
      <Flex direction="column" className="app-root" data-theme={theme}>
        <SetupPage onReady={() => setSetupReady(true)} />
        <SettingsDialog
          open={settingsOpen}
          section={settingsSection}
          onOpenChange={setSettingsOpen}
          onSectionChange={setSettingsSection}
          onProjectsImported={handleProjectsImported}
          onBattleStartPanelChange={setBattleStartPanel}
        />
        <SelfCheckDialog
          open={selfCheckOpen}
          loading={selfCheckLoading}
          status={selfCheckStatus}
          error={selfCheckError}
          onOpenChange={setSelfCheckOpen}
        />
      </Flex>
    );
  }

  return (
    <Flex direction="column" className="app-root" data-theme={theme}>
      <Flex className="app-container">
        <Box className="main-content">
          {view === "battle" ? (
            <BattlePage
              projects={projects}
              projectCatalog={projectCatalog}
              grandClassDefinitions={grandClassDefinitions}
              servants={servants}
              activeProjectId={activeProjectId}
              onProjectSelect={handleProjectSelect}
              onCreateProject={handleCreateProject}
              onRenameProject={handleRenameProject}
              onDuplicateProject={handleDuplicateProject}
              onDeleteProject={handleDeleteProject}
              onCreateProjectGroup={handleCreateProjectGroup}
              onRenameProjectGroup={handleRenameProjectGroup}
              onDeleteProjectGroup={handleDeleteProjectGroup}
              onMoveProjectToGroup={handleMoveProjectToGroup}
              onReorderProjectGroups={handleReorderProjectGroups}
              onReorderProjectsInGroup={handleReorderProjectsInGroup}
              onOpenProjectSettings={handleOpenProjectSettings}
              onUpdateProject={handleUpdateProject}
              onBack={handleBackToConfig}
              onAutomationStart={handleBattleAutomationStart}
              onAutomationStartFailed={handleBattleAutomationStartFailed}
              onLogEntry={appendOperationLog}
            />
          ) : view === "enhancement" && featureToggles.servantEnhancement ? (
            <EnhancementPage
              servants={servants}
              onBack={handleBackToConfig}
              onAutomationStart={handleAutomationStart}
              onLogEntry={appendOperationLog}
            />
          ) : view === "craftEssenceEnhancement" &&
            featureToggles.craftEssenceEnhancement ? (
            <CraftEssenceEnhancementPage
              onBack={handleBackToConfig}
              onAutomationStart={handleStandaloneAutomationStart}
              onLogEntry={appendOperationLog}
            />
          ) : view === "friendPointSummon" &&
            featureToggles.friendPointSummon ? (
            <FriendPointSummonPage
              onBack={handleBackToConfig}
              onAutomationStart={handleStandaloneAutomationStart}
              onLogEntry={appendOperationLog}
            />
          ) : view === "debug" && featureToggles.cvDebug ? (
            <DebugPage
              onBack={handleBackToConfig}
              servants={servants}
              craftEssences={craftEssences}
              defaultCardServantIds={partyServantIds}
            />
          ) : (
            <Box className="main-content-inner">
              <ProjectBar
                projects={projects}
                projectCatalog={projectCatalog}
                grandClassDefinitions={grandClassDefinitions}
                activeProjectId={activeProjectId}
                onProjectSelect={handleProjectSelect}
                onCreateProject={handleCreateProject}
                onRenameProject={handleRenameProject}
                onDuplicateProject={handleDuplicateProject}
                onDeleteProject={handleDeleteProject}
                onCreateProjectGroup={handleCreateProjectGroup}
                onRenameProjectGroup={handleRenameProjectGroup}
                onDeleteProjectGroup={handleDeleteProjectGroup}
                onMoveProjectToGroup={handleMoveProjectToGroup}
                onReorderProjectGroups={handleReorderProjectGroups}
                onReorderProjectsInGroup={handleReorderProjectsInGroup}
                onOpenProjectSettings={handleOpenProjectSettings}
              />
              {loading ? (
                <Flex align="center" justify="center" style={{ flex: 1 }}>
                  <Spinner size="3" />
                </Flex>
              ) : error ? (
                <Flex
                  align="center"
                  justify="center"
                  direction="column"
                  gap="2"
                  style={{ flex: 1 }}
                >
                  <Text size="3" color="red" weight="medium">
                    加载从者数据失败
                  </Text>
                  <Text size="2" color="gray">
                    {error}
                  </Text>
                </Flex>
              ) : view === "command" ? (
                <>
                  <CommandEditor
                    key={activeProjectId ?? "no-project"}
                    projectId={activeProjectId}
                    partyLineup={partyLineup}
                    partyMembers={partyMembers}
                    advancedMode={activeProject?.advancedMode === true}
                    disableAutoSkillTargetRecognition={
                      activeProject?.disableAutoSkillTargetRecognition === true
                    }
                    grandServants={activeProject?.grandServants ?? []}
                    grandClass={activeProject?.grandClass ?? "saber"}
                    grandClassDefinition={grandClassDefinitions.find(
                      (definition) => definition.id === (activeProject?.grandClass ?? "saber"),
                    )}
                    grandCardStrategy={activeProject?.grandCardStrategy}
                    grandCardPriorityEnabled
                    turnAttackModesEnabled={featureToggles.turnAttackModes}
                    onGrandServantsChange={(grandServants) => {
                      if (!activeProject) return;
                      void handleUpdateProject({ ...activeProject, grandServants });
                    }}
                    onGrandCardStrategyChange={(grandCardStrategy) => {
                      if (!activeProject) return;
                      void handleUpdateProject({ ...activeProject, grandCardStrategy });
                    }}
                  />
                  <Flex justify="between" align="center" className="page-footer">
                    <Button
                      type="button"
                      variant="soft"
                      color="gray"
                      onClick={handleBackToTeam}
                    >
                      <ArrowLeftIcon width={14} height={14} />
                      <Text size="2">队伍设置</Text>
                    </Button>
                    <Button type="button" onClick={handleStartRun}>
                      <Text size="2" weight="bold">
                        开始任务
                      </Text>
                    </Button>
                  </Flex>
                </>
              ) : (
                <>
                  <Box className="team-stage">
                    <Box className="team-stage-body">
                      <ContentGrid
                        servants={servants}
                        craftEssences={craftEssences}
                        slots={slots}
                        onSlotsChange={handleSlotsChange}
                        activeProject={activeProject}
                        grandClassDefinitions={grandClassDefinitions}
                        onUpdateActiveProject={handleUpdateProject}
                      />
                    </Box>
                  </Box>
                  <Flex justify="between" align="center" className="page-footer" gap="3">
                    <Flex align="center" gap="3">
                      {featureToggles.friendPointSummon && (
                        <Button
                          type="button"
                          variant="soft"
                          color="gray"
                          onClick={handleOpenFriendPointSummon}
                        >
                          <Text size="2" weight="medium">
                            友情点抽取
                          </Text>
                        </Button>
                      )}
                      {featureToggles.craftEssenceEnhancement && (
                        <Button
                          type="button"
                          variant="soft"
                          color="gray"
                          onClick={handleOpenCraftEssenceEnhancement}
                        >
                          <Text size="2" weight="medium">
                            强化概念礼装
                          </Text>
                        </Button>
                      )}
                    </Flex>
                    <Flex align="center" gap="3">
                      {featureToggles.servantEnhancement && (
                        <Button
                          type="button"
                          variant="soft"
                          color="gray"
                          onClick={handleOpenEnhancement}
                        >
                          <Text size="2" weight="medium">
                            强化从者
                          </Text>
                        </Button>
                      )}
                      <Button type="button" onClick={handleGotoCommand}>
                        <Text size="2" weight="bold">
                          指令设置
                        </Text>
                      </Button>
                    </Flex>
                  </Flex>
                </>
              )}
            </Box>
          )}
        </Box>
      </Flex>
      <StatusBar
        onOpenSettings={handleOpenSettings}
        onOpenDebug={featureToggles.cvDebug ? handleOpenDebug : undefined}
        theme={theme}
        themePreference={themePreference}
        onThemeChange={onThemeChange}
        operationLogs={operationLogs}
        servants={servants}
        operationLogOpen={operationLogOpen}
        onOperationLogOpenChange={handleOperationLogOpenChange}
        battleRunStatus={battleRunStatus}
        battleDailyStatistics={battleDailyStatistics}
        battleRunStatusOpen={battleRunStatusOpen}
        onBattleRunStatusOpenChange={handleBattleRunStatusOpenChange}
        updateAvailable={availableUpdate != null}
        updateChecking={updateChecking}
        updateInstalling={updateInstalling}
        updateProgressText={updateProgressText}
        onInstallUpdate={handleInstallUpdate}
        onLogEntry={appendOperationLog}
      />
      <SettingsDialog
        open={settingsOpen}
        section={settingsSection}
        onOpenChange={setSettingsOpen}
        onSectionChange={setSettingsSection}
        onProjectsImported={handleProjectsImported}
        onBattleStartPanelChange={setBattleStartPanel}
      />
      <ProjectSettingsDialog
        open={projectSettingsOpen}
        project={activeProject}
        onOpenChange={setProjectSettingsOpen}
        onUpdateProject={handleUpdateProject}
      />
      <SelfCheckDialog
        open={selfCheckOpen}
        loading={selfCheckLoading}
        status={selfCheckStatus}
        error={selfCheckError}
        onOpenChange={setSelfCheckOpen}
      />
      <SoftwareUpdateDialog
        open={softwareUpdateOpen}
        currentVersion={availableUpdate?.currentVersion ?? null}
        version={availableUpdate?.version ?? null}
        installing={updateInstalling}
        progressText={updateProgressText}
        error={updateInstallError}
        onOpenChange={setSoftwareUpdateOpen}
        onInstall={handleInstallUpdate}
      />
    </Flex>
  );
}

export default App;
