import { Badge, Box, Button, Dialog, Flex, Grid, Spinner, Text } from "@radix-ui/themes";
import type { SelfCheckAssetGroup, SelfCheckStatus } from "../types/selfCheck";

interface SelfCheckDialogProps {
  open: boolean;
  loading: boolean;
  status: SelfCheckStatus | null;
  error: string | null;
  onOpenChange: (open: boolean) => void;
}

function versionLabel(value: number | null): string {
  return value == null ? "未安装" : `v${value}`;
}

function healthBadge(ok: boolean): JSX.Element {
  return (
    <Badge color={ok ? "green" : "red"} variant="soft">
      {ok ? "有" : "无"}
    </Badge>
  );
}

function installBadge(installed: boolean): JSX.Element {
  return (
    <Badge color={installed ? "green" : "amber"} variant="soft">
      {installed ? "已安装" : "未安装"}
    </Badge>
  );
}

function AssetGroupRow({
  label,
  value,
}: {
  label: string;
  value: SelfCheckAssetGroup;
}) {
  return (
    <Grid columns="120px 1fr 1fr 1fr" gap="3" align="center" className="self-check-row">
      <Text size="2" weight="medium">
        {label}
      </Text>
      <Text size="2">{value.entries} 个</Text>
      <Flex align="center" gap="2">
        <Text size="2" color="gray">
          图片
        </Text>
        {healthBadge(value.hasImage)}
      </Flex>
      <Flex align="center" gap="2">
        <Text size="2" color="gray">
          JSON
        </Text>
        {healthBadge(value.hasJson)}
      </Flex>
    </Grid>
  );
}

export function SelfCheckDialog({
  open,
  loading,
  status,
  error,
  onOpenChange,
}: SelfCheckDialogProps) {
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Content maxWidth="560px">
        <Dialog.Title size="4">自检结果</Dialog.Title>
        <Flex direction="column" gap="4">
          {loading ? (
            <Flex align="center" gap="2">
              <Spinner size="1" />
              <Text size="2">正在自检...</Text>
            </Flex>
          ) : error ? (
            <Text size="2" color="red">
              自检失败：{error}
            </Text>
          ) : status ? (
            <>
              <Box>
                <Text as="div" size="2" weight="bold" mb="2">
                  版本信息
                </Text>
                <Grid columns="140px 1fr auto" gap="3" align="center">
                  <Text size="2" color="gray">
                    软件版本
                  </Text>
                  <Text size="2">{status.appVersion}</Text>
                  <Box />
                  <Text size="2" color="gray">
                    CV runtime
                  </Text>
                  <Text size="2">{status.cvRuntimeVersion}</Text>
                  {installBadge(status.cvRuntimeInstalled)}
                  <Text size="2" color="gray">
                    CV code
                  </Text>
                  <Text size="2">{status.cvCodeVersion}</Text>
                  {installBadge(status.cvCodeInstalled)}
                  <Text size="2" color="gray">
                    资源包版本
                  </Text>
                  <Text size="2">
                    {versionLabel(status.assetVersion)} / 目标 v{status.appAssetsVersion}
                  </Text>
                  <Box />
                </Grid>
              </Box>

              <Box>
                <Text as="div" size="2" weight="bold" mb="2">
                  资源包
                </Text>
                <Flex direction="column" gap="2">
                  <AssetGroupRow label="servants" value={status.servants} />
                  <AssetGroupRow label="ces" value={status.ces} />
                </Flex>
              </Box>
            </>
          ) : null}

          <Flex justify="end">
            <Dialog.Close>
              <Button type="button" variant="soft" color="gray">
                关闭
              </Button>
            </Dialog.Close>
          </Flex>
        </Flex>
      </Dialog.Content>
    </Dialog.Root>
  );
}
