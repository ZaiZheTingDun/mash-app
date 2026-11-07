import { useCallback, useEffect, useMemo, useState, type ComponentProps } from "react";
import { Box, Button, Flex, Text } from "@radix-ui/themes";
import { ImageIcon, ReloadIcon } from "@radix-ui/react-icons";
import { OptionCardRadioGroup } from "../../components/common/OptionCardRadioGroup";
import { PageHeader } from "../../components/common/PageHeader";
import { SectionHeading } from "../../components/common/SectionHeading";
import { TaskActionButton } from "../../components/common/TaskActionButton";
import { convertFileSrc, invoke, listen } from "../../tauri";
import { ProjectBar } from "../projects/ProjectBar";
import type { Server } from "../../types/server";
import type {
  RankUpQuestCaptureResult,
  RankUpQuestMode,
  RankUpQuestRow,
} from "../../types/rankUpQuest";
import { isAutomationTerminal, type AutomationStatus } from "../../types/automation";
import { buildProjectRunConfig } from "../battle/projectRunConfig";

interface AutomationEvent {
  status: AutomationStatus;
  currentScreen: string;
  message: string;
}

interface RankUpQuestPageProps extends Omit<ComponentProps<typeof ProjectBar>, "disabled" | "headerStyle"> {
  onBack: () => void;
  onAutomationStart?: () => void;
  onAutomationStartFailed?: () => void;
  onLogEntry?: (message: string) => void;
}

function rowStyle(row: RankUpQuestRow) {
  return {
    left: `${row.region.x * 100}%`,
    top: `${row.region.y * 100}%`,
    width: `${row.region.w * 100}%`,
    height: `${row.region.h * 100}%`,
  };
}

export function RankUpQuestPage({
  projects,
  activeProjectId,
  onBack,
  onAutomationStart,
  onAutomationStartFailed,
  onLogEntry,
  ...projectBarProps
}: RankUpQuestPageProps) {
  const [mode, setMode] = useState<RankUpQuestMode>("single");
  const [server, setServer] = useState<Server | null>(null);
  const [capture, setCapture] = useState<RankUpQuestCaptureResult | null>(null);
  const [selectedCandidateId, setSelectedCandidateId] = useState<string | null>(null);
  const [capturing, setCapturing] = useState(false);
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [currentScreen, setCurrentScreen] = useState("");

  const selectedProject = useMemo(
    () => projects.find((project) => project.id === activeProjectId) ?? null,
    [activeProjectId, projects],
  );

  useEffect(() => {
    invoke<Server>("get_server").then(setServer).catch((value) => setError(String(value)));
  }, []);

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

  const capturePage = useCallback(() => {
    setCapturing(true);
    setError(null);
    setSelectedCandidateId(null);
    invoke<RankUpQuestCaptureResult>("capture_rank_up_quest_page")
      .then(setCapture)
      .catch((value) => {
        setCapture(null);
        setError(String(value));
      })
      .finally(() => setCapturing(false));
  }, []);

  const handleStart = useCallback(() => {
    if (!selectedProject || server !== "CN") return;
    if (mode === "single" && (!capture || !selectedCandidateId)) return;
    if (
      selectedProject.apRecoveryItems?.includes("rainbow") &&
      !window.confirm("当前队伍允许使用圣晶石恢复行动力。确认开始强化任务自动化？")
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
    invoke("start_rank_up_quest_automation", {
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
    mode,
    onAutomationStart,
    onAutomationStartFailed,
    onLogEntry,
    selectedCandidateId,
    selectedProject,
    server,
  ]);

  const canStart =
    !running &&
    !capturing &&
    server === "CN" &&
    selectedProject != null &&
    (mode === "all" || (capture != null && selectedCandidateId != null));

  const statusError = error || (server === "JP" ? "强化任务自动化仅支持国服。" : null);
  const statusMessage = statusError
    || (capturing ? "正在截取游戏画面…"
      : running ? `当前页面：${currentScreen || "正在启动"}`
      : !selectedProject ? projects.length === 0 ? "请先创建战斗队伍" : "请选择战斗队伍"
      : mode === "all" ? "就绪，将按列表顺序执行"
      : selectedCandidateId ? "已选择强化任务，可以开始"
      : "请截图并选择强化任务");

  return (
    <main className="main-content-inner workspace-page battle-page rank-up-quest-page" aria-label="强化任务">
      <PageHeader title="强化任务" english="RANK UP QUEST" onBack={onBack} backDisabled={running}>
        <ProjectBar {...projectBarProps} projects={projects} activeProjectId={activeProjectId} headerStyle disabled={running} />
      </PageHeader>

      <Flex direction="column" className="battle-body battle-body-scroll">
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
                title: "完成指定强化任务",
                description: "截图后从当前画面选择一个可强化任务",
              },
              {
                value: "all",
                title: "按顺序完成所有强化任务",
                description: "从列表顶部开始，依次完成所有亮色任务",
              },
            ]}
          />
          <Text size="1" color="gray" className="rank-up-quest-help">
            沿用所选队伍的助战条件、战斗指令和行动力恢复设置；忽略普通重复次数。
          </Text>
        </Box>

        {mode === "single" && (
          <Box className="battle-panel">
            <Flex align="center" justify="between" gap="3" className="rank-up-quest-capture-header">
              <SectionHeading rail stacked english="QUEST">选择强化任务</SectionHeading>
              <Button
                variant="ghost"
                color="gray"
                className="rank-up-quest-capture-button"
                onClick={capturePage}
                disabled={running || capturing || server !== "CN"}
              >
                <ReloadIcon />
                {capturing ? "正在截图…" : capture ? "重新截图" : "截取游戏画面"}
              </Button>
            </Flex>
            {capture ? (
              <Box className="rank-up-quest-capture">
                <img src={convertFileSrc(capture.imagePath)} alt="强化任务截图" />
                {capture.rows.map((row) => {
                  const selected = row.candidateId === selectedCandidateId;
                  return (
                    <button
                      key={row.candidateId}
                      type="button"
                      style={rowStyle(row)}
                      className={`rank-up-quest-row${row.actionable ? " is-actionable" : " is-disabled"}${selected ? " is-selected" : ""}`}
                      disabled={!row.actionable || running}
                      aria-label={row.actionable ? "选择可强化任务" : "不可点击的强化任务"}
                      aria-pressed={selected}
                      onClick={() => setSelectedCandidateId(row.candidateId)}
                    >
                      <span>{row.actionable ? (selected ? "已选择" : "可强化") : "不可点击"}</span>
                    </button>
                  );
                })}
              </Box>
            ) : (
              <div className="rank-up-quest-empty">
                <ImageIcon width={28} height={28} aria-hidden="true" />
                <Text size="2" color="gray">请先在游戏中打开强化任务页面，再截取画面。</Text>
              </div>
            )}
          </Box>
        )}
      </Flex>

      <Flex justify="between" className="page-footer rank-up-quest-footer" gap="4">
        <div className="task-status" role={statusError ? "alert" : "status"} data-error={!!statusError}>
          <i aria-hidden="true" />
          <span>{statusMessage}</span>
        </div>
        <TaskActionButton label={running ? "停止" : "开始强化任务"} running={running} disabled={!running && !canStart}
          onClick={running ? () => void invoke("stop_automation") : handleStart} />
      </Flex>
    </main>
  );
}
