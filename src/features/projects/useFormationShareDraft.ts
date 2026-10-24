import { useCallback, useEffect, useRef, useState } from "react";
import type { FormationSharePreview } from "../../types/formationShare";
import { formationShareApi } from "./formationShareApi";

function discard(token: string | null) {
  if (token) void formationShareApi.discard(token).catch(console.error);
}

/** A preview token belongs to exactly one revision of the input draft. */
export function useFormationShareDraft(onPreview: (preview: FormationSharePreview) => void) {
  const [attempt, setAttempt] = useState(0);
  const [text, setText] = useState("");
  const [preview, setPreview] = useState<FormationSharePreview | null>(null);
  const [busy, setBusy] = useState<"preview" | "import" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const revision = useRef(0);
  const token = useRef<string | null>(null);
  const importing = useRef(false);

  const updateText = useCallback((value: string) => {
    revision.current += 1;
    discard(token.current);
    token.current = null;
    setPreview(null);
    setError(null);
    setText(value);
    setBusy(value.trim() ? "preview" : null);
  }, []);

  useEffect(() => {
    if (!text.trim()) return;
    const current = revision.current;
    const timer = window.setTimeout(() => {
      void formationShareApi.preview(text).then((result) => {
        if (current !== revision.current) {
          discard(result.token);
          return;
        }
        token.current = result.token;
        setPreview(result);
        onPreview(result);
        setBusy(null);
      }).catch((error) => {
        if (current === revision.current) {
          setError(String(error));
          setBusy(null);
        }
      });
    }, 300);
    return () => window.clearTimeout(timer);
  }, [text, attempt, onPreview]);

  useEffect(() => () => {
    revision.current += 1;
    discard(token.current);
  }, []);

  const importProject = async (name: string, groupId: string | null) => {
    if (!preview || preview.token !== token.current || !name.trim() || importing.current || busy !== null) return null;
    importing.current = true;
    setBusy("import");
    setError(null);
    try {
      const project = await formationShareApi.import(preview.token, name, groupId);
      token.current = null;
      return project;
    } catch (error) {
      setError(String(error));
      return null;
    } finally {
      importing.current = false;
      setBusy(null);
    }
  };

  const retry = () => {
    updateText(text);
    setAttempt((value) => value + 1);
  };

  return { text, preview, busy, error, updateText, importProject, retry };
}
