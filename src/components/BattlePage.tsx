import { useState, useEffect, useCallback, useRef } from "react";
import { Box, Flex, Text } from "@radix-ui/themes";
import { invoke, listen } from "../tauri";
import { ChevronLeftIcon } from "@radix-ui/react-icons";
import type { Project } from "../types/project";

interface AutomationEvent {
  state: string;
  currentScreen: string;
  message: string;
}

interface LogEntry {
  time: string;
  message: string;
}

type ApRecoveryItem = "rainbow" | "gold" | "silver" | "bronze" | "copper";

const AP_RECOVERY_OPTIONS: { value: ApRecoveryItem; label: string }[] = [
  { value: "rainbow", label: "彩苹果" },
  { value: "gold", label: "黄金苹果" },
  { value: "silver", label: "白银苹果" },
  { value: "bronze", label: "青铜苹果" },
  { value: "copper", label: "赤铜苹果" },
];

interface BattlePageProps {
  defaultProjectId: string | null;
  onBack: () => void;
}

function timestamp(): string {
  const d = new Date();
  return [d.getHours(), d.getMinutes(), d.getSeconds()]
    .map((n) => String(n).padStart(2, "0"))
    .join(":");
}

export function BattlePage({ defaultProjectId, onBack }: BattlePageProps) {
  const [projects, setProjects] = useState<Project[]>([]);
  const [selectedId, setSelectedId] = useState<string>(defaultProjectId ?? "");
  const [running, setRunning] = useState(false);
  const [runCount, setRunCount] = useState("");
  const [autoEatApples, setAutoEatApples] = useState(false);
  const [apRecoveryItems, setApRecoveryItems] = useState<ApRecoveryItem[]>([]);
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const logEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    invoke<Project[]>("list_projects")
      .then((list) => {
        setProjects(list);
        setSelectedId((prev) => prev || list[0]?.id || "");
      })
      .catch(console.error);
  }, []);

  useEffect(() => {
    const unlisten = listen<AutomationEvent>("automation-status", (event) => {
      const { message, state } = event.payload;
      setLogs((prev) => [...prev, { time: timestamp(), message }]);

      if (state.includes("Idle") || state.includes("Finished") || state.includes("Error")) {
        setRunning(false);
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    logEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [logs]);

  const handleStart = useCallback(() => {
    if (!selectedId) return;
    setLogs([]);
    setRunning(true);

    const project = projects.find((p) => p.id === selectedId);
    const parsedRunCount = Number(runCount);
    const maxMissionRuns =
      runCount.trim() && Number.isInteger(parsedRunCount) && parsedRunCount > 0
        ? parsedRunCount
        : null;
    // The runner currently only consumes the support slot's CE pin (for
    // row verification on the support-select screen). Party-slot CEs
    // are persisted on the project but ignored here.
    const supportSlot = project?.slots?.find((s) => s.type === "support");
    const config = {
      projectId: selectedId,
      partyOrder: null,
      supportClassFilter: null,
      supportServantName: null,
      supportServantId: project?.supportServantId ?? null,
      supportCraftEssenceId: supportSlot?.craftEssenceId ?? null,
      servantSelections: [],
      maxSupportScrolls: 3,
      repeatMission: project?.repeatMission ?? false,
      maxMissionRuns,
      apRecoveryItems: autoEatApples ? apRecoveryItems : [],
    };

    invoke("start_automation", { config }).catch((err) => {
      setLogs((prev) => [
        ...prev,
        { time: timestamp(), message: `启动失败: ${err}` },
      ]);
      setRunning(false);
    });
  }, [selectedId, projects, runCount, autoEatApples, apRecoveryItems]);

  const handleStop = useCallback(() => {
    invoke("stop_automation").catch(console.error);
  }, []);

  const handleStopAfterCurrent = useCallback(() => {
    invoke("stop_automation_after_current")
      .then(() => {
        setLogs((prev) => [
          ...prev,
          { time: timestamp(), message: "已设置：运行完当前轮次后停止" },
        ]);
      })
      .catch(console.error);
  }, []);

  const handleToggleApRecoveryItem = useCallback((item: ApRecoveryItem) => {
    if (
      item === "rainbow" &&
      !apRecoveryItems.includes("rainbow") &&
      !window.confirm("彩苹果会消耗圣晶石。确定要在本次运行中允许自动使用彩苹果吗？")
    ) {
      return;
    }
    setApRecoveryItems((prev) =>
      prev.includes(item) ? prev.filter((v) => v !== item) : [...prev, item]
    );
  }, [apRecoveryItems]);

  // Persist the "repeat mission" toggle on the active project so it
  // survives reloads and project switches. Updates the local list in
  // place from the backend's saved copy to avoid a refetch round-trip.
  const handleToggleRepeat = useCallback(
    (next: boolean) => {
      const project = projects.find((p) => p.id === selectedId);
      if (!project) return;
      invoke<Project>("update_project", {
        project: { ...project, repeatMission: next },
      })
        .then((saved) => {
          setProjects((prev) =>
            prev.map((p) => (p.id === saved.id ? saved : p))
          );
        })
        .catch(console.error);
    },
    [projects, selectedId]
  );

  const selectedProject = projects.find((p) => p.id === selectedId);

  return (
    <Flex direction="column" className="battle-page">
      <Flex align="center" gap="3" className="battle-header">
        <button className="battle-back-btn" onClick={onBack}>
          <ChevronLeftIcon width={16} height={16} />
          <Text size="2">返回</Text>
        </button>
        <Text size="4" weight="bold">
          战斗运行
        </Text>
      </Flex>

      <Flex direction="column" gap="4" className="battle-body">
        <Box>
          <Text size="2" weight="medium" style={{ marginBottom: 6, display: "block" }}>
            选择项目
          </Text>
          <select
            className="battle-selector"
            value={selectedId}
            onChange={(e) => setSelectedId(e.target.value)}
            disabled={running}
          >
            {projects.length === 0 && (
              <option value="">-- 无可用项目 --</option>
            )}
            {projects.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
        </Box>

        <Flex align="center" gap="2">
          <input
            id="repeat-mission"
            type="checkbox"
            className="battle-checkbox"
            checked={selectedProject?.repeatMission ?? false}
            disabled={running || !selectedProject}
            onChange={(e) => handleToggleRepeat(e.target.checked)}
          />
          <label htmlFor="repeat-mission">
            <Text size="2">重复任务（结算后继续同一任务）</Text>
          </label>
        </Flex>

        <Box>
          <Text size="2" weight="medium" style={{ marginBottom: 6, display: "block" }}>
            运行轮数
          </Text>
          <input
            className="battle-number-input"
            type="number"
            min="1"
            step="1"
            inputMode="numeric"
            placeholder="不限制"
            value={runCount}
            disabled={running}
            onChange={(e) => setRunCount(e.target.value)}
          />
        </Box>

        <Box>
          <Flex align="center" gap="2">
            <input
              id="auto-eat-apples"
              type="checkbox"
              className="battle-checkbox"
              checked={autoEatApples}
              disabled={running}
              onChange={(e) => setAutoEatApples(e.target.checked)}
            />
            <label htmlFor="auto-eat-apples">
              <Text size="2">自动吃苹果</Text>
            </label>
          </Flex>
          {autoEatApples && (
            <div className="battle-apple-menu">
              {AP_RECOVERY_OPTIONS.map((option) => (
                <label key={option.value} className="battle-apple-option">
                  <input
                    type="checkbox"
                    className="battle-checkbox"
                    checked={apRecoveryItems.includes(option.value)}
                    disabled={running}
                    onChange={() => handleToggleApRecoveryItem(option.value)}
                  />
                  <Text size="2">{option.label}</Text>
                </label>
              ))}
            </div>
          )}
        </Box>

        <Flex gap="3" className="battle-controls">
          <button
            className="battle-btn battle-btn-start"
            disabled={running || !selectedId}
            onClick={handleStart}
          >
            开始
          </button>
          <button
            className="battle-btn battle-btn-stop"
            disabled={!running}
            onClick={handleStop}
          >
            停止
          </button>
          <button
            className="battle-btn battle-btn-stop"
            disabled={!running}
            onClick={handleStopAfterCurrent}
          >
            运行完当前轮次后停止
          </button>
        </Flex>

        <Box className="battle-log-container">
          <Text size="2" weight="medium" style={{ marginBottom: 6, display: "block" }}>
            操作日志
          </Text>
          <Box className="battle-log">
            {logs.length === 0 && (
              <Text size="1" className="battle-log-placeholder">
                等待启动…
              </Text>
            )}
            {logs.map((entry, i) => (
              <div key={i} className="battle-log-entry">
                <span className="battle-log-time">{entry.time}</span>
                <span className="battle-log-msg">{entry.message}</span>
              </div>
            ))}
            <div ref={logEndRef} />
          </Box>
        </Box>
      </Flex>
    </Flex>
  );
}
