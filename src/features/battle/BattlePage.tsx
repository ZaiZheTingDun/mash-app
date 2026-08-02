import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  AlertDialog,
  Box,
  Button,
  CheckboxCards,
  Dialog,
  Flex,
  IconButton,
  Switch,
  Text,
  TextField,
} from "@radix-ui/themes";
import { invoke, listen } from "../../tauri";
import {
  ChevronLeftIcon,
  Cross1Icon,
  GearIcon,
  MinusIcon,
  PlusIcon,
} from "@radix-ui/react-icons";
import { ProjectBar } from "../projects/ProjectBar";
import { OptionCardRadioGroup } from "../../components/common/OptionCardRadioGroup";
import { SectionHeading } from "../../components/common/SectionHeading";
import goldFruitImage from "../../../src-tauri/resources/images/item_fruit_golden.png";
import silverFruitImage from "../../../src-tauri/resources/images/item_fruit_silver.png";
import bronzeFruitImage from "../../../src-tauri/resources/images/item_fruit_bronzed_cobalt.png";
import copperFruitImage from "../../../src-tauri/resources/images/item_fruit_bronze.png";
import saintQuartzImage from "../../../src-tauri/resources/images/item_saint_quartz.png";
import type {
  BattleApRecoveryItem,
  BattleRepeatMode,
  GrandClass,
  GrandClassDefinition,
  GrandChainPriorityItem,
  Project,
} from "../../types/project";
import type { Servant } from "../../types/servant";
import {
  grandClassDefinition,
  validateGrandServants as grandServantsAreValid,
} from "../advanced/grandClassModel";
import { isAutomationTerminal, type AutomationStatus } from "../../types/automation";

