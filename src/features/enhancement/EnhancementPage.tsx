import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Box, Button, Flex, Text } from "@radix-ui/themes";
import { ChevronLeftIcon } from "@radix-ui/react-icons";
import { invoke, listen } from "../../tauri";
import { ServantSelectDialog } from "../team/ServantSelectDialog";
import type { Servant } from "../../types/servant";

interface EnhancementEvent {
  state: string;
  currentScreen: string;
  message: string;
}

interface LogEntry {
  time: string;
  message: string;
}

interface EnhancementPageProps {
  servants: Servant[];
  onBack: () => void;
  onAutomationStart?: () => void;
  onLogEntry?: (message: string) => void;
}

function timestamp(): string {
  const d = new Date();
  return [d.getHours(), d.getMinutes(), d.getSeconds()]
    .map((n) => String(n).padStart(2, "0"))
    .join(":");
}

export function EnhancementPage({
  servants,
  onBack,
  onAutomationStart,
  onLogEntry,
}: EnhancementPageProps) {
  const [selectedVariantKey, setSelectedVariantKey] = useState<string>(
    servants[0]?.variantKey ?? ""
  );
  const [dialogOpen, setDialogOpen] = useState(false);
  const [running, setRunning] = useState(false);
  const [currentScreen, setCurrentScreen] = useState("");
  const [logs, setLogs] = useState<LogEntry[]>([]);
  const logEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const unlisten = listen<EnhancementEvent>(
      "enhancement-automation-status",
      (event) => {
        const { currentScreen: nextScreen, message, state } = event.payload;
        setCurrentScreen(nextScreen);
        setLogs((prev) => [...prev, { time: timestamp(), message }]);
        if (state.includes("Idle") || state.includes("Finished") || state.includes("Error")) {
          setRunning(false);
        }
      }
    );
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    logEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [logs]);

  const selectedServant = useMemo(
    () =>
      servants.find((servant) => servant.variantKey === selectedVariantKey) ??
      servants[0] ??
      null,
    [servants, selectedVariantKey]
  );

  const handleStart = useCallback(() => {
    if (!selectedServant) return;
    setLogs([]);
    setCurrentScreen("");
    setRunning(true);
    onAutomationStart?.();
    invoke("start_enhancement_automation", {
      config: {
        targetServantId: selectedServant.id,
        targetServantVariantKey: selectedServant.variantKey,
      },
    }).catch((err) => {
      setLogs((prev) => [
        ...prev,
        { time: timestamp(), message: `启动失败: ${String(err)}` },
      ]);
      onLogEntry?.(`启动失败: ${String(err)}`);
      setRunning(false);
    });
  }, [onAutomationStart, onLogEntry, selectedServant]);

  const handleStop = useCallback(() => {
    invoke("stop_enhancement_automation").catch(console.error);
  }, []);

  return (
    <Flex direction="column" className="battle-page">
      <Flex align="center" gap="3" className="battle-header">
        <Button variant="soft" color="gray" onClick={onBack}>
          <ChevronLeftIcon width={16} height={16} />
          <Text size="2">返回</Text>
        </Button>
        <Text size="4" weight="bold">
          强化从者
        </Text>
      </Flex>

      <Flex direction="column" gap="4" className="battle-body">
        <Box>
          <Text size="2" weight="medium" style={{ marginBottom: 6, display: "block" }}>
            目标从者
          </Text>
          <Button
            type="button"
            variant="surface"
            color="gray"
            className="enhancement-servant-trigger"
            onClick={() => setDialogOpen(true)}
            disabled={running}
          >
            {selectedServant
              ? `${selectedServant.name_cn} / ${selectedServant.class} / ${selectedServant.rarity} 星`
              : "选择从者"}
          </Button>
        </Box>

        <Box className="enhancement-summary">
          <Text size="2" color="gray">
            当前页面：{currentScreen || "等待启动"}
          </Text>
          {selectedServant && (
            <Text size="2" color="gray">
              目标：{selectedServant.name_cn} / {selectedServant.class} / {selectedServant.rarity} 星
            </Text>
          )}
        </Box>

        <Flex gap="3" className="battle-controls">
          <Button
            disabled={running || !selectedServant}
            onClick={handleStart}
          >
            开始
          </Button>
          <Button
            color="red"
            variant="soft"
            disabled={!running}
            onClick={handleStop}
          >
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
      <ServantSelectDialog
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        onSelect={(servant) => setSelectedVariantKey(servant.variantKey)}
        servants={servants}
      />
    </Flex>
  );
}
