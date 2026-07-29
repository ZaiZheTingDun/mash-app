import { Button, Dialog, Flex, Spinner, Text } from "@radix-ui/themes";

interface SoftwareUpdateDialogProps {
  open: boolean;
  currentVersion: string | null;
  version: string | null;
  installing: boolean;
  progressText: string | null;
  error: string | null;
  onOpenChange: (open: boolean) => void;
  onInstall: () => void;
}

export function SoftwareUpdateDialog({
  open,
  currentVersion,
  version,
  installing,
  progressText,
  error,
  onOpenChange,
  onInstall,
}: SoftwareUpdateDialogProps) {
  const handleOpenChange = (nextOpen: boolean) => {
    if (!nextOpen && installing) return;
    onOpenChange(nextOpen);
  };

  return (
    <Dialog.Root open={open} onOpenChange={handleOpenChange}>
      <Dialog.Content maxWidth="420px">
        <Dialog.Title size="4">发现软件更新</Dialog.Title>
        <Dialog.Description size="2" color="gray">
          Mash {version ?? "新版本"} 已可用
          {currentVersion ? `，当前版本为 ${currentVersion}` : ""}。
        </Dialog.Description>

        <Flex direction="column" gap="4" mt="4">
          {error && (
            <Text size="2" color="red">
              更新失败：{error}
            </Text>
          )}

          <Flex justify="end" gap="3">
            <Button
              type="button"
              variant="soft"
              color="gray"
              disabled={installing}
              onClick={() => handleOpenChange(false)}
            >
              稍后
            </Button>
            <Button type="button" disabled={installing} onClick={onInstall}>
              {installing && <Spinner size="1" />}
              {installing ? progressText ?? "更新中…" : "立即更新"}
            </Button>
          </Flex>
        </Flex>
      </Dialog.Content>
    </Dialog.Root>
  );
}
