import { useState, useEffect, useCallback } from "react";
import { Flex } from "@radix-ui/themes";
import { invoke } from "@tauri-apps/api/core";
import { BattleSceneBlock } from "./BattleSceneBlock";
import type { BattleScene, AttackCard } from "../types/command";
import type { Servant } from "../types/servant";

interface CommandEditorProps {
  projectId: string | null;
  partyServants: (Servant | null)[];
}

let nextSceneId = 1;

function createSceneId(): string {
  return `scene_${nextSceneId++}_${Date.now()}`;
}

function createDefaultAttackPriority(): AttackCard[] {
  return [
    { id: `atk_${Date.now()}_0`, card: null },
    { id: `atk_${Date.now()}_1`, card: null },
    { id: `atk_${Date.now()}_2`, card: null },
  ];
}

function createDefaultScene(): BattleScene {
  return {
    id: createSceneId(),
    servantActions: [],
    equipmentActions: [],
    commandSpellActions: [],
    attackPriority: createDefaultAttackPriority(),
  };
}

export function CommandEditor({ projectId, partyServants }: CommandEditorProps) {
  const [scenes, setScenes] = useState<BattleScene[]>([]);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    if (!projectId) {
      setScenes([createDefaultScene()]);
      setLoaded(true);
      return;
    }
    setLoaded(false);
    invoke<BattleScene[]>("load_battle_scenes", { projectId })
      .then((saved) => {
        if (saved.length > 0) {
          setScenes(saved);
        } else {
          setScenes([createDefaultScene()]);
        }
      })
      .catch(() => {
        setScenes([createDefaultScene()]);
      })
      .finally(() => setLoaded(true));
  }, [projectId]);

  const saveScenes = useCallback(
    (updated: BattleScene[]) => {
      if (!projectId) return;
      invoke("save_battle_scenes", { projectId, scenes: updated }).catch(console.error);
    },
    [projectId]
  );

  const handleSceneChange = useCallback(
    (sceneId: string, updatedScene: BattleScene) => {
      setScenes((prev) => {
        const next = prev.map((s) => (s.id === sceneId ? updatedScene : s));
        saveScenes(next);
        return next;
      });
    },
    [saveScenes]
  );

  const handleDeleteScene = useCallback(
    (sceneId: string) => {
      setScenes((prev) => {
        const next = prev.filter((s) => s.id !== sceneId);
        saveScenes(next);
        return next;
      });
    },
    [saveScenes]
  );

  const handleAddScene = useCallback(() => {
    setScenes((prev) => {
      const next = [...prev, createDefaultScene()];
      saveScenes(next);
      return next;
    });
  }, [saveScenes]);

  if (!loaded) return null;

  return (
    <Flex direction="column" gap="4" style={{ flex: 1 }}>
      {scenes.map((scene, index) => (
        <BattleSceneBlock
          key={scene.id}
          scene={scene}
          index={index}
          partyServants={partyServants}
          onChange={(updated) => handleSceneChange(scene.id, updated)}
          onDelete={() => handleDeleteScene(scene.id)}
          canDelete={scenes.length > 1}
        />
      ))}
      <button className="add-scene-btn" onClick={handleAddScene}>
        + 添加新场景
      </button>
    </Flex>
  );
}
