import { useCallback, useEffect, useState } from "react";
import { Box, Button, Flex, Text } from "@radix-ui/themes";
import { invoke } from "../tauri";
import { SelfCheckContent } from "./SelfCheckDialog";
import type { SelfCheckStatus } from "../types/selfCheck";

interface SettingsSelfCheckPageProps {
  active: boolean;
}

export function SettingsSelfCheckPage({ active }: SettingsSelfCheckPageProps) {
  const [loading, setLoading] = useState(false);
  const [status, setStatus] = useState<SelfCheckStatus | null>(null);
  const [error, setError] = useState<string | null>(null);

  const runSelfCheck = useCallback(async () => {
    setLoading(true);
    setStatus(null);
    setError(null);
    try {
      const next = await invoke<SelfCheckStatus>("get_self_check_status");
      setStatus(next);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (active && !loading && !status && !error) {
      void runSelfCheck();
    }
  }, [active, error, loading, runSelfCheck, status]);

  return (
    <Flex direction="column" gap="4">
      <Flex align="start" justify="between" gap="3">
        <Box>
          <Text size="2" color="gray" className="setup-subtitle">
            检查应用版本信息和资源包状态
          </Text>
        </Box>
        <Button
          type="button"
          variant="soft"
          color="gray"
          onClick={runSelfCheck}
          disabled={loading}
        >
          重新自检
        </Button>
      </Flex>
      <Box className="settings-section-panel">
        <Flex direction="column" gap="4">
          <SelfCheckContent loading={loading} status={status} error={error} />
        </Flex>
      </Box>
    </Flex>
  );
}
