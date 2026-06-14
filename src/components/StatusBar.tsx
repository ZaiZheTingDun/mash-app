import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { Box, Flex, Text, Popover, Button, Spinner, Checkbox, Select, IconButton } from "@radix-ui/themes";
import {
  HamburgerMenuIcon,
  Link2Icon,
  DesktopIcon,
  MagnifyingGlassIcon,
  MoonIcon,
  SunIcon,
  Cross1Icon,
  GearIcon,
} from "@radix-ui/react-icons";
import { invoke, listen } from "../tauri";
import { SERVER_LABELS, type Server } from "../types/server";
import type { AppTheme, AppThemePreference } from "../types/theme";

type LogLevel = "info" | "debug";

interface OperationLogEntry {
  time: string;
  message: string;
  level: LogLevel;
}

interface AdbStatus {
  connected: boolean;
  deviceName: string | null;
}

interface AutomationStatusEvent {
  state: string;
}

const POLL_INTERVAL_MS = 3000;

interface StatusBarProps {
  onOpenSettings?: () => void;
  /** Wired by `App.tsx` to switch the main view to the CV debug page.
   * Optional so existing tests (and any callers that don't need the
   * shortcut) can still mount `<StatusBar />` with no props. */
  onOpenDebug?: () => void;
  theme?: AppTheme;
  themePreference?: AppThemePreference;
  onThemeChange?: (theme: AppThemePreference) => void;
  operationLogs?: OperationLogEntry[];
  operationLogOpen?: boolean;
  onOperationLogOpenChange?: (open: boolean) => void;
  updateAvailable?: boolean;
  updateChecking?: boolean;
  updateInstalling?: boolean;
  updateProgressText?: string | null;
  onInstallUpdate?: () => void;
}

export function StatusBar({
  onOpenSettings,
  onOpenDebug,
  theme,
  themePreference,
  onThemeChange,
  operationLogs = [],
  operationLogOpen = false,
  onOperationLogOpenChange,
  updateAvailable = false,
  updateChecking = false,
  updateInstalling = false,
  updateProgressText,
  onInstallUpdate,
}: StatusBarProps = {}) {
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
  // Debug entries (CV anchor positions, swipe distances, …) are
  // hidden by default to keep the operation log readable during a
  // normal run; flip this toggle to surface them for triage.
  const [showDebugLogs, setShowDebugLogs] = useState(false);
  const logEndRef = useRef<HTMLDivElement>(null);
  const runnerRunning = battleRunnerRunning || enhancementRunnerRunning;

  const visibleOperationLogs = useMemo(
    () =>
      showDebugLogs
        ? operationLogs
        : operationLogs.filter((entry) => entry.level !== "debug"),
    [operationLogs, showDebugLogs],
  );
  // Show the count of user-facing (info) entries even when debug is on;
  // the trigger label is meant to track "what the runner is doing", not
  // raw event volume.
  const infoLogCount = useMemo(
    () => operationLogs.filter((entry) => entry.level !== "debug").length,
    [operationLogs],
  );

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

  const handleThemeToggle = useCallback(() => {
    if (!theme || !onThemeChange) return;
    const currentPreference = themePreference ?? theme;
    const nextPreference =
      currentPreference === "light"
        ? "dark"
        : currentPreference === "dark"
          ? "system"
          : "light";
    onThemeChange(nextPreference);
  }, [theme, themePreference, onThemeChange]);

  useEffect(() => {
    if (!operationLogOpen) return;
    logEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [visibleOperationLogs, operationLogOpen]);

  return (
    <>
      {operationLogOpen && (
        <Box className="operation-log-panel">
          <Flex align="center" justify="between" className="operation-log-header">
            <Text size="1" weight="bold">操作日志</Text>
            <Flex align="center" gap="2">
              <label className="operation-log-debug-toggle">
                <Checkbox
                  size="1"
                  checked={showDebugLogs}
                  onCheckedChange={(checked) => setShowDebugLogs(checked === true)}
                />
                <Text size="1">显示调试</Text>
              </label>
              <IconButton
                variant="ghost"
                color="gray"
                aria-label="关闭操作日志"
                onClick={() => onOperationLogOpenChange?.(false)}
              >
                <Cross1Icon width={15} height={15} />
              </IconButton>
            </Flex>
          </Flex>
          <Box className="operation-log-list">
            {visibleOperationLogs.length === 0 && (
              <Text size="2" className="operation-log-placeholder">
                {operationLogs.length === 0 ? "等待启动…" : "没有可见日志（开启显示调试查看更多）"}
              </Text>
            )}
            {visibleOperationLogs.map((entry, index) => (
              <div
                key={index}
                className={
                  entry.level === "debug"
                    ? "operation-log-entry operation-log-entry--debug"
                    : "operation-log-entry"
                }
              >
                <span className="operation-log-time">{entry.time}</span>
                <span className="operation-log-msg">{entry.message}</span>
              </div>
            ))}
            <div ref={logEndRef} />
          </Box>
        </Box>
      )}
      <Flex className="status-bar" align="center" justify="between" gap="2">
        <Flex align="center" gap="2">
          {onOpenSettings && (
            <button
              type="button"
              className="status-settings-btn"
              onClick={onOpenSettings}
            >
              <GearIcon width={14} height={14} />
              <Text size="1">设置</Text>
            </button>
          )}
          <button
            type="button"
            className="status-log-btn"
            aria-pressed={operationLogOpen}
            onClick={() => onOperationLogOpenChange?.(!operationLogOpen)}
          >
            <HamburgerMenuIcon width={14} height={14} />
            <Text size="1">
              操作日志{infoLogCount > 0 ? ` (${infoLogCount})` : ""}
            </Text>
          </button>
        </Flex>

        <Flex align="center" justify="end" gap="2">
          {updateAvailable && (
            <button
              type="button"
              className="status-update-btn"
              disabled={updateInstalling || updateChecking}
              onClick={onInstallUpdate}
            >
              {updateInstalling || updateChecking ? <Spinner size="1" /> : null}
              <Text size="1">
                {updateInstalling
                  ? updateProgressText ?? "更新中…"
                  : updateChecking
                    ? "检测中…"
                    : "更新"}
              </Text>
            </button>
          )}
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
          {theme && onThemeChange && (
            <button
              type="button"
              className="status-theme-btn"
              aria-label={`切换主题模式，当前${
                themePreference === "system"
                  ? "跟随系统"
                  : theme === "dark"
                    ? "深色"
                    : "浅色"
              }`}
              onClick={handleThemeToggle}
            >
              {themePreference === "system" ? (
                <DesktopIcon width={12} height={12} />
              ) : theme === "dark" ? (
                <MoonIcon width={12} height={12} />
              ) : (
                <SunIcon width={12} height={12} />
              )}
              <Text size="1">
                {themePreference === "system"
                  ? "系统"
                  : theme === "dark"
                    ? "深色"
                    : "浅色"}
              </Text>
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
      </Flex>
    </>
  );
}
