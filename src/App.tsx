import { useState, useEffect, useMemo, useCallback } from "react";
import { Box, Flex, Text, Spinner } from "@radix-ui/themes";
import { invoke } from "./tauri";
import { ArrowLeftIcon } from "@radix-ui/react-icons";
import { ContentGrid } from "./components/ContentGrid";
import { derivePartyLineup, derivePartyServants } from "./components/partyServants";
import { CommandEditor } from "./components/CommandEditor";
import { BattlePage } from "./components/BattlePage";
import { EnhancementPage } from "./components/EnhancementPage";
import { DebugPage } from "./components/DebugPage";
import { StatusBar } from "./components/StatusBar";
import { ProjectBar } from "./components/ProjectBar";
import { AssetBundleButton } from "./components/AssetBundleButton";
import { createInitialProjectSlots } from "./components/projectSlots";
import type { SlotItem } from "./components/ContentGrid";
import type { Servant } from "./types/servant";
import type { CraftEssence } from "./types/craftEssence";
import type { Project } from "./types/project";
import "./App.css";

// Linear flow: 队伍设置 → 指令设置 → 开始任务. Each forward step is
// triggered by the bottom-right primary button on the previous page;
// `debug` is reached out-of-band from the sidebar. Replaces the older
// horizontal `StageNavigator` (queue/support/command tabs).
type View = "team" | "command" | "battle" | "enhancement" | "debug";

function App() {
  const [view, setView] = useState<View>("team");
  const [activeProjectId, setActiveProjectId] = useState<string | null>(null);
  const [projects, setProjects] = useState<Project[]>([]);
  const [servants, setServants] = useState<Servant[]>([]);
  const [craftEssences, setCraftEssences] = useState<CraftEssence[]>([]);
  const [assetVersion, setAssetVersion] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

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
  const handleCreateProject = useCallback(() => {
    invoke<Project>("create_project", { name: `Project ${projects.length + 1}` })
      .then((p) => {
        setProjects((prev) => [...prev, p]);
        setActiveProjectId(p.id);
      })
      .catch(console.error);
  }, [projects.length]);

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
    setView("debug");
  }, []);

  const handleOpenEnhancement = useCallback(() => {
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

  return (
    <Flex direction="column" className="app-root">
      <Flex className="app-container">
        <Box className="main-content">
          {view === "battle" ? (
            <BattlePage
              defaultProjectId={activeProjectId}
              onBack={handleBackToConfig}
            />
          ) : view === "enhancement" ? (
            <EnhancementPage servants={servants} onBack={handleBackToConfig} />
          ) : view === "debug" ? (
            <DebugPage
              onBack={handleBackToConfig}
              defaultCardServantIds={partyServantIds}
            />
          ) : (
            <Box className="main-content-inner">
              <ProjectBar
                projects={projects}
                activeProjectId={activeProjectId}
                onProjectSelect={setActiveProjectId}
                onCreateProject={handleCreateProject}
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
                  />
                  <Flex justify="between" align="center" className="page-footer">
                    <button
                      type="button"
                      className="page-back-btn"
                      onClick={handleBackToTeam}
                    >
                      <ArrowLeftIcon width={14} height={14} />
                      <Text size="2">队伍设置</Text>
                    </button>
                    <button
                      type="button"
                      className="page-next-btn"
                      onClick={handleStartRun}
                    >
                      <Text size="2" weight="bold">
                        开始任务
                      </Text>
                    </button>
                  </Flex>
                </>
              ) : (
                <>
                  <ContentGrid
                    key={`assets-${assetVersion}`}
                    servants={servants}
                    craftEssences={craftEssences}
                    slots={slots}
                    onSlotsChange={handleSlotsChange}
                    activeProject={activeProject}
                    onUpdateActiveProject={handleUpdateProject}
                  />
                  <Flex justify="between" align="center" className="page-footer" gap="3">
                    <AssetBundleButton
                      onImported={() => setAssetVersion((prev) => prev + 1)}
                    />
                    <Flex align="center">
                    <button
                      type="button"
                      className="page-secondary-btn"
                      onClick={handleOpenEnhancement}
                    >
                      <Text size="2" weight="medium">
                        强化从者
                      </Text>
                    </button>
                    <button
                      type="button"
                      className="page-next-btn"
                      onClick={handleGotoCommand}
                    >
                      <Text size="2" weight="bold">
                        指令设置
                      </Text>
                    </button>
                    </Flex>
                  </Flex>
                </>
              )}
            </Box>
          )}
        </Box>
      </Flex>
      <StatusBar onOpenDebug={handleOpenDebug} />
    </Flex>
  );
}

export default App;
