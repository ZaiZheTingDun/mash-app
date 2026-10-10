import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "../../tauri";
import type { BattleTurn, AdvancedBattleScene, BattleScene } from "../../types/command";

export interface CommandDocument<T> { scenes: T[]; wave: number; turn: number }
export type CommandMutation = { type: "addWave" | "deleteWave" | "addTurn" | "deleteTurn" } | { type: "updateTurn"; turn: BattleTurn } | { type: "updateScene"; scene: BattleScene | AdvancedBattleScene };

export function useCommandDocument<T extends BattleScene | AdvancedBattleScene>(projectId: string | null, advanced: boolean) {
  const [document, setDocument] = useState<CommandDocument<T> | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const request = useRef(0);
  const pending = useRef(false);
  useEffect(() => {
    const token = ++request.current;
    setDocument(null); setError(null); setBusy(false); pending.current = false;
    if (!projectId) return;
    invoke<CommandDocument<T>>("load_command_editor", { projectId, advanced }).then(result => {
      if (token === request.current) setDocument(result);
    }).catch(reason => { if (token === request.current) setError(String(reason)); });
    return () => { request.current = token + 1; };
  }, [projectId, advanced]);
  const mutate = useCallback(async (mutation: CommandMutation) => {
    if (!projectId || !document || pending.current) return;
    pending.current = true; setBusy(true); setError(null);
    const token = request.current;
    try {
      const result = await invoke<CommandDocument<T>>("mutate_command_editor", { projectId, advanced, mutation, wave: document.wave, turn: document.turn });
      if (token === request.current) setDocument(result);
    } catch (reason) { if (token === request.current) setError(String(reason)); }
    finally { if (token === request.current) { pending.current = false; setBusy(false); } }
  }, [projectId, advanced, document]);
  const navigate = useCallback((wave: number, turn: number) => setDocument(previous => previous ? { ...previous, wave, turn } : previous), []);
  return { document, busy, error, mutate, navigate };
}
