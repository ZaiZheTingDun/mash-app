import { useCallback, useEffect, useMemo, useState } from "react";
import { Box, Button, Flex, Select, Text } from "@radix-ui/themes";
import { ChevronLeftIcon, ReloadIcon } from "@radix-ui/react-icons";
import { OptionCardRadioGroup } from "../../components/common/OptionCardRadioGroup";
import { SectionHeading } from "../../components/common/SectionHeading";
import { convertFileSrc, invoke, listen } from "../../tauri";
import type { Project } from "../../types/project";
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

interface RankUpQuestPageProps {
  projects: Project[];
  activeProjectId: string | null;
  onProjectSelect: (id: string) => void;
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
  onProjectSelect,
  onBack,
  onAutomationStart,
  onAutomationStartFailed,
  onLogEntry,
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
    () => projects.find((project) => project.id === activeProjectId) ?? projects[0] ?? null,
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

  return (
    <Flex direction="column" className="battle-page rank-up-quest-page">
      <Flex align="center" gap="3" className="battle-header">
        <Button variant="soft" color="gray" onClick={onBack} disabled={running}>
          <ChevronLeftIcon width={16} height={16} />
          <Text size="2">返回</Text>
        </Button>
        <Text size="4" weight="bold">强化任务</Text>
      </Flex>

      <Flex direction="column" gap="4" className="battle-body battle-body-scroll">
        {server === "JP" && (
          <Box className="rank-up-quest-notice">
            <Text size="2" color="red">强化任务自动化首版仅支持国服。</Text>
          </Box>
        )}

        <Box className="battle-panel">
          <SectionHeading>完成方式</SectionHeading>
          <OptionCardRadioGroup
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
        </Box>

        <Box className="battle-panel">
          <SectionHeading>战斗队伍</SectionHeading>
          <Select.Root
            value={selectedProject?.id ?? ""}
            onValueChange={onProjectSelect}
            disabled={running || projects.length === 0}
          >
            <Select.Trigger placeholder="选择队伍" className="rank-up-quest-team-select" />
            <Select.Content>
              {projects.map((project) => (
                <Select.Item key={project.id} value={project.id}>{project.name}</Select.Item>
              ))}
            </Select.Content>
          </Select.Root>
          <Text size="1" color="gray" className="rank-up-quest-help">
            沿用该队伍的助战条件、战斗指令和行动力恢复设置；忽略普通重复次数。
          </Text>
        </Box>

        {mode === "single" && (
          <Box className="battle-panel">
            <Flex align="center" justify="between" gap="3">
              <SectionHeading>选择强化任务</SectionHeading>
              <Button
                variant="soft"
                color="gray"
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
                      onClick={() => setSelectedCandidateId(row.candidateId)}
                    >
                      <span>{row.actionable ? (selected ? "已选择" : "可强化") : "不可点击"}</span>
                    </button>
                  );
                })}
              </Box>
            ) : (
              <Text size="2" color="gray">请先在游戏中打开强化任务页面，再截取画面。</Text>
            )}
          </Box>
        )}

        {error && <Text size="2" color="red">{error}</Text>}
        {running && (
          <Text size="2" color="gray">当前页面：{currentScreen || "正在启动"}</Text>
        )}
      </Flex>

      <Flex justify="end" align="center" className="page-footer" gap="3">
        {running ? (
          <Button color="red" variant="soft" onClick={() => void invoke("stop_automation")}>停止</Button>
        ) : (
          <Button disabled={!canStart} onClick={handleStart}>开始强化任务</Button>
        )}
      </Flex>
    </Flex>
  );
}
