import { useEffect, useState } from "react";
import { Button, Code, Flex, Text } from "@radix-ui/themes";
import { DiamondSwitch } from "../../components/common/DiamondSwitch";
import { invoke } from "../../tauri";
import type { McpClientConfigPart, McpServerStatus } from "../../types/mcp";
import { SettingsRow } from "./SettingsRow";

const copyStatus: Record<McpClientConfigPart, string> = {
  address: "已复制连接地址",
  token: "已复制访问令牌",
  codex: "已复制 Codex 配置",
};

export function SettingsMcpPage() {
  const [status, setStatus] = useState<McpServerStatus | null>(null);
  const [allowBattleRunsOnStart, setAllowBattleRunsOnStart] = useState(false);
  const [busy, setBusy] = useState(false);
  const [copied, setCopied] = useState<McpClientConfigPart | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    void invoke<McpServerStatus>("get_mcp_server_status").then(
      (current) => { if (active) setStatus(current); },
      (reason: unknown) => { if (active) setError(String(reason)); },
    );
    return () => { active = false; };
  }, []);

  const setServerRunning = async (enabled: boolean) => {
    setBusy(true);
    setError(null);
    setCopied(null);
    try {
      const next = enabled
        ? await invoke<McpServerStatus>("start_mcp_server", { allowBattleRuns: allowBattleRunsOnStart })
        : await invoke<McpServerStatus>("stop_mcp_server");
      setStatus(next);
      setAllowBattleRunsOnStart(false);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  };

  const setBattlePermission = async (allowBattleRuns: boolean) => {
    if (!status?.endpoint) {
      setAllowBattleRunsOnStart(allowBattleRuns);
      setError(null);
      return;
    }
    setBusy(true);
    setError(null);
    setCopied(null);
    try {
      setStatus(await invoke<McpServerStatus>("set_mcp_battle_permission", { allowBattleRuns }));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  };

  const copyConfig = async (part: McpClientConfigPart) => {
    setBusy(true);
    setError(null);
    setCopied(null);
    try {
      await invoke("copy_mcp_client_config", { part });
      setCopied(part);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Flex direction="column" className="settings-section-panel settings-basic settings-basic-group">
      <SettingsRow
        label="本地 MCP 服务"
        description="允许本机客户端查询资料和创建编队。首次配置后可在服务重启时继续使用。"
      >
        <DiamondSwitch
          aria-label="本地 MCP 服务"
          checked={Boolean(status?.endpoint)}
          disabled={busy || status == null}
          onCheckedChange={(enabled) => { void setServerRunning(enabled); }}
        />
      </SettingsRow>
      {status && (
        <SettingsRow
          label="允许运行项目"
          description="允许运行已保存的项目，使用 Mash 当前的 ADB 设置及项目配置的 AP 回复道具（包括圣晶石）。"
        >
          <DiamondSwitch
            aria-label="允许运行项目"
            checked={status.endpoint ? status.allowBattleRuns : allowBattleRunsOnStart}
            disabled={busy}
            onCheckedChange={(enabled) => { void setBattlePermission(enabled); }}
          />
        </SettingsRow>
      )}
      {status?.endpoint && (
        <>
          <SettingsRow label="连接地址" description={<Code>{status.endpoint}</Code>}>
            <Button type="button" size="2" variant="soft" disabled={busy} onClick={() => { void copyConfig("address"); }}>
              复制地址
            </Button>
          </SettingsRow>
          <SettingsRow label="访问令牌" description={<Code aria-label="令牌已隐藏">••••••••••••</Code>}>
            <Button type="button" size="2" variant="soft" disabled={busy} onClick={() => { void copyConfig("token"); }}>
              复制令牌
            </Button>
          </SettingsRow>
          <Flex justify={{ initial: "start", sm: "end" }}>
            <Button type="button" size="2" variant="soft" disabled={busy} onClick={() => { void copyConfig("codex"); }}>
              复制 Codex 配置
            </Button>
          </Flex>
        </>
      )}
      {copied && <Text role="status" size="2">{copyStatus[copied]}</Text>}
      {error && <Text role="alert" size="2" color="red">{error}</Text>}
    </Flex>
  );
}
