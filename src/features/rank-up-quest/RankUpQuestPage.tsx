import { useCallback, useEffect, useMemo, useRef, useState, type ComponentProps } from "react";
import { Box, Button, Flex, Text } from "@radix-ui/themes";
import { ImageIcon, ReloadIcon } from "@radix-ui/react-icons";
import { OptionCardRadioGroup } from "../../components/common/OptionCardRadioGroup";
import { PageHeader } from "../../components/common/PageHeader";
import { SectionHeading } from "../../components/common/SectionHeading";
import { FooterNote } from "../../components/common/FooterNote";
import { TaskActionButton } from "../../components/common/TaskActionButton";
import { convertFileSrc, invoke, listen } from "../../tauri";
import { ProjectBar } from "../projects/ProjectBar";
import type { Server } from "../../types/server";
import type {
  InterludeCaptureResult,
  NormalizedRect,
  RankUpQuestCaptureResult,
  RankUpQuestMode,
} from "../../types/rankUpQuest";
import { isAutomationTerminal, type AutomationStatus } from "../../types/automation";
import { buildProjectRunConfig } from "../battle/projectRunConfig";

interface AutomationEvent {
  status: AutomationStatus;
  currentScreen: string;
  message: string;
}

interface RankUpQuestPageProps extends Omit<ComponentProps<typeof ProjectBar>, "disabled" | "headerStyle"> {
  server: Server | null;
  interlude?: boolean;
  onBack: () => void;
  onAutomationStart?: () => void;
  onAutomationStartFailed?: () => void;
  onLogEntry?: (message: string) => void;
}

function rowStyle({ region }: { region: NormalizedRect }) {
  return {
    left: `${region.x * 100}%`,
    top: `${region.y * 100}%`,
    width: `${region.w * 100}%`,
    height: `${region.h * 100}%`,
  };
}

function useRankUpQuestCapture(setError: (value: string | null) => void, interlude = false) {
  const [capture, setCapture] = useState<RankUpQuestCaptureResult | InterludeCaptureResult | null>(null);
  const [selectedCandidateId, setSelectedCandidateId] = useState<string | null>(null);
  const [capturing, setCapturing] = useState(false);
  const captureRequest = useRef(0);

  useEffect(() => () => { captureRequest.current += 1; }, []);

  const capturePage = useCallback(() => {
    const request = ++captureRequest.current;
    setCapturing(true);
    setError(null);
    setSelectedCandidateId(null);
    invoke<RankUpQuestCaptureResult | InterludeCaptureResult>(
      interlude ? "capture_interlude_page" : "capture_rank_up_quest_page",
    )
      .then((result) => {
        if (request === captureRequest.current) setCapture(result);
      })
      .catch((value) => {
        if (request !== captureRequest.current) return;
        setCapture(null);
        setError(String(value));
      })
      .finally(() => {
        if (request === captureRequest.current) setCapturing(false);
      });
  }, [interlude, setError]);

  return { capture, capturing, capturePage, selectedCandidateId, setSelectedCandidateId };
}

export function RankUpQuestPage(props: RankUpQuestPageProps) {
  const [mode, setMode] = useState<RankUpQuestMode>("single");
  return <RankUpQuestServerPage key={props.server ?? "loading"} {...props} mode={mode} setMode={setMode} />;
}

