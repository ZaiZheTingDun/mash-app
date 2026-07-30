import { useCallback, useEffect, useState } from "react";
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

interface CraftEssenceEnhancementPageProps {
  onBack: () => void;
  onAutomationStart?: () => void;
  onLogEntry?: (message: string) => void;
}

type CraftEssenceEnhancementMode = "qpEfficient" | "fast";

export function CraftEssenceEnhancementPage({
  onBack,
  onAutomationStart,
  onLogEntry,
}: CraftEssenceEnhancementPageProps) {
  const [running, setRunning] = useState(false);
  const [currentScreen, setCurrentScreen] = useState("");

  useEffect(() => {
    const unlisten = listen<CraftEssenceEnhancementEvent>(
      "craft-essence-enhancement-automation-status",
      (event) => {
        const { currentScreen: nextScreen } = event.payload;
        setCurrentScreen(nextScreen);
        if (isAutomationTerminal(event.payload)) {
          setRunning(false);
        }
      }
    );
    return () => {
      unlisten.then((dispose) => dispose());
    };
  }, []);

  const handleStart = useCallback((mode: CraftEssenceEnhancementMode) => {
    setCurrentScreen("");
    setRunning(true);
    onAutomationStart?.();
    invoke("start_craft_essence_enhancement_automation", { mode }).catch((error) => {
      const message = `启动失败: ${String(error)}`;
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
          <Text size="1" color="gray" style={{ display: "block", marginTop: 4 }}>
            只消耗未锁定的 1/2 星礼装；程序不会解锁任何礼装。
          </Text>
          <Text size="1" color="gray" style={{ display: "block", marginTop: 2 }}>
            找不到丸子时会先制作并锁定新底卡；节省 QP 策略制作经验包，快速策略直接使用推荐素材。
          </Text>
        </Box>

        <Flex gap="3" wrap="wrap" className="battle-controls">
          <Button disabled={running} onClick={() => handleStart("qpEfficient")}>
            制作丸子（节省 QP 策略）
          </Button>
          <Button
            disabled={running}
            variant="soft"
            onClick={() => handleStart("fast")}
          >
            制作丸子（快速策略）
          </Button>
          <Button color="red" variant="soft" disabled={!running} onClick={handleStop}>
            停止
          </Button>
        </Flex>
      </Flex>
    </Flex>
  );
}
