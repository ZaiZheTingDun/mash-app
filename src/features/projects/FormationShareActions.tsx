import { useRef, useState } from "react";
import { Button, Dialog, Flex, Spinner, Text, TextArea } from "@radix-ui/themes";
import { Share1Icon } from "@radix-ui/react-icons";
import type { FormationShareCode, FormationShareIntegration } from "../../types/formationShare";
import type { ProjectGroup } from "../../types/project";
import { FormationShareImportButton } from "./FormationShareImportButton";
import { formationShareApi } from "./formationShareApi";

interface Props extends FormationShareIntegration {
  projectId: string | null;
  groups: ProjectGroup[];
  disabled?: boolean;
}

export function FormationShareActions({ projectId, groups, disabled = false, beforeShare, onFormationImported }: Props) {
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [fallback, setFallback] = useState<FormationShareCode | null>(null);
  const pending = useRef(false);

  const share = async () => {
    if (!projectId || pending.current) return;
    pending.current = true;
    setBusy(true); setMessage(null); setError(null);
    try {
      await beforeShare?.(projectId);
      const result = await formationShareApi.create(projectId);
      if (result.copied) setMessage(`编队分享码已复制（${result.characters} 字符）`);
      else setFallback(result);
    } catch (error) {
      setError(String(error));
    } finally {
      pending.current = false;
      setBusy(false);
    }
  };

  return <>
    <Button type="button" variant="soft" color="gray" disabled={disabled || busy || !projectId} onClick={() => void share()}>
      {busy ? <Spinner size="1" /> : <Share1Icon />}分享
    </Button>
    {onFormationImported && <FormationShareImportButton groups={groups} disabled={disabled || busy} onImported={onFormationImported} />}
    {message && <Text size="1" color="gray" role="status">{message}</Text>}
    {error && <Text size="1" color="red" role="alert">{error}</Text>}
    {fallback && <Dialog.Root open onOpenChange={(open) => { if (!open) setFallback(null); }}>
      <Dialog.Content maxWidth="560px">
        <Dialog.Title>手动复制分享码</Dialog.Title>
        <Dialog.Description size="2" mb="3">未能写入剪贴板，请选中下方完整文本后复制。</Dialog.Description>
        <TextArea aria-label="可手动复制的分享码" className="formation-share-code" readOnly value={fallback.code} rows={5} onFocus={(event) => event.target.select()} />
        <Flex justify="end" mt="3"><Button type="button" onClick={() => setFallback(null)}>关闭</Button></Flex>
      </Dialog.Content>
    </Dialog.Root>}
  </>;
}
