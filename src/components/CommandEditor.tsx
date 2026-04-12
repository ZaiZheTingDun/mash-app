import { useState, useEffect, useCallback } from "react";
import { Flex } from "@radix-ui/themes";
import { invoke } from "@tauri-apps/api/core";
import { TurnBlock } from "./TurnBlock";
import type { Turn, AttackCard } from "../types/command";
import type { Servant } from "../types/servant";

interface CommandEditorProps {
  partyServants: (Servant | null)[];
}

let nextTurnId = 1;

function createTurnId(): string {
  return `turn_${nextTurnId++}_${Date.now()}`;
}

function createDefaultAttackPriority(): AttackCard[] {
  return [
    { id: `atk_${Date.now()}_0`, card: null },
    { id: `atk_${Date.now()}_1`, card: null },
    { id: `atk_${Date.now()}_2`, card: null },
  ];
}

function createDefaultTurn(): Turn {
  return {
    id: createTurnId(),
    servantActions: [],
    equipmentActions: [],
    attackPriority: createDefaultAttackPriority(),
  };
}

export function CommandEditor({ partyServants }: CommandEditorProps) {
  const [turns, setTurns] = useState<Turn[]>([]);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    invoke<Turn[]>("load_turns")
      .then((saved) => {
        if (saved.length > 0) {
          setTurns(saved);
        } else {
          setTurns([createDefaultTurn()]);
        }
      })
      .catch(() => {
        setTurns([createDefaultTurn()]);
      })
      .finally(() => setLoaded(true));
  }, []);

  const saveTurns = useCallback((updated: Turn[]) => {
    invoke("save_turns", { turns: updated }).catch(console.error);
  }, []);

  const handleTurnChange = useCallback(
    (turnId: string, updatedTurn: Turn) => {
      setTurns((prev) => {
        const next = prev.map((t) => (t.id === turnId ? updatedTurn : t));
        saveTurns(next);
        return next;
      });
    },
    [saveTurns]
  );

  const handleDeleteTurn = useCallback(
    (turnId: string) => {
      setTurns((prev) => {
        const next = prev.filter((t) => t.id !== turnId);
        saveTurns(next);
        return next;
      });
    },
    [saveTurns]
  );

  const handleAddTurn = useCallback(() => {
    setTurns((prev) => {
      const next = [...prev, createDefaultTurn()];
      saveTurns(next);
      return next;
    });
  }, [saveTurns]);

  if (!loaded) return null;

  return (
    <Flex direction="column" gap="4" style={{ flex: 1 }}>
      {turns.map((turn, index) => (
        <TurnBlock
          key={turn.id}
          turn={turn}
          index={index}
          partyServants={partyServants}
          onChange={(updated) => handleTurnChange(turn.id, updated)}
          onDelete={() => handleDeleteTurn(turn.id)}
          canDelete={turns.length > 1}
        />
      ))}
      <button className="add-turn-btn" onClick={handleAddTurn}>
        + Add New Turn
      </button>
    </Flex>
  );
}
