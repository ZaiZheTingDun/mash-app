import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  AlertDialog,
  Box,
  Button,
  CheckboxCards,
  Flex,
  RadioGroup,
  Text,
} from "@radix-ui/themes";
import { invoke, listen } from "../tauri";
import { ChevronLeftIcon, MinusIcon, PlusIcon } from "@radix-ui/react-icons";
import { ProjectBar } from "./ProjectBar";
import goldFruitImage from "../../src-tauri/resources/images/item_fruit_golden.png";
import silverFruitImage from "../../src-tauri/resources/images/item_fruit_silver.png";
import bronzeFruitImage from "../../src-tauri/resources/images/item_fruit_bronzed_cobalt.png";
import copperFruitImage from "../../src-tauri/resources/images/item_fruit_bronze.png";
import saintQuartzImage from "../../src-tauri/resources/images/item_saint_quartz.png";
import type {
  BattleApRecoveryItem,
  BattleRepeatMode,
  Project,
} from "../types/project";

interface AutomationEvent {
  state: string;
  currentScreen: string;
  message: string;
}

const AP_RECOVERY_OPTIONS: {
  value: BattleApRecoveryItem;
  label: string;
  recoveryLabel: string;
  imageSrc: string;
}[] = [
    {
      value: "gold",
      label: "黄金果实",
      recoveryLabel: "+100% 行动力",
      imageSrc: goldFruitImage,
    },
    {
      value: "silver",
      label: "白银果实",
      recoveryLabel: "+50% 行动力",
      imageSrc: silverFruitImage,
    },
    {
      value: "bronze",
      label: "青铜果实",
      recoveryLabel: "+40 行动力",
      imageSrc: bronzeFruitImage,
    },
    {
      value: "copper",
      label: "赤铜果实",
      recoveryLabel: "+10 行动力",
      imageSrc: copperFruitImage,
    },
    {
      value: "rainbow",
      label: "圣晶石",
      recoveryLabel: "+100% 行动力",
      imageSrc: saintQuartzImage,
    },
  ];
const EMPTY_AP_RECOVERY_ITEMS: BattleApRecoveryItem[] = [];

interface BattlePageProps {
  projects: Project[];
  activeProjectId: string | null;
  onProjectSelect: (id: string) => void;
  onCreateProject: (name: string, advancedMode?: boolean) => void;
  onRenameProject: (id: string, name: string) => void;
  onDuplicateProject: (id: string, name: string) => void;
  onDeleteProject: (id: string) => void;
  onUpdateProject: (project: Project) => Promise<void>;
  onBack: () => void;
  onAutomationStart?: () => void;
  onLogEntry?: (message: string) => void;
}

interface BattleProjectDraft {
  projectId: string;
  repeatMode: BattleRepeatMode;
  repeatCount: number | null;
  apRecoveryItems: BattleApRecoveryItem[];
}

function projectRepeatMode(project: Project | null | undefined): BattleRepeatMode {
  if (project?.repeatMode === "infinite" || project?.repeatMode === "count") {
    return project.repeatMode;
  }
  if (project?.repeatMission) {
    return "infinite";
  }
  return "single";
}

function projectRepeatCount(project: Project | null | undefined): number | null {
  const count = project?.repeatCount;
  return typeof count === "number" && Number.isInteger(count) && count > 0 ? count : null;
}

function orderedApRecoveryItems(project: Project | null | undefined): BattleApRecoveryItem[] {
  const configured = project?.apRecoveryItems ?? [];
  return AP_RECOVERY_OPTIONS.map((option) => option.value).filter((value) =>
    configured.includes(value)
  );
}

function projectDraftFromProject(project: Project): BattleProjectDraft {
  return {
    projectId: project.id,
    repeatMode: projectRepeatMode(project),
    repeatCount: projectRepeatCount(project),
    apRecoveryItems: orderedApRecoveryItems(project),
  };
}

