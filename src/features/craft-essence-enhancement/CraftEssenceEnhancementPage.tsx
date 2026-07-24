import { useCallback, useEffect, useRef, useState } from "react";
import { Box, Button, Flex, Text } from "@radix-ui/themes";
import { ChevronLeftIcon } from "@radix-ui/react-icons";
import { invoke, listen } from "../../tauri";
import { isAutomationTerminal, type AutomationStatus } from "../../types/automation";

interface CraftEssenceEnhancementEvent {
  state: string;
  status: AutomationStatus;
  currentScreen: string;
  message: string;
}

interface LogEntry {
  time: string;
  message: string;
}

interface CraftEssenceEnhancementPageProps {
  onBack: () => void;
  onAutomationStart?: () => void;
  onLogEntry?: (message: string) => void;
}

function timestamp(): string {
  const now = new Date();
  return [now.getHours(), now.getMinutes(), now.getSeconds()]
    .map((value) => String(value).padStart(2, "0"))
    .join(":");
}

export function CraftEssenceEnhancementPage({
  onBack,
  onAutomationStart,
  onLogEntry,
}: CraftEssenceEnhancementPageProps) {
  const [running, setRunning] = useState(false);
  const [currentScreen, setCurrentScreen] = useState("");
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const logEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const unlisten = listen<CraftEssenceEnhancementEvent>(
      "craft-essence-enhancement-automation-status",
      (event) => {
        const { currentScreen: nextScreen, message } = event.payload;
        setCurrentScreen(nextScreen);
        setLogs((previous) => [...previous, { time: timestamp(), message }]);
        if (isAutomationTerminal(event.payload)) {
          setRunning(false);
        }
      }
    );
    return () => {
      unlisten.then((dispose) => dispose());
    };
  }, []);

  useEffect(() => {
    logEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [logs]);

  const handleStart = useCallback(() => {
    setLogs([]);
    setCurrentScreen("");
    setRunning(true);
    onAutomationStart?.();
    invoke("start_craft_essence_enhancement_automation").catch((error) => {
      const message = `启动失败: ${String(error)}`;
      setLogs((previous) => [...previous, { time: timestamp(), message }]);
      onLogEntry?.(message);
      setRunning(false);
    });
  }, [onAutomationStart, onLogEntry]);

  const handleStop = useCallback(() => {
    invoke("stop_craft_essence_enhancement_automation").catch(console.error);
  }, []);

  return (
    <Flex direction="column" className="battle-page">
      <Flex align="center" gap="3" className="battle-header">
        <Button variant="soft" color="gray" onClick={onBack}>
          <ChevronLeftIcon width={16} height={16} />
          <Text size="2">返回</Text>
        </Button>
        <Text size="4" weight="bold">
          强化概念礼装
        </Text>
      </Flex>

      <Flex direction="column" gap="4" className="battle-body">
        <Box className="enhancement-summary">
          <Text size="2" color="gray">
            当前页面：{currentScreen || "等待启动"}
          </Text>
        </Box>

        <Flex gap="3" className="battle-controls">
          <Button disabled={running} onClick={handleStart}>
            开始
          </Button>
          <Button color="red" variant="soft" disabled={!running} onClick={handleStop}>
            停止
          </Button>
        </Flex>

        <Box className="battle-log-container">
          <Text size="2" weight="medium" style={{ marginBottom: 6, display: "block" }}>
            运行日志
          </Text>
          <Box className="battle-log">
            {logs.length === 0 && (
              <Text size="1" className="battle-log-placeholder">
                等待启动…
              </Text>
            )}
            {logs.map((entry, index) => (
              <div key={index} className="battle-log-entry">
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
