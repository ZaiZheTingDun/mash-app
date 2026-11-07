import { useCallback, useEffect, useState } from "react";
import { Box, Flex, Text } from "@radix-ui/themes";
import { PageHeader } from "../../components/common/PageHeader";
import { SectionHeading } from "../../components/common/SectionHeading";
import { TaskActionButton } from "../../components/common/TaskActionButton";
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

const SCREEN_LABELS: Record<string, string> = {
  FriendPointSummonMain: "友情点召唤主页",
  FriendPointSummonConfirmation: "召唤确认",
  FriendPointSummonAnimation: "召唤动画",
  FriendPointSummonResult: "召唤结果",
};

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
    <main className="main-content-inner workspace-page battle-page friend-point-summon-page" aria-label="友情点召唤">
      <PageHeader title="友情点召唤" english="FRIEND POINT SUMMON" onBack={onBack} backDisabled={running}>
        {null}
      </PageHeader>

      <Flex direction="column" className="battle-body battle-body-scroll">
        <Box asChild className="battle-panel">
          <section aria-label="召唤方式">
            <SectionHeading rail stacked english="SUMMON">召唤方式</SectionHeading>
            <Flex align="baseline" gap="3" className="friend-point-summon-mode">
              <Text size="4" weight="bold">100 次召唤</Text>
              <Text size="2" color="gray">仅支持国服</Text>
            </Flex>
            <Text as="p" size="2" color="gray" className="friend-point-summon-note">
              启动前请停留在友情点召唤主页，无法继续召唤时自动停止。
            </Text>
          </section>
        </Box>

        <Box asChild className="battle-panel">
          <section aria-label="召唤进度">
            <SectionHeading rail stacked english="PROGRESS">召唤进度</SectionHeading>
            <dl className="friend-point-summon-progress" aria-live="polite" aria-atomic="true">
              <div>
                <dt>已完成批次</dt>
                <dd>{completedBatches}<span>批</span></dd>
              </div>
              <div>
                <dt>累计召唤</dt>
                <dd>{summonedCount}<span>次</span></dd>
              </div>
            </dl>
          </section>
        </Box>
      </Flex>

      <Flex justify="between" gap="4" className="page-footer friend-point-summon-footer">
        <div className="task-status" role="status">
          <i aria-hidden="true" />
          <span>当前页面：{currentScreen ? SCREEN_LABELS[currentScreen] ?? currentScreen : running ? "正在识别…" : "等待启动"}</span>
        </div>
        <TaskActionButton label={running ? "停止" : "开始"} running={running} onClick={running ? handleStop : handleStart} />
      </Flex>
    </main>
  );
}
