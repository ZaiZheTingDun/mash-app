import { useRef, useState } from "react";
import type { FormationShareCode } from "../../types/formationShare";
import { formationShareApi } from "./formationShareApi";

export function useFormationShare(
  projectId: string | null,
  beforeShare?: (projectId: string) => Promise<void>,
) {
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [content, setContent] = useState<FormationShareCode | null>(null);
  const pending = useRef(false);

  const share = async () => {
    if (!projectId || pending.current) return;
    pending.current = true;
    setBusy(true); setMessage(null); setError(null);
    try {
      await beforeShare?.(projectId);
      const result = await formationShareApi.create(projectId);
      if (result.copied) setMessage(`编队分享码已复制（${result.characters} 字符）`);
      setContent(result);
    } catch (error) {
      setError(String(error));
    } finally {
      pending.current = false;
      setBusy(false);
    }
  };

  return { busy, share, message, error, content, close: () => setContent(null) };
}
