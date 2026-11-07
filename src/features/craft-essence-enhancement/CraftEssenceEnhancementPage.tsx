import { useCallback, useEffect, useState } from "react";
import { Box, Flex } from "@radix-ui/themes";
import { OptionCardRadioGroup } from "../../components/common/OptionCardRadioGroup";
import { PageHeader } from "../../components/common/PageHeader";
import { SectionHeading } from "../../components/common/SectionHeading";
import { TaskActionButton } from "../../components/common/TaskActionButton";
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

const SCREEN_LABELS: Record<string, string> = {
  CraftEssenceEnhancement: "礼装强化",
  CraftEssenceSelect: "选择礼装",
  CraftEssenceLockMode: "锁定礼装",
  MaterialSelect: "选择素材",
  FilterDialog: "筛选设置",
  OrderDialog: "排序设置",
  RecommendMaterialDialog: "推荐素材确认",
  RecommendMaterialEmptyDialog: "推荐素材不足",
  EnhancedMaterialWarningDialog: "强化素材确认",
  EnhancementConfirmDialog: "强化确认",
  EnhancementAnimation: "强化动画",
  EnhancementResultRecovery: "强化结果",
  QpEfficientComplete: "经验包制作完成",
  Unknown: "正在识别…",
};

export function CraftEssenceEnhancementPage({
  onBack,
  onAutomationStart,
  onLogEntry,
}: CraftEssenceEnhancementPageProps) {
  const [running, setRunning] = useState(false);
  const [mode, setMode] = useState<CraftEssenceEnhancementMode>("qpEfficient");
  const [currentScreen, setCurrentScreen] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const unlisten = listen<CraftEssenceEnhancementEvent>(
      "craft-essence-enhancement-automation-status",
      (event) => {
        const { currentScreen: nextScreen } = event.payload;
        setCurrentScreen(nextScreen);
        if (event.payload.status === "error") {
          setError(event.payload.message);
        }
        if (isAutomationTerminal(event.payload)) {
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
    setError(null);
    setRunning(true);
    onAutomationStart?.();
    invoke("start_craft_essence_enhancement_automation", { mode }).catch((error) => {
      const message = `启动失败: ${String(error)}`;
      onLogEntry?.(message);
      setError(message);
      setRunning(false);
    });
  }, [mode, onAutomationStart, onLogEntry]);

  const handleStop = useCallback(() => {
    invoke("stop_craft_essence_enhancement_automation").catch(console.error);
  }, []);

  return (
    <main className="main-content-inner workspace-page battle-page craft-essence-enhancement-page" aria-label="强化概念礼装">
      <PageHeader title="强化概念礼装" english="CE ENHANCEMENT" onBack={onBack} backDisabled={running}>
        {null}
      </PageHeader>

      <Flex direction="column" className="battle-body battle-body-scroll">
        <Box asChild className="battle-panel">
          <section aria-label="制作策略">
            <SectionHeading rail stacked english="STRATEGY">制作策略</SectionHeading>
            <OptionCardRadioGroup
              indicator="diamond"
              className="battle-repeat-cards craft-essence-enhancement-modes"
              value={mode}
              disabled={running}
              onValueChange={(value) => setMode(value as CraftEssenceEnhancementMode)}
              options={[
                { value: "qpEfficient", title: "节省 QP 策略", description: "制作经验包，节省 QP" },
                { value: "fast", title: "快速策略", description: "直接使用推荐素材" },
              ]}
            />
          </section>
        </Box>

        <Box asChild className="battle-panel">
          <section aria-label="素材规则">
            <SectionHeading rail stacked english="MATERIALS">素材规则</SectionHeading>
            <dl className="craft-essence-enhancement-rules">
              <div>
                <dt>消耗素材</dt>
                <dd>只消耗未锁定的 1 / 2 星礼装，程序不会解锁任何礼装。</dd>
              </div>
              <div>
                <dt>底卡处理</dt>
                <dd>找不到丸子时，先制作并锁定新底卡。</dd>
              </div>
            </dl>
          </section>
        </Box>
      </Flex>

      <Flex justify="between" gap="4" className="page-footer craft-essence-enhancement-footer">
        <div className="task-status" role={error ? "alert" : "status"} data-error={!!error}>
          <i aria-hidden="true" />
          <span>{error || `当前页面：${currentScreen ? SCREEN_LABELS[currentScreen] ?? currentScreen : running ? "正在识别…" : "等待启动"}`}</span>
        </div>
        <TaskActionButton label={running ? "停止" : "开始制作丸子"} running={running} onClick={running ? handleStop : handleStart} />
      </Flex>
    </main>
  );
}
