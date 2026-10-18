import { useCallback, useEffect, useState } from "react";
import { Box, Button, Dialog, Flex, Spinner, Text } from "@radix-ui/themes";
import { PlusIcon, ReloadIcon } from "@radix-ui/react-icons";
import { OptionCardRadioGroup } from "../../components/common/OptionCardRadioGroup";
import { invoke } from "../../tauri";
import type { EmulatorMode, EmulatorOptions } from "../../types/emulator";

interface EmulatorSelectorProps {
  disabled: boolean;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSupportChange: (supported: boolean) => void;
}

/** Windows-only installation picker. Rust owns discovery, validation and selection. */
export function EmulatorSelector({ disabled, open, onOpenChange, onSupportChange }: EmulatorSelectorProps) {
  const [options, setOptions] = useState<EmulatorOptions | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      const result = await invoke<EmulatorOptions>("get_emulator_options");
      onSupportChange(result.supported);
      setOptions(result);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  }, [onSupportChange]);

  useEffect(() => {
    invoke<boolean>("supports_emulator_selection").then(onSupportChange).catch(() => {});
  }, [onSupportChange]);
  useEffect(() => { if (open) void load(); }, [open, load]);

  const choose = async (mode: EmulatorMode, path?: string) => {
    setBusy(true);
    setError(null);
    try {
      await invoke("select_emulator", { mode, path: path ?? null });
      setOptions(await invoke<EmulatorOptions>("get_emulator_options"));
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  };

  const add = async () => {
    setBusy(true);
    setError(null);
    try {
      const result = await invoke<EmulatorOptions | null>("add_mumu_installation");
      if (result) setOptions(result);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  };

  const allowOtherOnce = async () => {
    setBusy(true);
    setError(null);
    try {
      await invoke("allow_other_emulator_once");
      setOptions(await invoke<EmulatorOptions>("get_emulator_options"));
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  };

  const choices = [
    { value: "auto", title: "自动选择", description: "根据当前 ADB 设备识别 MuMu 安装目录" },
    ...(options?.installations.map((path) => ({
      value: `mumu:${path}`,
      title: "MuMu",
      description: <span className="emulator-choice-path">{path}</span>,
      ariaLabel: `MuMu ${path}`,
    })) ?? []),
    { value: "other", title: "其他模拟器", description: "使用普通模拟器画面" },
  ];

  const selectedValue = options?.mode === "mumu" && options.selectedPath
    ? `mumu:${options.selectedPath}`
    : options?.mode ?? "auto";

  const selectChoice = (value: string) => {
    const path = options?.installations.find((candidate) => `mumu:${candidate}` === value);
    if (path) void choose("mumu", path);
    else if (value === "auto" || value === "other") void choose(value);
  };

  return (
      <Dialog.Root open={open} onOpenChange={onOpenChange}>
        <Dialog.Content maxWidth="620px" className="emulator-dialog">
          <Flex align="center" justify="between" gap="3" mb="3">
            <Box>
              <Dialog.Title>选择模拟器</Dialog.Title>
              <Dialog.Description size="2" color="gray">选择 MuMu 安装目录；首次使用会自动识别。</Dialog.Description>
            </Box>
            <Button size="1" variant="soft" color="gray" disabled={busy || disabled} onClick={() => void load()}>
              {busy ? <Spinner size="1" /> : <ReloadIcon width={14} height={14} />}重新扫描
            </Button>
          </Flex>
          <Flex direction="column" gap="2">
            {busy && !options && <Text size="2" color="gray">正在扫描模拟器安装目录…</Text>}
            <OptionCardRadioGroup
              value={selectedValue}
              options={choices}
              disabled={busy || disabled || !options}
              className="emulator-choices"
              onValueChange={selectChoice}
            />
            {options?.installations.length === 0 && <Text size="2" color="gray">未自动发现 MuMu 安装目录，可以手动添加。</Text>}
            <Button size="1" variant="soft" disabled={busy || disabled} onClick={() => void add()}>
              <PlusIcon width={14} height={14} />添加 MuMu 安装目录
            </Button>
            {options?.mode === "other" && (
              <Flex align="center" gap="2" wrap="wrap">
                <Button size="1" variant="soft" color="orange" disabled={busy || disabled || options.otherOverrideReady}
                  onClick={() => void allowOtherOnce()}>本次跳过 MuMu 检查</Button>
                {options.otherOverrideReady && <Text size="1" color="gray">已允许下一次启动；请重新启动任务</Text>}
              </Flex>
            )}
            {options?.scanError && <Text size="1" color="orange">自动扫描失败：{options.scanError}</Text>}
            {error && <Text size="2" color="red">{error}</Text>}
          </Flex>
        </Dialog.Content>
      </Dialog.Root>
  );
}