interface AutomationEvent {
  state: string;
  status: AutomationStatus;
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
const DEFAULT_GRAND_CHAIN_PRIORITY: GrandChainPriorityItem[] = [
  "mainBraveChain",
  "mainReadyNp",
  "deputyBraveChain",
  "mainColorChain",
  "deputyColorChain",
  "fallback",
];
const DEFAULT_FIVE_STAR_CE_DROP_TARGET_COUNT = 1;

interface BattlePageProps {
  projects: Project[];
  grandClassDefinitions: GrandClassDefinition[];
  servants: Servant[];
  activeProjectId: string | null;
  onProjectSelect: (id: string) => void;
  onCreateProject: (name: string, advancedMode?: boolean, grandClass?: GrandClass) => void;
  onRenameProject: (id: string, name: string) => void;
  onDuplicateProject: (id: string, name: string) => void;
  onDeleteProject: (id: string) => void;
  onOpenProjectSettings: () => void;
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

function normalizeFiveStarCeDropTargetCount(value: unknown) {
  return typeof value === "number" && Number.isInteger(value) && value > 0
    ? value
    : DEFAULT_FIVE_STAR_CE_DROP_TARGET_COUNT;
}

function projectStopsOnFiveStarCeDrop(project: Project | null | undefined) {
  return project?.recognitionSettings?.stopOnFiveStarCeDrop === true;
}

function projectFiveStarCeDropTargetCount(project: Project | null | undefined) {
  return normalizeFiveStarCeDropTargetCount(
    project?.recognitionSettings?.fiveStarCeDropTargetCount
  );
}

function recognitionSettingsFiveStarCeDropTargetCount(
  settings: Project["recognitionSettings"]
) {
  return normalizeFiveStarCeDropTargetCount(settings?.fiveStarCeDropTargetCount);
}

interface BattleAdvancedSettingsDialogProps {
  open: boolean;
  project: Project | null;
  disabled: boolean;
  onOpenChange: (open: boolean) => void;
  onUpdateProject: (updater: (project: Project) => Project) => void;
}

function BattleAdvancedSettingsDialog({
  open,
  project,
  disabled,
  onOpenChange,
  onUpdateProject,
}: BattleAdvancedSettingsDialogProps) {
  const stopOnFiveStarCeDrop = projectStopsOnFiveStarCeDrop(project);
  const targetCount = projectFiveStarCeDropTargetCount(project);

  const updateRecognitionSettings = useCallback(
    (
      updater: (
        settings: NonNullable<Project["recognitionSettings"]>
      ) => Project["recognitionSettings"]
    ) => {
      if (!project || disabled) return;
      onUpdateProject((current) => ({
        ...current,
        recognitionSettings: updater(current.recognitionSettings ?? {}),
      }));
    },
    [disabled, onUpdateProject, project]
  );

  const setStopOnFiveStarCeDrop = useCallback(
    (value: boolean) => {
      updateRecognitionSettings((settings) => ({
        ...settings,
        stopOnFiveStarCeDrop: value,
        fiveStarCeDropTargetCount: recognitionSettingsFiveStarCeDropTargetCount(settings),
      }));
    },
    [updateRecognitionSettings]
  );

  const setTargetCount = useCallback(
    (value: number) => {
      const nextValue = normalizeFiveStarCeDropTargetCount(value);
      updateRecognitionSettings((settings) => ({
        ...settings,
        fiveStarCeDropTargetCount: nextValue,
      }));
    },
    [updateRecognitionSettings]
  );

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Content className="settings-dialog">
        <Flex className="settings-shell">
          <Flex asChild direction="column" className="settings-nav">
            <nav aria-label="高级设置导航">
              <Box className="settings-nav-title">
                <Dialog.Title size="4">高级设置</Dialog.Title>
              </Box>
              <Flex direction="column" gap="4">
                <Flex direction="column" gap="3" className="settings-nav-group">
                  <Text size="1" weight="bold" color="gray" className="settings-nav-group-label">
                    战斗
                  </Text>
                  <Button
                    type="button"
                    variant="ghost"
                    color="gray"
                    data-active="true"
                    aria-current="page"
                    className="settings-nav-button"
                  >
                    <GearIcon width={15} height={15} />
                    <Text size="2" weight="medium">
                      战利品掉落
                    </Text>
                  </Button>
                </Flex>
              </Flex>
            </nav>
          </Flex>

          <Flex direction="column" className="settings-content">
            <Flex align="center" className="settings-content-header">
              <Flex align="center" className="settings-content-header-inner">
                <Text size="5" weight="bold">
                  战利品掉落
                </Text>
              </Flex>
              <Dialog.Close>
                <IconButton type="button" variant="ghost" color="gray" aria-label="关闭高级设置">
                  <Cross1Icon width={15} height={15} />
                </IconButton>
              </Dialog.Close>
            </Flex>

            <Box className="settings-content-scroll">
              <Box className="settings-content-body">
                <Box className="settings-section-panel">
                  <Flex direction="column" gap="4" className="recognition-setting-block">
                    <Flex
                      align="start"
                      justify="between"
                      gap="4"
                      wrap="wrap"
                      className="basic-setting-row"
                    >
                      <Flex direction="column" gap="1" className="basic-setting-copy">
                        <Text size="2" weight="bold">
                          五星礼装掉落自动停止
                        </Text>
                        <Text size="1" color="gray">
                          开启后在战利品结算页检测前两行，累计达到目标数量后停止
                        </Text>
                      </Flex>

                      <Switch
                        checked={stopOnFiveStarCeDrop}
                        onCheckedChange={setStopOnFiveStarCeDrop}
                        disabled={disabled || !project}
                        aria-label="五星礼装掉落自动停止"
                      />
                    </Flex>

                    {stopOnFiveStarCeDrop && (
                      <Flex
                        align="center"
                        justify="between"
                        gap="4"
                        wrap="wrap"
                        className="basic-setting-row"
                      >
                        <Flex direction="column" gap="1" className="basic-setting-copy">
                          <Text size="2" weight="bold">
                            掉落个数
                          </Text>
                          <Text size="1" color="gray">
                            本次自动化运行内累计计算，默认 1
                          </Text>
                        </Flex>

                        <Flex align="center" gap="2" className="battle-repeat-counter">
                          <Button
                            type="button"
                            color="indigo"
                            disabled={disabled || !project || targetCount <= 1}
                            aria-label="减少五星礼装掉落个数"
                            onClick={() => setTargetCount(targetCount - 1)}
                          >
                            <MinusIcon width={15} height={15} />
                          </Button>
                          <TextField.Root
                            className="battle-counter-value"
                            type="number"
                            min="1"
                            step="1"
                            variant="soft"
                            radius="none"
                            inputMode="numeric"
                            aria-label="五星礼装掉落个数"
                            value={targetCount}
                            disabled={disabled || !project}
                            onChange={(event) => {
                              const parsed = Number(event.currentTarget.value);
                              if (Number.isInteger(parsed) && parsed > 0) {
                                setTargetCount(parsed);
                              }
                            }}
                          />
                          <Button
                            type="button"
                            disabled={disabled || !project}
                            color="indigo"
                            aria-label="增加五星礼装掉落个数"
                            onClick={() => setTargetCount(targetCount + 1)}
                          >
                            <PlusIcon width={15} height={15} />
                          </Button>
                        </Flex>
                      </Flex>
                    )}
                  </Flex>
                </Box>
              </Box>
            </Box>
          </Flex>
        </Flex>
      </Dialog.Content>
    </Dialog.Root>
  );
}

export function BattlePage({
  projects,
  grandClassDefinitions,
  activeProjectId,
  onProjectSelect,
  onCreateProject,
  onRenameProject,
  onDuplicateProject,
  onDeleteProject,
  onOpenProjectSettings,
  onUpdateProject,
  onBack,
  onAutomationStart,
  onLogEntry,
}: BattlePageProps) {
  const [running, setRunning] = useState(false);
  const [rainbowConfirmOpen, setRainbowConfirmOpen] = useState(false);
  const [stopAfterCurrentRequested, setStopAfterCurrentRequested] = useState(false);
  const [advancedSettingsOpen, setAdvancedSettingsOpen] = useState(false);
  const [draft, setDraft] = useState<BattleProjectDraft | null>(null);
  const [startError, setStartError] = useState<string | null>(null);
  const draftRef = useRef<BattleProjectDraft | null>(null);

  useEffect(() => {
    const unlisten = listen<AutomationEvent>("automation-status", (event) => {
      if (isAutomationTerminal(event.payload)) {
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

  const validateGrandServants = useCallback(() => {
    if (selectedProject?.advancedMode !== true) return true;
    const definition = grandClassDefinition(grandClassDefinitions, selectedProject.grandClass);
    if (!definition || !grandServantsAreValid(selectedProject.grandServants ?? [], definition)) {
      setStartError(definition?.validationMessage ?? "无法读取冠位职阶定义");
      return false;
    }
    return true;
  }, [grandClassDefinitions, selectedProject]);

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
    if (!validateGrandServants()) return;
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
            ? {
                memberId: slot.id,
                variantKey: slot.servantVariantKey ?? null,
                slotIndex,
                servantId: slot.servantId,
              }
            : null
        )
        .filter((selection): selection is { memberId: string; variantKey: string | null; slotIndex: number; servantId: number } =>
          selection != null
        ) ?? [];
    const maxMissionRuns =
      latestDraft.repeatMode === "count" ? latestDraft.repeatCount : null;
    const stopOnFiveStarCeDrop = projectStopsOnFiveStarCeDrop(selectedProject);
    const config = {
      projectId: selectedProject.id,
      partyOrder: null,
      supportClassFilter: null,
      supportServantName: null,
      supportServantId: selectedProject.supportServantId ?? null,
      supportServantVariantKey: selectedProject.supportServantVariantKey ?? null,
      supportSlotIndex:
        supportSlot != null ? selectedProject.slots?.indexOf(supportSlot) ?? null : null,
      supportMemberId: supportSlot?.id ?? null,
      supportCraftEssenceId: supportSlot?.craftEssenceId ?? null,
      supportCraftEssenceMlbRequired:
        supportSlot?.craftEssenceMlbRequired ?? true,
      supportGrandMode: selectedProject.supportGrandMode ?? false,
      supportGrandCraftEssenceIds:
        selectedProject.supportGrandCraftEssenceIds ?? [null, null, null],
      supportGrandCraftEssenceMlbRequired:
        selectedProject.supportGrandCraftEssenceMlbRequired ?? [true, true, true],
      supportGrandBondCeMode: selectedProject.supportGrandBondCeMode ?? "any",
      grandServants: selectedProject.grandServants ?? [],
      grandClass: selectedProject.grandClass ?? "saber",
      grandCardStrategy: selectedProject.grandCardStrategy ?? {
        chainPriority: DEFAULT_GRAND_CHAIN_PRIORITY,
      },
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
      stopOnFiveStarCeDrop,
      fiveStarCeDropTargetCount: stopOnFiveStarCeDrop
        ? projectFiveStarCeDropTargetCount(selectedProject)
        : DEFAULT_FIVE_STAR_CE_DROP_TARGET_COUNT,
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
    validateGrandServants,
  ]);

  const handleStart = useCallback(() => {
    if (!selectedProject) return;
    if (!validateGrandServants()) return;
    if (apRecoveryItems.includes("rainbow")) {
      setRainbowConfirmOpen(true);
      return;
    }
    startAutomation();
  }, [apRecoveryItems, selectedProject, startAutomation, validateGrandServants]);

  const handleStop = useCallback(() => {
    invoke("stop_automation")
      .then(() => {
        setRunning(false);
        setStopAfterCurrentRequested(false);
        onLogEntry?.("已请求停止自动化");
      })
      .catch(console.error);
  }, [onLogEntry]);

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
          grandClassDefinitions={grandClassDefinitions}
          activeProjectId={activeProjectId}
          disabled={running}
          onProjectSelect={onProjectSelect}
          onCreateProject={onCreateProject}
          onRenameProject={onRenameProject}
          onDuplicateProject={onDuplicateProject}
          onDeleteProject={onDeleteProject}
          onOpenProjectSettings={onOpenProjectSettings}
        />
      </Box>

      <Flex direction="column" className="battle-body battle-body-scroll">
        <Box className="battle-panel">
          <SectionHeading>重复任务</SectionHeading>
          <OptionCardRadioGroup
            value={repeatMode}
            className="battle-repeat-cards"
            disabled={running || !selectedProject}
            onValueChange={(value) => handleRepeatModeChange(value as BattleRepeatMode)}
            options={[
              {
                value: "single",
                title: "不重复",
                description: "任务执行完毕后停止",
              },
              {
                value: "infinite",
                title: "无限",
                description: "无限重复执行任务",
                accessory: <span className="battle-repeat-symbol">∞</span>,
              },
              {
                value: "count",
                title: "设置次数",
                className: "battle-repeat-card-count",
                onSelect: enableRepeatCount,
                accessory: (
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
                    <TextField.Root
                      className="battle-counter-value"
                      type="number"
                      min="1"
                      step="1"
                      variant="soft"
                      radius="none"
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
                ),
              },
            ]}
          />
        </Box>

        <Box className="battle-panel">
          <SectionHeading>行动力恢复</SectionHeading>
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
          <Button
            type="button"
            variant="soft"
            color="gray"
            disabled={running || !selectedProject}
            onClick={() => setAdvancedSettingsOpen(true)}
          >
            <GearIcon width={16} height={16} />
            <Text size="2">高级设置</Text>
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

      <BattleAdvancedSettingsDialog
        open={advancedSettingsOpen}
        project={selectedProject}
        disabled={running}
        onOpenChange={setAdvancedSettingsOpen}
        onUpdateProject={updateSelectedProject}
      />

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

      <AlertDialog.Root open={startError != null} onOpenChange={(open) => {
        if (!open) setStartError(null);
      }}>
        <AlertDialog.Content maxWidth="420px">
          <AlertDialog.Title>无法开始战斗</AlertDialog.Title>
          <AlertDialog.Description size="2">
            {startError}
          </AlertDialog.Description>
          <Flex justify="end" gap="3" mt="4">
            <AlertDialog.Action>
              <Button type="button" onClick={() => setStartError(null)}>
                知道了
              </Button>
            </AlertDialog.Action>
          </Flex>
        </AlertDialog.Content>
      </AlertDialog.Root>
    </Flex>
  );
}