function RankUpQuestServerPage({
  server,
  interlude = false,
  projects,
  activeProjectId,
  onBack,
  onAutomationStart,
  onAutomationStartFailed,
  onLogEntry,
  mode,
  setMode,
  ...projectBarProps
}: RankUpQuestPageProps & { mode: RankUpQuestMode; setMode: (mode: RankUpQuestMode) => void }) {
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [currentScreen, setCurrentScreen] = useState("");
  const selection = useRankUpQuestCapture(setError, interlude);
  const { capture, selectedCandidateId, capturing } = selection;
  const title = interlude ? "幕间物语" : "强化任务";
  const canUseServer = server != null;

  const selectedProject = useMemo(
    () => projects.find((project) => project.id === activeProjectId) ?? null,
    [activeProjectId, projects],
  );

  useEffect(() => {
    const unlisten = listen<AutomationEvent>("automation-status", (event) => {
      setCurrentScreen(event.payload.currentScreen);
      if (isAutomationTerminal(event.payload)) {
        setRunning(false);
      }
    });
    return () => {
      void unlisten.then((fn) => fn());
    };
  }, []);

  const handleStart = useCallback(() => {
    if (!selectedProject || !canUseServer) return;
    if (mode === "single" && (!capture || !selectedCandidateId)) return;
    if (
      selectedProject.apRecoveryItems?.includes("rainbow") &&
      !window.confirm(`当前队伍允许使用圣晶石恢复行动力。确认开始${title}自动化？`)
    ) {
      return;
    }

    setError(null);
    setRunning(true);
    setCurrentScreen("");
    onAutomationStart?.();
    const config = buildProjectRunConfig(selectedProject, {
      repeatMission: false,
      maxMissionRuns: null,
    });
    invoke(interlude ? "start_interlude_automation" : "start_rank_up_quest_automation", {
      config,
      workflow: {
        mode,
        captureId: mode === "single" ? capture?.captureId ?? null : null,
        candidateId: mode === "single" ? selectedCandidateId : null,
      },
    }).catch((value) => {
      const message = `启动失败: ${String(value)}`;
      setError(message);
      setRunning(false);
      onLogEntry?.(message);
      onAutomationStartFailed?.();
    });
  }, [
    capture,
    canUseServer,
    interlude,
    mode,
    onAutomationStart,
    onAutomationStartFailed,
    onLogEntry,
    selectedCandidateId,
    selectedProject,
    title,
  ]);

  const canStart =
    !running &&
    !capturing &&
    canUseServer &&
    selectedProject != null &&
    (mode !== "single" || (capture != null && selectedCandidateId != null));

  const statusError = error;
  const statusMessage = statusError
    || (server == null ? "正在读取服务器…"
      : capturing ? "正在截取游戏画面…"
      : running ? `当前页面：${currentScreen || "正在启动"}`
      : !selectedProject ? projects.length === 0 ? "请先创建战斗队伍" : "请选择战斗队伍"
      : mode === "all" ? (interlude ? "可开始，接手当前战斗时会在结算后停止" : "就绪，将按列表顺序执行")
      : selectedCandidateId ? `已选择${title}，可以开始`
      : `请截图并选择${title}`);

  return (
    <main className="main-content-inner workspace-page battle-page rank-up-quest-page" aria-label={title}>
      <PageHeader title={title} english={interlude ? "INTERLUDE" : "RANK UP QUEST"} onBack={onBack} backDisabled={running}>
        <ProjectBar {...projectBarProps} projects={projects} activeProjectId={activeProjectId} headerStyle disabled={running} />
      </PageHeader>

      <Flex direction="column" className="battle-body battle-body-scroll">
        <RankUpQuestPanels {...selection} server={server} interlude={interlude} running={running} mode={mode} setMode={setMode} />
      </Flex>

      <Flex justify="between" className="page-footer rank-up-quest-footer" gap="4">
        <FooterNote className="task-status" role={statusError ? "alert" : "status"}>
          {statusMessage}
        </FooterNote>
        <TaskActionButton label={running ? "停止" : `开始${title}`} running={running} disabled={!running && !canStart}
          onClick={running ? () => void invoke("stop_automation") : handleStart} />
      </Flex>
    </main>
  );
}

