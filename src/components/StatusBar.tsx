import { useState, useEffect, useCallback } from "react";
import { Flex, Text, Popover, Button, Spinner, Checkbox } from "@radix-ui/themes";
import { Link2Icon, DesktopIcon } from "@radix-ui/react-icons";
import { invoke } from "@tauri-apps/api/core";

interface AdbStatus {
  connected: boolean;
  deviceName: string | null;
}

const POLL_INTERVAL_MS = 3000;

export function StatusBar() {
  const [status, setStatus] = useState<AdbStatus>({ connected: false, deviceName: null });
  const [checking, setChecking] = useState(false);
  const [useBluestack, setUseBluestack] = useState(false);

  useEffect(() => {
    invoke<boolean>("get_use_bluestack").then(setUseBluestack).catch(() => {});
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

  return (
    <Flex className="status-bar" align="center" justify="end">
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
