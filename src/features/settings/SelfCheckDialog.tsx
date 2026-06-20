import { Badge, Box, Button, DataList, Dialog, Flex, Spinner, Text } from "@radix-ui/themes";
import type { SelfCheckAssetGroup, SelfCheckStatus } from "../../types/selfCheck";

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
  classifier,
  value,
}: {
  label: string;
  classifier: string;
  value: SelfCheckAssetGroup;
}) {
  return (
    <DataList.Item align="center">
      <DataList.Label minWidth="120px">{label}</DataList.Label>
      <DataList.Value>
        <Flex align="center" justify="between" gap="5" className="self-check-asset-value">
          <Text size="2">{value.entries} {classifier}</Text>
          <Flex align="center" gap="5" wrap="wrap">
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
          </Flex>
        </Flex>
      </DataList.Value>
    </DataList.Item>
  );
}

export function SelfCheckContent({
  loading,
  status,
  error,
}: {
  loading: boolean;
  status: SelfCheckStatus | null;
  error: string | null;
}) {
  if (loading) {
    return (
      <Flex align="center" gap="2">
        <Spinner size="1" />
        <Text size="2">正在自检...</Text>
      </Flex>
    );
  }

  if (error) {
    return (
      <Text size="2" color="red">
        自检失败：{error}
      </Text>
    );
  }

  if (!status) {
    return null;
  }

  return (
    <>
      <Box>
        <Text as="div" size="2" weight="bold" mb="2">
          版本信息
        </Text>
        <DataList.Root>
          <DataList.Item>
            <DataList.Label minWidth="120px">软件版本</DataList.Label>
            <DataList.Value>{status.appVersion}</DataList.Value>
          </DataList.Item>
          <DataList.Item align="center">
            <DataList.Label minWidth="120px">CV runtime</DataList.Label>
            <DataList.Value>
              <Flex align="center" justify="between" gap="4" className="self-check-data-value">
                <Text size="2">{status.cvRuntimeVersion}</Text>
                {installBadge(status.cvRuntimeInstalled)}
              </Flex>
            </DataList.Value>
          </DataList.Item>
          <DataList.Item align="center">
            <DataList.Label minWidth="120px">CV code</DataList.Label>
            <DataList.Value>
              <Flex align="center" justify="between" gap="4" className="self-check-data-value">
                <Text size="2">{status.cvCodeVersion}</Text>
                {installBadge(status.cvCodeInstalled)}
              </Flex>
            </DataList.Value>
          </DataList.Item>
          <DataList.Item>
            <DataList.Label minWidth="120px">资源包版本</DataList.Label>
            <DataList.Value>
              {versionLabel(status.assetVersion)} / 目标 v{status.appAssetsVersion}
            </DataList.Value>
          </DataList.Item>
        </DataList.Root>
      </Box>

      <Box>
        <Text as="div" size="2" weight="bold" mb="2">
          资源包
        </Text>
        <DataList.Root>
          <AssetGroupRow label="从者" classifier="骑" value={status.servants} />
          <AssetGroupRow label="概念礼装" classifier="张" value={status.ces} />
        </DataList.Root>
      </Box>
    </>
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
          <SelfCheckContent loading={loading} status={status} error={error} />

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
