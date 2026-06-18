import { useState, useEffect, useMemo, useCallback } from "react";
import { AlertDialog, Box, Button, Flex, Text, Spinner } from "@radix-ui/themes";
import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
import { invoke, listen } from "./tauri";
import { ArrowLeftIcon } from "@radix-ui/react-icons";
import { ContentGrid } from "./components/ContentGrid";
import {
  derivePartyMembers,
  derivePartyLineup,
  derivePartyServants,
} from "./components/partyServants";
import { CommandEditor } from "./components/CommandEditor";
import { BattlePage } from "./components/BattlePage";
import { EnhancementPage } from "./components/EnhancementPage";
import { DebugPage } from "./components/DebugPage";
import { StatusBar } from "./components/StatusBar";
import { ProjectBar } from "./components/ProjectBar";
import { SetupPage } from "./components/SetupPage";
import { SettingsDialog, type SettingsSection } from "./components/SettingsPage";
import { SelfCheckDialog } from "./components/SelfCheckDialog";
import { createInitialProjectSlots } from "./components/projectSlots";
import { featureToggles } from "./featureToggles";
import type { SlotItem } from "./components/ContentGrid";
import type { Servant } from "./types/servant";
import type { CraftEssence } from "./types/craftEssence";
import type { GrandClass, Project } from "./types/project";
import type { AssetBundleStatus } from "./types/assets";
import type { RuntimeStatus } from "./types/runtime";
import type { SelfCheckStatus } from "./types/selfCheck";
import type { AppTheme, AppThemePreference } from "./types/theme";
import {
  appendCoalescedOperationLog,
  type LogLevel,
  type OperationLogEntry,
} from "./operationLog";
import "./App.css";

// Linear flow: 队伍设置 → 指令设置 → 开始任务. Each forward step is
// triggered by the bottom-right primary button on the previous page;
// `debug` is reached out-of-band from the sidebar. Replaces the older
// horizontal `StageNavigator` (queue/support/command tabs).
type View = "team" | "command" | "battle" | "enhancement" | "debug";

interface AutomationEvent {
  state: string;
  currentScreen: string;
  message: string;
  // Optional for backwards compatibility with older backends that don't
  // emit a level; treat missing as "info".
  level?: LogLevel;
}

interface AppProps {
  theme: AppTheme;
  themePreference: AppThemePreference;
  onThemeChange: (theme: AppThemePreference) => void;
}

