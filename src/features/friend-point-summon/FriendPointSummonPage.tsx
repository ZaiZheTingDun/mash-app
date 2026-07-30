import { useCallback, useEffect, useState } from "react";
import { Box, Button, Flex, Text } from "@radix-ui/themes";
import { ChevronLeftIcon } from "@radix-ui/react-icons";
import { invoke, listen } from "../../tauri";
import { isAutomationTerminal, type AutomationStatus } from "../../types/automation";

interface FriendPointSummonEvent {
  state: string;
  status: AutomationStatus;
  currentScreen: string;
  message: string;
  completedBatches: number;
  summonedCount: number;
}

interface FriendPointSummonPageProps {
  onBack: () => void;
  onAutomationStart?: () => void;
  onLogEntry?: (message: string) => void;
}

export function FriendPointSummonPage({
  onBack,
  onAutomationStart,
  onLogEntry,
}: FriendPointSummonPageProps) {
  const [running, setRunning] = useState(false);
  const [currentScreen, setCurrentScreen] = useState("");
  const [completedBatches, setCompletedBatches] = useState(0);
  const [summonedCount, setSummonedCount] = useState(0);

  useEffect(() => {
    const unlisten = listen<FriendPointSummonEvent>(
      "friend-point-summon-automation-status",
      (event) => {
        const payload = event.payload;
        setCurrentScreen(payload.currentScreen);
        setCompletedBatches(payload.completedBatches);
        setSummonedCount(payload.summonedCount);
        if (isAutomationTerminal(payload)) {
          setRunning(false);
        }
      }
    );
    return () => {
      unlisten.then((dispose) => dispose());
    };
  }, []);

  const handleStart = useCallback(() => {
    setCurrentScreen("");
    setCompletedBatches(0);
    setSummonedCount(0);
    setRunning(true);
    onAutomationStart?.();
    invoke("start_friend_point_summon_automation").catch((error) => {
      const message = `启动失败: ${String(error)}`;
      onLogEntry?.(message);
      setRunning(false);
    });
  }, [onAutomationStart, onLogEntry]);

  const handleStop = useCallback(() => {
    invoke("stop_friend_point_summon_automation").catch(console.error);
  }, []);

  return (
    <Flex direction="column" className="battle-page">
      <Flex align="center" gap="3" className="battle-header">
        <Button variant="soft" color="gray" onClick={onBack}>
          <ChevronLeftIcon width={16} height={16} />
          <Text size="2">返回</Text>
        </Button>
        <Text size="4" weight="bold">
          友情点抽取
        </Text>
      </Flex>

      <Flex direction="column" gap="4" className="battle-body">
        <Box className="enhancement-summary">
          <Text size="2" color="gray">
            当前页面：{currentScreen || "等待启动"}
          </Text>
          <Text size="2" color="gray" style={{ display: "block", marginTop: 4 }}>
            已完成 {completedBatches} 批，共 {summonedCount} 次召唤
          </Text>
          <Text size="1" color="gray" style={{ display: "block", marginTop: 4 }}>
            仅支持国服。启动前请停留在友情点抽取主页；程序只执行“100次召唤”流程，并在无法继续时停止。
          </Text>
        </Box>

        <Flex gap="3" wrap="wrap" className="battle-controls">
          <Button disabled={running} onClick={handleStart}>
            开始友情点抽取
          </Button>
          <Button color="red" variant="soft" disabled={!running} onClick={handleStop}>
            停止
          </Button>
        </Flex>
      </Flex>
    </Flex>
  );
}