export function BattlePage({
  projects,
  activeProjectId,
  onProjectSelect,
  onCreateProject,
  onRenameProject,
  onDuplicateProject,
  onDeleteProject,
  onUpdateProject,
  onBack,
  onAutomationStart,
  onLogEntry,
}: BattlePageProps) {
  const [running, setRunning] = useState(false);
  const [rainbowConfirmOpen, setRainbowConfirmOpen] = useState(false);
  const [stopAfterCurrentRequested, setStopAfterCurrentRequested] = useState(false);
  const [draft, setDraft] = useState<BattleProjectDraft | null>(null);
  const draftRef = useRef<BattleProjectDraft | null>(null);

  useEffect(() => {
    const unlisten = listen<AutomationEvent>("automation-status", (event) => {
      const { state } = event.payload;

      if (state.includes("Idle") || state.includes("Finished") || state.includes("Error")) {
        setRunning(false);
        setStopAfterCurrentRequested(false);
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const selectedProject = projects.find((project) => project.id === activeProjectId) ?? null;

  const projectDraft = useMemo(
    () => (selectedProject ? projectDraftFromProject(selectedProject) : null),
    [selectedProject]
  );
  const activeDraft =
    selectedProject && draft?.projectId === selectedProject.id
      ? draft
      : projectDraft;
  const repeatMode = activeDraft?.repeatMode ?? "single";
  const repeatCount = activeDraft?.repeatCount ?? null;
  const apRecoveryItems = activeDraft?.apRecoveryItems ?? EMPTY_AP_RECOVERY_ITEMS;

  const saveProject = useCallback(
    (nextProject: Project) => {
      onUpdateProject(nextProject).catch(console.error);
    },
    [onUpdateProject]
  );

  const updateSelectedProject = useCallback(
    (updater: (project: Project) => Project) => {
      if (!selectedProject) return;
      const nextProject = updater(selectedProject);
      const nextDraft = projectDraftFromProject(nextProject);
      draftRef.current = nextDraft;
      setDraft(nextDraft);
      saveProject(nextProject);
    },
    [saveProject, selectedProject]
  );

  const startAutomation = useCallback(() => {
    if (!selectedProject) return;
    const latestDraft =
      draftRef.current?.projectId === selectedProject.id
        ? draftRef.current
        : projectDraftFromProject(selectedProject);
    setRunning(true);
    setStopAfterCurrentRequested(false);
    onAutomationStart?.();

    const supportSlot = selectedProject.slots?.find((slot) => slot.type === "support");
    const servantSelections =
      selectedProject.slots
        ?.map((slot, slotIndex) =>
          slot.type === "servant" && slot.servantId != null
            ? { slotIndex, servantId: slot.servantId }
            : null
        )
        .filter((selection): selection is { slotIndex: number; servantId: number } =>
          selection != null
        ) ?? [];
    const maxMissionRuns =
      latestDraft.repeatMode === "count" ? latestDraft.repeatCount : null;
    const config = {
      projectId: selectedProject.id,
      partyOrder: null,
      supportClassFilter: null,
      supportServantName: null,
      supportServantId: selectedProject.supportServantId ?? null,
      supportSlotIndex:
        supportSlot != null ? selectedProject.slots?.indexOf(supportSlot) ?? null : null,
      supportCraftEssenceId: supportSlot?.craftEssenceId ?? null,
      supportNoblePhantasmLevelMin:
        selectedProject.supportNoblePhantasmLevelMin ?? null,
      supportSkillLevelMins:
        selectedProject.supportSkillLevelMins ?? [null, null, null],
      supportAppendSkillLevelMins:
        selectedProject.supportAppendSkillLevelMins ?? [null, null, null, null, null],
      servantSelections,
      maxSupportScrolls: 3,
      repeatMission: latestDraft.repeatMode === "infinite",
      maxMissionRuns,
      apRecoveryItems: latestDraft.apRecoveryItems,
    };

    invoke("start_automation", { config }).catch((err) => {
      onLogEntry?.(`启动失败: ${String(err)}`);
      setRunning(false);
      setStopAfterCurrentRequested(false);
    });
  }, [
    onAutomationStart,
    onLogEntry,
    selectedProject,
  ]);

  const handleStart = useCallback(() => {
    if (!selectedProject) return;
    if (apRecoveryItems.includes("rainbow")) {
      setRainbowConfirmOpen(true);
      return;
    }
    startAutomation();
  }, [apRecoveryItems, selectedProject, startAutomation]);

  const handleStop = useCallback(() => {
    invoke("stop_automation").catch(console.error);
  }, []);

  const handleStopAfterCurrent = useCallback(() => {
    invoke("stop_automation_after_current")
      .then(() => {
        setStopAfterCurrentRequested(true);
        onLogEntry?.("已设置：运行完当前轮次后停止");
      })
      .catch(console.error);
  }, [onLogEntry]);

  const handleRepeatModeChange = useCallback(
    (nextMode: BattleRepeatMode) => {
      if (!selectedProject || running) return;
      if (nextMode === "count") {
        updateSelectedProject((project) => ({
          ...project,
          repeatMission: true,
          repeatMode: "count",
          repeatCount: repeatCount ?? 1,
        }));
        return;
      }
      updateSelectedProject((project) => ({
        ...project,
        repeatMission: nextMode !== "single",
        repeatMode: nextMode,
        repeatCount: null,
      }));
    },
    [repeatCount, running, selectedProject, updateSelectedProject]
  );

  const setRepeatCountValue = useCallback(
    (nextCount: number) => {
      if (!selectedProject || running) return;
      const normalized = Math.max(1, Math.floor(nextCount));
      updateSelectedProject((project) => ({
        ...project,
        repeatMission: true,
        repeatMode: "count",
        repeatCount: normalized,
      }));
    },
    [running, selectedProject, updateSelectedProject]
  );

  const handleRepeatCountStep = useCallback(
    (delta: number) => {
      setRepeatCountValue((repeatCount ?? 1) + delta);
    },
    [repeatCount, setRepeatCountValue]
  );

  const handleRepeatCountInput = useCallback(
    (value: string) => {
      if (!value.trim()) return;
      const parsed = Number(value);
      if (!Number.isInteger(parsed) || parsed <= 0) return;
      setRepeatCountValue(parsed);
    },
    [setRepeatCountValue]
  );

  const enableRepeatCount = useCallback(() => {
    if (!selectedProject || running) return;
    updateSelectedProject((project) => ({
      ...project,
      repeatMission: true,
      repeatMode: "count",
      repeatCount: repeatCount ?? 1,
    }));
  }, [repeatCount, running, selectedProject, updateSelectedProject]);

  const setApRecoveryItems = useCallback(
    (items: BattleApRecoveryItem[]) => {
      updateSelectedProject((project) => ({
        ...project,
        apRecoveryItems: AP_RECOVERY_OPTIONS.map((option) => option.value).filter((value) =>
          items.includes(value)
        ),
      }));
    },
    [updateSelectedProject]
  );

  const displayedRepeatCount = repeatCount ?? 1;

  return (
    <Flex direction="column" className="battle-page">
      <Box className="battle-topbar">
        <ProjectBar
          projects={projects}
          activeProjectId={activeProjectId}
          disabled={running}
          onProjectSelect={onProjectSelect}
          onCreateProject={onCreateProject}
          onRenameProject={onRenameProject}
          onDuplicateProject={onDuplicateProject}
          onDeleteProject={onDeleteProject}
        />
      </Box>

      <Flex direction="column" className="battle-body battle-body-scroll">
        <Box className="battle-panel">
          <Box className="battle-section-heading">
            <Text size="4" weight="bold">
              重复任务
            </Text>
          </Box>
          <RadioGroup.Root
            value={repeatMode}
            className="battle-repeat-cards"
            disabled={running || !selectedProject}
            onValueChange={(value) => handleRepeatModeChange(value as BattleRepeatMode)}
          >
            <div
              aria-disabled={running || !selectedProject}
              aria-pressed={repeatMode === "single"}
              className={`battle-repeat-card ${repeatMode === "single" ? "is-selected" : ""}`}
              onClick={() => handleRepeatModeChange("single")}
            >
              <RadioGroup.Item value="single" className="battle-repeat-radio" />
              <div>
                <Text size="3" weight="bold">不重复</Text>
                <Text> </Text>
                <Text size="2" color="gray">任务执行完毕后停止</Text>
              </div>
            </div>

            <div
              aria-disabled={running || !selectedProject}
              aria-pressed={repeatMode === "infinite"}
              className={`battle-repeat-card ${repeatMode === "infinite" ? "is-selected" : ""}`}
              onClick={() => handleRepeatModeChange("infinite")}
            >
              <RadioGroup.Item value="infinite" className="battle-repeat-radio" />
              <div>
                <Text size="3" weight="bold">无限</Text>
                <Text> </Text>
                <Text size="2" color="gray">无限重复执行任务</Text>
              </div>
              <span className="battle-repeat-symbol">∞</span>
            </div>

            <div
              aria-disabled={running || !selectedProject}
              aria-pressed={repeatMode === "count"}
              className={`battle-repeat-card battle-repeat-card-count ${repeatMode === "count" ? "is-selected" : ""
                }`}
              onClick={enableRepeatCount}
            >
              <RadioGroup.Item value="count" className="battle-repeat-radio" />
              <div>
                <Text size="3" weight="bold">设置次数</Text>
              </div>
              <div className="battle-repeat-counter" aria-label="重复次数">
                <Button
                  color="indigo"
                  disabled={running || !selectedProject || displayedRepeatCount <= 1}
                  aria-label="减少重复次数"
                  onClick={(event) => {
                    event.stopPropagation();
                    handleRepeatCountStep(-1);
                  }}
                >
                  <MinusIcon width={15} height={15} />
                </Button>
                <input
                  className="battle-counter-value"
                  type="number"
                  min="1"
                  step="1"
                  inputMode="numeric"
                  aria-label="重复次数"
                  value={displayedRepeatCount}
                  disabled={running || !selectedProject}
                  onClick={(event) => event.stopPropagation()}
                  onChange={(event) => handleRepeatCountInput(event.target.value)}
                />
                <Button
                  disabled={running || !selectedProject}
                  color="indigo"
                  aria-label="增加重复次数"
                  onClick={(event) => {
                    event.stopPropagation();
                    handleRepeatCountStep(1);
                  }}
                >
                  <PlusIcon width={15} height={15} />
                </Button>
              </div>
            </div>
          </RadioGroup.Root>
        </Box>

        <Box className="battle-panel">
          <Box className="battle-section-heading">
            <Text size="4" weight="bold">
              行动力恢复
            </Text>
          </Box>
          <CheckboxCards.Root
            value={apRecoveryItems}
            className="battle-recovery-cards"
            disabled={running || !selectedProject}
            onValueChange={(items) => setApRecoveryItems(items as BattleApRecoveryItem[])}
          >
            {AP_RECOVERY_OPTIONS.map((option) => {
              const checked = apRecoveryItems.includes(option.value);
              return (
                <CheckboxCards.Item
                  key={option.value}
                  value={option.value}
                  className={`battle-recovery-card ${checked ? "is-selected" : ""}`}
                >
                  <img src={option.imageSrc} alt="" className="battle-recovery-image" />
                  <Text size="3" weight="bold">{option.label}</Text>
                  <Text size="2" weight="bold" className="battle-recovery-amount">
                    {option.recoveryLabel}
                  </Text>
                </CheckboxCards.Item>
              );
            })}
          </CheckboxCards.Root>
        </Box>
      </Flex>

      <Flex justify="between" align="center" className="battle-footer" gap="3">
        <Flex align="center" gap="2">
          <Button type="button" variant="soft" color="gray" disabled={running} onClick={onBack}>
            <ChevronLeftIcon width={16} height={16} />
            <Text size="2">返回</Text>
          </Button>
        </Flex>
        <Flex align="center" gap="3" wrap="wrap" justify="end">
          <Button
            color="red"
            variant="soft"
            disabled={!running || stopAfterCurrentRequested}
            onClick={handleStopAfterCurrent}
          >
            运行完当前轮次后停止
          </Button>
          <Button color="red" variant="soft" disabled={!running} onClick={handleStop}>
            停止
          </Button>
          <Button disabled={running || !selectedProject} onClick={handleStart}>
            开始
          </Button>
        </Flex>
      </Flex>

      <AlertDialog.Root open={rainbowConfirmOpen} onOpenChange={setRainbowConfirmOpen}>
        <AlertDialog.Content maxWidth="420px">
          <AlertDialog.Title>确认开始任务</AlertDialog.Title>
          <AlertDialog.Description size="2">
            当前已勾选圣晶石。开始后若行动力不足，自动化可能会消耗圣晶石补充行动力。确认继续开始吗？
          </AlertDialog.Description>
          <Flex justify="end" gap="3" mt="4">
            <AlertDialog.Cancel>
              <Button type="button" variant="soft" color="gray">
                取消
              </Button>
            </AlertDialog.Cancel>
            <AlertDialog.Action>
              <Button
                type="button"
                color="red"
                onClick={() => {
                  setRainbowConfirmOpen(false);
                  startAutomation();
                }}
              >
                确认开始
              </Button>
            </AlertDialog.Action>
          </Flex>
        </AlertDialog.Content>
      </AlertDialog.Root>
    </Flex>
  );
}
