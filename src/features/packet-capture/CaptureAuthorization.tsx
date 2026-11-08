import { Button, Dialog, Flex, Text, TextField } from "@radix-ui/themes";
import { useState } from "react";
import type { CaptureAuthorizationPrompt } from "../../types/packetCapture";

export function CaptureAuthorization({ prompt, submit, open, onOpenChange }: {
  prompt: CaptureAuthorizationPrompt;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  submit: (requestId: string, key: string) => Promise<void>;
}) {
  const [draft, setDraft] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  return <Dialog.Root open={open} onOpenChange={next => {
    if (!next) { setDraft(""); setError(null); }
    onOpenChange(next);
  }}>
    <Dialog.Content maxWidth="420px">
      <Dialog.Title>手动输入 API key</Dialog.Title>
      <Dialog.Description size="2" mb="3">{prompt.message}</Dialog.Description>
      <form aria-label="API key 授权" onSubmit={event => {
        event.preventDefault();
        if (pending) return;
        setPending(true);
        setError(null);
        void submit(prompt.requestId, draft).then(() => setDraft(""))
          .catch(error => setError(String(error))).finally(() => setPending(false));
      }}>
        <Flex direction="column" gap="3">
          <TextField.Root type="password" aria-label="Reqable API key" autoComplete="off" value={draft} disabled={pending || !prompt.acceptingInput} onChange={event => setDraft(event.target.value)} />
          {error && <Text size="2" color="red" role="alert">{error}</Text>}
          <Flex gap="3" justify="end">
            <Dialog.Close><Button type="button" variant="soft" color="gray">关闭</Button></Dialog.Close>
            <Button type="submit" size="2" disabled={pending || !prompt.acceptingInput || !draft.trim()}>验证并继续</Button>
          </Flex>
        </Flex>
      </form>
    </Dialog.Content>
  </Dialog.Root>;
}
