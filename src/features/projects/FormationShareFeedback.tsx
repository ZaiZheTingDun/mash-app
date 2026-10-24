import { Button, Dialog, Flex, Text, TextArea } from "@radix-ui/themes";
import type { useFormationShare } from "./useFormationShare";

export function FormationShareFeedback({ message, error, content, close }: ReturnType<typeof useFormationShare>) {
  return <>
    {error && <Text size="1" color="red" role="alert">{error}</Text>}
    {content && <Dialog.Root open onOpenChange={(open) => { if (!open) close(); }}>
      <Dialog.Content maxWidth="560px">
        <Dialog.Title>分享编队</Dialog.Title>
        <Dialog.Description size="2" mb="3">
          {content.copied ? "分享码已复制，也可以选中下方文本手动复制。" : "未能写入剪贴板，请选中下方完整文本后复制。"}
        </Dialog.Description>
        <TextArea aria-label="可手动复制的分享码" className="formation-share-code" readOnly value={content.code} rows={5} onFocus={(event) => event.target.select()} />
        <Flex justify="between" align="center" gap="3" mt="3">
          {message && <Text size="1" color="gray" role="status">{message}</Text>}
          <Button type="button" ml="auto" onClick={close}>关闭</Button>
        </Flex>
      </Dialog.Content>
    </Dialog.Root>}
  </>;
}
