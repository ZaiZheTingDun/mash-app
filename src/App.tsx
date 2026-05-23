import { useState, useEffect, useMemo, useCallback } from "react";
import { Box, Button, Flex, Text, Spinner } from "@radix-ui/themes";
import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
import { invoke, listen } from "./tauri";
import { ArrowLeftIcon } from "@radix-ui/react-icons";
import { ContentGrid } from "./components/ContentGrid";
import { derivePartyLineup, derivePartyServants } from "./components/partyServants";
import { CommandEditor } from "./components/CommandEditor";
import { BattlePage } from "./components/BattlePage";
import { EnhancementPage } from "./components/EnhancementPage";
import { DebugPage } from "./components/DebugPage";
import { StatusBar } from "./components/StatusBar";
import { ProjectBar } from "./components/ProjectBar";
import { SetupPage } from "./components/SetupPage";
import { createInitialProjectSlots } from "./components/projectSlots";
import { featureToggles } from "./featureToggles";
import type { SlotItem } from "./components/ContentGrid";
import type { Servant } from "./types/servant";
import type { CraftEssence } from "./types/craftEssence";
import type { Project } from "./types/project";
import type { AppTheme } from "./types/theme";
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
}

export interface OperationLogEntry {
  time: string;
  message: string;
}

interface AppProps {
  theme: AppTheme;
  onThemeChange: (theme: AppTheme) => void;
}

function localDateKey(date: Date): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function App({ theme, onThemeChange }: AppProps) {
  const [view, setView] = useState<View>("team");
  const [activeProjectId, setActiveProjectId] = useState<string | null>(null);
  const [projects, setProjects] = useState<Project[]>([]);
  const [servants, setServants] = useState<Servant[]>([]);
  const [craftEssences, setCraftEssences] = useState<CraftEssence[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [setupReady, setSetupReady] = useState(false);
  const [operationLogs, setOperationLogs] = useState<OperationLogEntry[]>([]);
  const [operationLogOpen, setOperationLogOpen] = useState(false);
  const [availableUpdate, setAvailableUpdate] = useState<Update | null>(null);
  const [updateChecking, setUpdateChecking] = useState(false);
  const [updateInstalling, setUpdateInstalling] = useState(false);
  const [updateProgressText, setUpdateProgressText] = useState<string | null>(null);

  const appendOperationLog = useCallback((message: string) => {
    const d = new Date();
    const time = [d.getHours(), d.getMinutes(), d.getSeconds()]
      .map((n) => String(n).padStart(2, "0"))
      .join(":");
    setOperationLogs((prev) => [...prev, { time, message }]);
  }, []);

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
      appendOperationLog(event.payload.message);
    });
    const unlistenEnhancement = listen<AutomationEvent>(
      "enhancement-automation-status",
      (event) => {
        appendOperationLog(event.payload.message);
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

    return () => {
      cancelled = true;
      unlistenMenu.then((fn) => fn());
    };
  }, [checkForUpdates]);

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

  // Load the project list once at startup so the team-builder can read/write
  // `supportServantId` directly off the active project. Sidebar still owns
  // the create/delete UI, but it now mutates this lifted state instead of
  // its own local copy so the support slot stays in sync.
  useEffect(() => {
    invoke<Project[]>("list_projects")
      .then((list) => {
        setProjects(list);
        if (list.length > 0) {
          setActiveProjectId((prev) => prev ?? list[0].id);
        }
      })
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
  const handleCreateProject = useCallback((name: string, advancedMode = false) => {
    invoke<Project>("create_project", { name, advancedMode })
      .then((p) => {
        setProjects((prev) => [...prev, p]);
        setActiveProjectId(p.id);
      })
      .catch(console.error);
  }, []);

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
      })
      .catch(console.error);
  }, []);

  const handleDeleteProject = useCallback(
    (id: string) => {
      invoke("delete_project", { id })
        .then(() => {
          setProjects((prev) => {
            const next = prev.filter((p) => p.id !== id);
            if (activeProjectId === id) {
              setActiveProjectId(next.length > 0 ? next[0].id : null);
            }
            return next;
          });
        })
        .catch(console.error);
    },
    [activeProjectId]
  );

  const handleStartRun = useCallback(() => {
    setView("battle");
  }, []);

  const handleOpenDebug = useCallback(() => {
    if (!featureToggles.cvDebug) return;
    setView("debug");
  }, []);

  const handleOpenEnhancement = useCallback(() => {
    if (!featureToggles.servantEnhancement) return;
    setView("enhancement");
  }, []);

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

  if (!setupReady) {
    return (
      <Flex direction="column" className="app-root" data-theme={theme}>
        <SetupPage onReady={() => setSetupReady(true)} />
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
              activeProjectId={activeProjectId}
              onProjectSelect={setActiveProjectId}
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
                onProjectSelect={setActiveProjectId}
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
                    advancedMode={activeProject?.advancedMode === true}
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
        onOpenDebug={featureToggles.cvDebug ? handleOpenDebug : undefined}
        theme={theme}
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
    </Flex>
  );
}

export default App;
