import { useCallback, useEffect, useRef, useState } from "react";
import { Button, DataList, Dialog, Flex, Text } from "@radix-ui/themes";
import type { DeviceResolutionResetResult, DeviceResolutionStatus, PixelSize } from "../../types/deviceResolution";
import { getDeviceResolution, resetDeviceResolution } from "./deviceResolutionApi";

interface DeviceResolutionDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onLogEntry?: (message: string) => void;
}

function sizeLabel(size: PixelSize): string {
  return `${size.width} × ${size.height}`;
}

export function DeviceResolutionDialog({ open, onOpenChange, onLogEntry }: DeviceResolutionDialogProps) {
  const [status, setStatus] = useState<DeviceResolutionStatus | null>(null);
  const [result, setResult] = useState<DeviceResolutionResetResult | null>(null);
  const [busy, setBusy] = useState<"read" | "reset" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const request = useRef(0);

  const refresh = useCallback(async () => {
    const current = ++request.current;
    setBusy("read");
    setStatus(null);
    setResult(null);
    setError(null);
    try {
      const next = await getDeviceResolution();
      if (current === request.current) setStatus(next);
    } catch (error) {
      if (current === request.current) setError(String(error));
    } finally {
      if (current === request.current) setBusy(null);
    }
  }, []);

  useEffect(() => {
    if (open) void refresh();
    return () => { request.current += 1; };
  }, [open, refresh]);

  const reset = async () => {
    if (!status || busy) return;
    const current = ++request.current;
    setBusy("reset");
    setError(null);
    setResult(null);
    try {
      const next = await resetDeviceResolution(status.serial);
      if (current !== request.current) return;
      setResult(next);
      setStatus(next.after);
      onLogEntry?.(`设备 ${next.before.serial}：${next.message}`);
    } catch (error) {
      if (current !== request.current) return;
      setStatus(null);
      setError(String(error));
      onLogEntry?.(`重置设备分辨率失败：${String(error)}`);
    } finally {
      if (current === request.current) setBusy(null);
    }
  };

  return (
    <Dialog.Root open={open} onOpenChange={(next) => { if (busy !== "reset") onOpenChange(next); }}>
      <Dialog.Content maxWidth="520px">
        <Dialog.Title>重置设备分辨率</Dialog.Title>
        <Dialog.Description size="2" mb="4">清除所选设备主屏幕的分辨率覆盖，恢复设备默认尺寸。</Dialog.Description>
        <Flex direction="column" gap="4">
          {status && (
            <DataList.Root>
              <DataList.Item><DataList.Label>设备</DataList.Label><DataList.Value>{status.serial}</DataList.Value></DataList.Item>
              <DataList.Item><DataList.Label>显示屏</DataList.Label><DataList.Value>主屏幕（{status.displayId}）</DataList.Value></DataList.Item>
              <DataList.Item><DataList.Label>默认尺寸</DataList.Label><DataList.Value>{sizeLabel(status.physicalSize)}</DataList.Value></DataList.Item>
              <DataList.Item><DataList.Label>分辨率覆盖</DataList.Label><DataList.Value>{status.overrideSize ? sizeLabel(status.overrideSize) : "无"}</DataList.Value></DataList.Item>
              <DataList.Item><DataList.Label>当前尺寸</DataList.Label><DataList.Value>{sizeLabel(status.effectiveSize)}</DataList.Value></DataList.Item>
            </DataList.Root>
          )}
          {result && (
            <Flex direction="column" gap="1">
              <Text size="2" color={result.verified ? "green" : "amber"} role={result.verified ? "status" : "alert"}>{result.message}</Text>
              <Text size="2" color="gray">重置前：{sizeLabel(result.before.effectiveSize)}；重置后：{result.after ? sizeLabel(result.after.effectiveSize) : "未能读取"}</Text>
            </Flex>
          )}
          {error && <Text size="2" color="red" role="alert">{error}</Text>}
          <Flex gap="3" justify="end">
            <Button variant="soft" color="gray" disabled={busy !== null} loading={busy === "read"} onClick={() => void refresh()}>刷新</Button>
            <Button disabled={busy !== null || !status?.overrideSize} loading={busy === "reset"} onClick={() => void reset()}>重置分辨率</Button>
            <Dialog.Close><Button variant="soft" color="gray" disabled={busy === "reset"}>关闭</Button></Dialog.Close>
          </Flex>
        </Flex>
      </Dialog.Content>
    </Dialog.Root>
  );
}