function RankUpQuestPanels({
  server, interlude = false, running, mode, setMode, capture, capturing, capturePage, selectedCandidateId, setSelectedCandidateId,
}: ReturnType<typeof useRankUpQuestCapture> & {
  server: Server | null;
  interlude?: boolean;
  running: boolean;
  mode: RankUpQuestMode;
  setMode: (mode: RankUpQuestMode) => void;
}) {
  const title = interlude ? "幕间物语" : "强化任务";
  const selectableLabel = interlude ? "选择幕间物语" : "选择可强化任务";
  const canUseServer = server != null;

  return (
    <>
        <Box className="battle-panel">
          <SectionHeading rail stacked english="MODE">完成方式</SectionHeading>
          <OptionCardRadioGroup
            indicator="diamond"
            className="battle-repeat-cards rank-up-quest-modes"
            value={mode}
            disabled={running}
            onValueChange={(value) => setMode(value as RankUpQuestMode)}
            options={[
              {
                value: "single",
                title: `完成指定${title}`,
                description: interlude ? "在总列表或幕间地图截图选关" : "截图后从当前画面选择一个可强化任务",
              },
              {
                value: "all",
                title: `按顺序完成所有${title}`,
                description: interlude ? "从幕间物语总列表启动批量执行" : "从列表顶部开始，依次完成所有亮色任务",
              },
            ]}
          />
          <Text size="1" color="gray" className="rank-up-quest-help">
            {interlude
              ? "自动跳过剧情和有编成条件的关卡。沿用所选队伍的助战、战斗和行动力恢复设置。"
              : "沿用所选队伍的助战条件、战斗指令和行动力恢复设置；忽略普通重复次数。"}
          </Text>
        </Box>

        {mode === "single" && (
          <Box className="battle-panel">
            <Flex align="center" justify="between" gap="3" className="rank-up-quest-capture-header">
              <SectionHeading rail stacked english="QUEST">选择{title}</SectionHeading>
              <Button
                variant="ghost"
                color="gray"
                className="rank-up-quest-capture-button"
                onClick={capturePage}
                disabled={running || capturing || !canUseServer}
              >
                <ReloadIcon />
                {capturing ? "正在截图…" : capture ? "重新截图" : "截取游戏画面"}
              </Button>
            </Flex>
            {capture ? (
              <Box className="rank-up-quest-capture">
                <img src={convertFileSrc(capture.imagePath)} alt={`${title}截图`} />
                {capture.rows.map((row) => {
                  const selected = row.candidateId === selectedCandidateId;
                  const disabledLabel = "restricted" in row && row.restricted ? "有编成条件" : "不可点击";
                  return (
                    <button
                      key={row.candidateId}
                      type="button"
                      style={rowStyle(row)}
                      className={`rank-up-quest-row${row.actionable ? " is-actionable" : " is-disabled"}${selected ? " is-selected" : ""}`}
                      disabled={!row.actionable || running}
                      aria-label={row.actionable ? selectableLabel : `${disabledLabel}的${title}`}
                      aria-pressed={selected}
                      onClick={() => setSelectedCandidateId(row.candidateId)}
                    >
                      <span>{row.actionable
                        ? (selected ? "已选择" : "kind" in row && row.kind === "quickLink" ? "地图入口" : interlude ? "可执行" : "可强化")
                        : disabledLabel}</span>
                    </button>
                  );
                })}
              </Box>
            ) : (
              <div className="rank-up-quest-empty">
                <ImageIcon width={28} height={28} aria-hidden="true" />
                <Text size="2" color="gray">请先在游戏中打开{title}页面，再截取画面。</Text>
              </div>
            )}
          </Box>
        )}
    </>
  );
}

export interface RankUpQuestWorkflowDraft {
  mode: RankUpQuestMode;
  captureId: string | null;
  candidateId: string | null;
}

export function RankUpQuestSelector({ running, onChange, onError }: {
  running: boolean;
  onChange: (workflow: RankUpQuestWorkflowDraft | null) => void;
  onError: (message: string | null) => void;
}) {
  const [mode, setMode] = useState<RankUpQuestMode>("single");
  const selection = useRankUpQuestCapture(onError);
  const { capture, selectedCandidateId, capturing } = selection;
  useEffect(() => {
    if (capturing) onChange(null);
    else if (mode === "all") onChange({ mode, captureId: null, candidateId: null });
    else if (capture && selectedCandidateId) onChange({ mode, captureId: capture.captureId, candidateId: selectedCandidateId });
    else onChange(null);
  }, [mode, capture, selectedCandidateId, capturing, onChange]);

  return <RankUpQuestPanels {...selection} server="CN" running={running} mode={mode} setMode={setMode} />;
}
