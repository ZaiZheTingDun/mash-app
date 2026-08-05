import { useEffect, useMemo, useState } from "react";
import { Box, Flex, IconButton, Text } from "@radix-ui/themes";
import { Cross1Icon } from "@radix-ui/react-icons";
import goldFruitImage from "../../../src-tauri/resources/images/item_fruit_golden.png";
import silverFruitImage from "../../../src-tauri/resources/images/item_fruit_silver.png";
import bronzeFruitImage from "../../../src-tauri/resources/images/item_fruit_bronzed_cobalt.png";
import copperFruitImage from "../../../src-tauri/resources/images/item_fruit_bronze.png";
import saintQuartzImage from "../../../src-tauri/resources/images/item_saint_quartz.png";
import type { BattleApRecoveryItem } from "../../types/project";
import {
  battleRunRemainingMs,
  formatBattleRunDuration,
  type BattleRunStatus,
} from "../../types/battleRunStatus";

const RECOVERY_ITEMS: {
  value: BattleApRecoveryItem;
  label: string;
  imageSrc: string;
}[] = [
  { value: "gold", label: "黄金果实", imageSrc: goldFruitImage },
  { value: "silver", label: "白银果实", imageSrc: silverFruitImage },
  { value: "bronze", label: "青铜果实", imageSrc: bronzeFruitImage },
  { value: "copper", label: "赤铜果实", imageSrc: copperFruitImage },
  { value: "rainbow", label: "圣晶石", imageSrc: saintQuartzImage },
];

interface BattleRunStatusPanelProps {
  status: BattleRunStatus | null;
  onClose: () => void;
}

function isActive(status: BattleRunStatus): boolean {
  return status.phase === "starting" || status.phase === "running";
}

export function BattleRunStatusPanel({ status, onClose }: BattleRunStatusPanelProps) {
  const [nowMs, setNowMs] = useState(() => Date.now());
  const active = status != null && isActive(status);

  useEffect(() => {
    if (!active) return;
    const interval = window.setInterval(() => setNowMs(Date.now()), 1000);
    return () => window.clearInterval(interval);
  }, [active, status?.startedAtMs]);

  const consumedItems = useMemo(
    () =>
      status == null
        ? []
        : RECOVERY_ITEMS.filter((item) => status.apRecoveryUsage[item.value] > 0),
    [status],
  );

  const displayNowMs = status?.endedAtMs ?? nowMs;
  const elapsedText =
    status == null
      ? null
      : formatBattleRunDuration(displayNowMs - status.startedAtMs);
  let estimateText: string | null = null;
  if (status?.maxRuns != null) {
    if (status.completedRuns >= status.maxRuns) {
      estimateText = "已完成";
    } else if (isActive(status)) {
      const remainingMs = battleRunRemainingMs(status, displayNowMs);
      estimateText =
        remainingMs == null
          ? "计算中..."
          : formatBattleRunDuration(Math.ceil(remainingMs / 1000) * 1000, false);
    }
  }

  return (
    <Box className="operation-log-panel battle-run-status-panel">
      <Flex align="center" justify="between" className="operation-log-header">
        <Text size="1" weight="bold">运行状态</Text>
        <IconButton
          variant="ghost"
          color="gray"
          aria-label="关闭运行状态"
          onClick={onClose}
        >
          <Cross1Icon width={15} height={15} />
        </IconButton>
      </Flex>
      <Box className="operation-log-list">
        {status == null ? (
          <Text size="2" className="operation-log-placeholder">尚无运行记录</Text>
        ) : (
          <>
            <div className="operation-log-entry">
              <span className="operation-log-time">运行轮次：</span>
              <span className="operation-log-msg">
                {status.completedRuns} 次
                {status.maxRuns != null ? ` / ${status.maxRuns} 次` : ""}
              </span>
            </div>
            <div className="operation-log-entry">
              <span className="operation-log-time">运行时间：</span>
              <span className="operation-log-msg">{elapsedText}</span>
            </div>
            {estimateText != null && (
              <div className="operation-log-entry">
                <span className="operation-log-time">预计完成时间：</span>
                <span className="operation-log-msg">{estimateText}</span>
              </div>
            )}
            <div className="operation-log-entry">
              <span className="operation-log-time">道具消耗：</span>
              {consumedItems.length === 0 ? (
                <span className="operation-log-msg">暂无</span>
              ) : (
                <span className="operation-log-msg battle-run-status-items">
                  {consumedItems.map((item) => (
                    <span key={item.value} className="battle-run-status-item">
                      <img
                        src={item.imageSrc}
                        alt={item.label}
                        className="battle-run-status-item-image"
                      />
                      <span>× {status.apRecoveryUsage[item.value]}</span>
                    </span>
                  ))}
                </span>
              )}
            </div>
          </>
        )}
      </Box>
    </Box>
  );
}
