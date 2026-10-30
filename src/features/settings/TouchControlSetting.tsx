import { useCallback, useEffect, useState } from "react";
import { Button, Flex, Select, Text } from "@radix-ui/themes";
import { invoke } from "../../tauri";
import type { TouchControlMode } from "../../types/deviceControl";
import { SettingsRow } from "./SettingsRow";

export function TouchControlSetting({ active }: { active: boolean }) {
  const [mode, setMode] = useState<TouchControlMode>();
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string>();
  const [saved, setSaved] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    setError(undefined);
    try {
      setMode(await invoke<TouchControlMode>("get_touch_control_mode"));
    } catch (reason) {
      setError(String(reason));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (active) void load();
  }, [active, load]);

  const save = async (value: TouchControlMode) => {
    setSaving(true);
    setError(undefined);
    setSaved(false);
    try {
      setMode(await invoke<TouchControlMode>("set_touch_control_mode", { value }));
      setSaved(true);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setSaving(false);
    }
  };

  return (
    <SettingsRow label="触控方式" description="触控异常时可切回旧方式，下次任务开始生效"
      help={<>
        {saved && <Text size="1" color="green" role="status">已保存，下次任务开始生效</Text>}
        {error && <Flex gap="2" align="center">
          <Text size="1" color="red" role="alert">{error}</Text>
          {!mode && <Button size="1" variant="ghost" onClick={() => void load()}>重试读取</Button>}
        </Flex>}
      </>}>
      <Select.Root value={mode ?? ""} disabled={loading || saving || !mode}
        onValueChange={(value) => void save(value as TouchControlMode)}>
        <Select.Trigger aria-label="触控方式" className="recognition-mode-select" placeholder="选择触控方式" />
        <Select.Content position="popper" className="settings-select-menu">
          <Select.Item value="scrcpy">scrcpy</Select.Item>
          <Select.Item value="adb">ADB Input（旧版）</Select.Item>
        </Select.Content>
      </Select.Root>
    </SettingsRow>
  );
}
