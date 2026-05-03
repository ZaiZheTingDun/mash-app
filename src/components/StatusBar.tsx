import { useState, useEffect, useCallback } from "react";
import { Flex, Text, Popover, Button, Spinner, Checkbox, Select } from "@radix-ui/themes";
import {
  Link2Icon,
  DesktopIcon,
  MagnifyingGlassIcon,
} from "@radix-ui/react-icons";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { SERVER_LABELS, type Server } from "../types/server";

interface AdbStatus {
  connected: boolean;
  deviceName: string | null;
}

interface AutomationStatusEvent {
  state: string;
}

const POLL_INTERVAL_MS = 3000;

interface StatusBarProps {
  /** Wired by `App.tsx` to switch the main view to the CV debug page.
   * Optional so existing tests (and any callers that don't need the
   * shortcut) can still mount `<StatusBar />` with no props. */
  onOpenDebug?: () => void;
}

export function StatusBar({ onOpenDebug }: StatusBarProps = {}) {
  const [status, setStatus] = useState<AdbStatus>({ connected: false, deviceName: null });
  const [checking, setChecking] = useState(false);
  const [useBluestack, setUseBluestack] = useState(false);
  const [server, setServer] = useState<Server>("JP");
  // The server selector must be locked while the runner is mid-run: the
  // sidecar already pinned templates / OCR for the previous server when
  // it spawned, so flipping the global setting now would silently
  // desync. Both battle automation and servant-enhancement automation
  // pin server-specific OCR/templates, so either one should lock the
  // selector until it exits.
  const [battleRunnerRunning, setBattleRunnerRunning] = useState(false);
  const [enhancementRunnerRunning, setEnhancementRunnerRunning] = useState(false);
  const runnerRunning = battleRunnerRunning || enhancementRunnerRunning;

  useEffect(() => {
    invoke<boolean>("get_use_bluestack").then(setUseBluestack).catch(() => {});
    invoke<Server>("get_server").then(setServer).catch(() => {});
  }, []);

  const pollAdb = useCallback(() => {
    invoke<AdbStatus>("check_adb")
      .then(setStatus)
      .catch(() => setStatus({ connected: false, deviceName: null }));
  }, []);

  useEffect(() => {
    pollAdb();
    const id = setInterval(pollAdb, POLL_INTERVAL_MS);
    return () => clearInterval(id);
  }, [pollAdb]);

  useEffect(() => {
    const unlistenBattle = listen<AutomationStatusEvent>(
      "automation-status",
      (event) => {
        const state = event.payload.state ?? "";
        setBattleRunnerRunning(state.includes("Running"));
      }
    );
    const unlistenEnhancement = listen<AutomationStatusEvent>(
      "enhancement-automation-status",
      (event) => {
        const state = event.payload.state ?? "";
        setEnhancementRunnerRunning(state.includes("Running"));
      }
    );
    return () => {
      unlistenBattle.then((fn) => fn());
      unlistenEnhancement.then((fn) => fn());
    };
  }, []);

  const handleConnect = useCallback(() => {
    setChecking(true);
    invoke<AdbStatus>("check_adb")
      .then(setStatus)
      .catch(() => setStatus({ connected: false, deviceName: null }))
      .finally(() => setChecking(false));
  }, []);

  const handleBluestackToggle = useCallback((checked: boolean) => {
    setUseBluestack(checked);
    invoke("set_use_bluestack", { value: checked }).catch(() => {});
  }, []);

  const handleServerChange = useCallback((value: string) => {
    if (value !== "JP" && value !== "CN") return;
    const next = value as Server;
    const previous = server;
    // Optimistic update; revert + log on backend rejection (e.g. the
    // runner started between this render and the IPC round-trip).
    setServer(next);
    invoke("set_server", { value: next }).catch((err) => {
      console.error("set_server failed", err);
      setServer(previous);
    });
  }, [server]);

  return (
    <Flex className="status-bar" align="center" justify="end" gap="2">
      {onOpenDebug && (
        <button
          type="button"
          className="status-debug-btn"
          onClick={onOpenDebug}
        >
          <MagnifyingGlassIcon width={12} height={12} />
          <Text size="1">CV 调试</Text>
        </button>
      )}
      <Popover.Root>
        <Popover.Trigger>
          <button className="status-trigger">
            <span
              className={`status-dot ${status.connected ? "connected" : "disconnected"}`}
            />
            <Text size="1" className="status-label">
              {status.connected ? "游戏已连接" : "游戏未连接"}
            </Text>
          </button>
        </Popover.Trigger>
        <Popover.Content side="top" align="end" size="1" className="status-popover">
          <Flex direction="column" gap="3">
            <Flex align="center" justify="between" gap="2">
              <Text size="2">服务器</Text>
              <Select.Root
                size="1"
                value={server}
                onValueChange={handleServerChange}
                disabled={runnerRunning}
              >
                <Select.Trigger aria-label="服务器" />
                <Select.Content>
                  <Select.Item value="JP">{SERVER_LABELS.JP}</Select.Item>
                  <Select.Item value="CN">{SERVER_LABELS.CN}</Select.Item>
                </Select.Content>
              </Select.Root>
            </Flex>

            <label className="bluestack-checkbox">
              <Checkbox
                size="1"
                checked={useBluestack}
                onCheckedChange={(checked) => handleBluestackToggle(checked === true)}
              />
              <Text size="2">使用 BlueStacks 模拟器</Text>
            </label>

            {status.connected ? (
              <Flex align="center" gap="2">
                <DesktopIcon width={14} height={14} style={{ color: "var(--gray-10)", flexShrink: 0 }} />
                <Text size="2" weight="medium">{status.deviceName}</Text>
              </Flex>
            ) : (
              <Flex direction="column" gap="2">
                <Text size="2" color="gray">未检测到游戏设备</Text>
                <Button
                  size="1"
                  variant="soft"
                  disabled={checking}
                  onClick={handleConnect}
                >
                  {checking ? (
                    <Spinner size="1" />
                  ) : (
                    <Link2Icon width={14} height={14} />
                  )}
                  {checking ? "检测中…" : "连接"}
                </Button>
              </Flex>
            )}
          </Flex>
        </Popover.Content>
      </Popover.Root>
    </Flex>
  );
}