function localDateKey(date: Date): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function App({ theme, themePreference, onThemeChange }: AppProps) {
  const [view, setView] = useState<View>("team");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsSection, setSettingsSection] = useState<SettingsSection>("dataManagement");
  const [activeProjectId, setActiveProjectId] = useState<string | null>(null);
  const [projects, setProjects] = useState<Project[]>([]);
  const [servants, setServants] = useState<Servant[]>([]);
  const [craftEssences, setCraftEssences] = useState<CraftEssence[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [setupChecking, setSetupChecking] = useState(true);
  const [setupReady, setSetupReady] = useState(false);
  const [operationLogs, setOperationLogs] = useState<OperationLogEntry[]>([]);
  const [operationLogOpen, setOperationLogOpen] = useState(false);
  const [availableUpdate, setAvailableUpdate] = useState<Update | null>(null);
  const [updateChecking, setUpdateChecking] = useState(false);
  const [updateInstalling, setUpdateInstalling] = useState(false);
  const [updateProgressText, setUpdateProgressText] = useState<string | null>(null);
  const [selfCheckOpen, setSelfCheckOpen] = useState(false);
  const [selfCheckLoading, setSelfCheckLoading] = useState(false);
  const [selfCheckStatus, setSelfCheckStatus] = useState<SelfCheckStatus | null>(null);
  const [selfCheckError, setSelfCheckError] = useState<string | null>(null);
  const [ruleValidationError, setRuleValidationError] = useState<string | null>(null);

  const appendOperationLog = useCallback(
    (message: string, level: LogLevel = "info") => {
      const d = new Date();
      const time = [d.getHours(), d.getMinutes(), d.getSeconds()]
        .map((n) => String(n).padStart(2, "0"))
        .join(":");
      setOperationLogs((prev) =>
        appendCoalescedOperationLog(prev, { time, message, level })
      );
    },
    [],
  );

  const handleAutomationStart = useCallback(() => {
    setOperationLogs([]);
    setOperationLogOpen(true);
  }, []);

  const checkForUpdates = useCallback(async (manual: boolean) => {
    setUpdateChecking(true);
    setUpdateProgressText(null);
    if (manual) {
      setOperationLogOpen(true);
    }
    appendOperationLog("正在检查更新…");
    try {
      const update = await check();
      if (update) {
        setAvailableUpdate(update);
        setOperationLogOpen(true);
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
  }, [appendOperationLog]);

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
    setOperationLogOpen(true);
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
  }, [appendOperationLog]);

  const handleInstallUpdate = useCallback(async () => {
    if (!availableUpdate || updateInstalling) return;
    setUpdateInstalling(true);
    setOperationLogOpen(true);
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
    } catch (err) {
      appendOperationLog(`安装更新失败: ${String(err)}`);
    } finally {
      setUpdateInstalling(false);
    }
  }, [appendOperationLog, availableUpdate, updateInstalling]);

  useEffect(() => {
    const unlistenBattle = listen<AutomationEvent>("automation-status", (event) => {
      appendOperationLog(event.payload.message, event.payload.level ?? "info");
    });
    const unlistenEnhancement = listen<AutomationEvent>(
      "enhancement-automation-status",
      (event) => {
        appendOperationLog(event.payload.message, event.payload.level ?? "info");
      }
    );
    return () => {
      unlistenBattle.then((fn) => fn());
      unlistenEnhancement.then((fn) => fn());
    };
  }, [appendOperationLog]);

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
    const [list, savedActiveProjectId] = await Promise.all([
      invoke<Project[]>("list_projects"),
      invoke<string | null>("get_active_project_id"),
    ]);
    setProjects(list);
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
    return raw.map((s) => ({
      id: s.id,
      type: s.type,
      servant:
        s.servantId != null
          ? (servants.find((sv) => sv.variantKey === s.servantVariantKey) ??
            servants.find((sv) => sv.id === s.servantId) ??
            null)
          : null,
      craftEssence:
        s.craftEssenceId != null
          ? (craftEssences.find((c) => c.id === s.craftEssenceId) ?? null)
          : null,
      craftEssenceMlbRequired: s.craftEssenceMlbRequired ?? true,
    }));
  }, [activeProject, servants, craftEssences]);

  // Persist any slot mutation (drag-reorder or selection from the dialog)
  // back onto the project. ContentGrid still receives a synchronous-looking
  // setter so its DnD/select code stays unchanged.
  const handleSlotsChange = useCallback(
    (next: SlotItem[]) => {
      if (!activeProject) return;
      const projectSlots = next.map((s) => ({
        id: s.id,
        type: s.type,
        servantId: s.servant?.id ?? null,
        servantVariantKey: s.servant?.variantKey ?? null,
        craftEssenceId: s.craftEssence?.id ?? null,
        craftEssenceMlbRequired: s.craftEssenceMlbRequired ?? true,
      }));
      void handleUpdateProject({ ...activeProject, slots: projectSlots });
    },
    [activeProject, handleUpdateProject]
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
  const handleCreateProject = useCallback((name: string, advancedMode = false, grandClass: GrandClass = "saber") => {
    invoke<Project>("create_project", { name, advancedMode, grandClass })
      .then((p) => {
        setProjects((prev) => [...prev, p]);
        setActiveProjectId(p.id);
        persistActiveProjectId(p.id);
      })
      .catch(console.error);
  }, [persistActiveProjectId]);

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
      })
      .catch(console.error);
  }, [persistActiveProjectId]);

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
        })
        .catch(console.error);
    },
    [activeProjectId, persistActiveProjectId]
  );

  const validateCustomRules = useCallback(() => {
    if (!activeProject?.advancedMode) return true;
    const rules = activeProject.grandCardStrategy?.customRules ?? [];
    if (rules.length === 0) return true;
    const currentIds = new Set(
      partyMembers
        .map((member) => member.servant?.id ?? null)
        .filter((id): id is number => id != null)
    );
    const missing: string[] = [];
    for (const [ruleIndex, rule] of rules.entries()) {
      for (const [slotIndex, slot] of (rule.slots ?? []).entries()) {
        if (slot.grandServant === true) {
          continue;
        }
        if (slot.servantId != null && !currentIds.has(slot.servantId)) {
          missing.push(`${rule.name || `规则 ${ruleIndex + 1}`} 第 ${slotIndex + 1} 张`);
        }
      }
    }
    if (missing.length === 0) return true;
    setRuleValidationError(`以下出牌规则引用的从者不在当前队伍中，请修复后继续：${missing.join("、")}`);
    return false;
  }, [activeProject, partyMembers]);

  const handleStartRun = useCallback(() => {
    if (!validateCustomRules()) return;
    setView("battle");
  }, [validateCustomRules]);

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

  const handleOpenSettings = useCallback(() => {
    setSettingsSection("dataManagement");
    setSettingsOpen(true);
  }, []);

  // After the runner exits we return to the team page (the start of
  // the linear flow) rather than to "config", which no longer exists.
  const handleBackToConfig = useCallback(() => {
    setView("team");
  }, []);

  const handleGotoCommand = useCallback(() => {
    if (!validateCustomRules()) return;
    setView("command");
  }, [validateCustomRules]);

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
              servants={servants}
              activeProjectId={activeProjectId}
              onProjectSelect={handleProjectSelect}
              onCreateProject={handleCreateProject}
              onRenameProject={handleRenameProject}
              onDuplicateProject={handleDuplicateProject}
              onDeleteProject={handleDeleteProject}
              onUpdateProject={handleUpdateProject}
              onBack={handleBackToConfig}
              onAutomationStart={handleAutomationStart}
              onLogEntry={appendOperationLog}
            />
          ) : view === "enhancement" && featureToggles.servantEnhancement ? (
            <EnhancementPage
              servants={servants}
              onBack={handleBackToConfig}
              onAutomationStart={handleAutomationStart}
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
                activeProjectId={activeProjectId}
                onProjectSelect={handleProjectSelect}
                onCreateProject={handleCreateProject}
                onRenameProject={handleRenameProject}
                onDuplicateProject={handleDuplicateProject}
                onDeleteProject={handleDeleteProject}
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
                    grandServants={activeProject?.grandServants ?? []}
                    grandClass={activeProject?.grandClass ?? "saber"}
                    grandCardStrategy={activeProject?.grandCardStrategy}
                    grandCardPriorityEnabled
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
                        onUpdateActiveProject={handleUpdateProject}
                      />
                    </Box>
                  </Box>
                  <Flex justify="between" align="center" className="page-footer" gap="3">
                    <Box />
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
        operationLogOpen={operationLogOpen}
        onOperationLogOpenChange={setOperationLogOpen}
        updateAvailable={availableUpdate != null}
        updateChecking={updateChecking}
        updateInstalling={updateInstalling}
        updateProgressText={updateProgressText}
        onInstallUpdate={handleInstallUpdate}
      />
      <SettingsDialog
        open={settingsOpen}
        section={settingsSection}
        onOpenChange={setSettingsOpen}
        onSectionChange={setSettingsSection}
        onProjectsImported={handleProjectsImported}
      />
      <SelfCheckDialog
        open={selfCheckOpen}
        loading={selfCheckLoading}
        status={selfCheckStatus}
        error={selfCheckError}
        onOpenChange={setSelfCheckOpen}
      />
      <AlertDialog.Root
        open={ruleValidationError != null}
        onOpenChange={(open) => {
          if (!open) setRuleValidationError(null);
        }}
      >
        <AlertDialog.Content maxWidth="420px">
          <AlertDialog.Title>需要修复出牌规则</AlertDialog.Title>
          <AlertDialog.Description>
            {ruleValidationError}
          </AlertDialog.Description>
          <Flex justify="end" mt="4">
            <AlertDialog.Action>
              <Button type="button">知道了</Button>
            </AlertDialog.Action>
          </Flex>
        </AlertDialog.Content>
      </AlertDialog.Root>
    </Flex>
  );
}

export default App;
