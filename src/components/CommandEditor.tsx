import { useState, useEffect, useCallback } from "react";
import { Flex, IconButton, Text } from "@radix-ui/themes";
import { invoke } from "../tauri";
import { AdvancedCommandEditor } from "./AdvancedCommandEditor";
import { BattleSceneBlock } from "./BattleSceneBlock";
import { deriveScenePartyLineups } from "./partyServants";
import {
  ChevronLeftIcon,
  ChevronRightIcon,
  PlusIcon,
  TrashIcon,
} from "@radix-ui/react-icons";
import type { BattleScene, AttackCard } from "../types/command";
import type { GrandCardStrategy, GrandServantConfig } from "../types/project";
import type { Servant } from "../types/servant";

interface CommandEditorProps {
  projectId: string | null;
  partyLineup: (Servant | null)[];
  advancedMode?: boolean;
  grandServants?: GrandServantConfig[];
  grandCardStrategy?: GrandCardStrategy;
  grandCardPriorityEnabled?: boolean;
  onGrandServantsChange?: (grandServants: GrandServantConfig[]) => void;
  onGrandCardStrategyChange?: (strategy: GrandCardStrategy) => void;
}

let nextSceneId = 1;
const FIXED_ATTACK_CARD_COUNT = 3;

function createSceneId(): string {
  return `scene_${nextSceneId++}_${Date.now()}`;
}

function createDefaultAttackPriority(): AttackCard[] {
  return Array.from({ length: FIXED_ATTACK_CARD_COUNT }, (_, index) => ({
    id: `atk_fixed_${index + 1}_${Date.now()}`,
    card: null,
  }));
}

function createDefaultScene(): BattleScene {
  return {
    id: createSceneId(),
    preparationActions: [],
    servantActions: [],
    equipmentActions: [],
    commandSpellActions: [],
    enemyTarget: null,
    attackPriority: createDefaultAttackPriority(),
  };
}

function normalizeScene(scene: BattleScene): BattleScene {
  const attackPriority = [...(scene.attackPriority ?? [])];
  while (attackPriority.length < FIXED_ATTACK_CARD_COUNT) {
    attackPriority.push({
      id: `atk_fixed_${attackPriority.length + 1}_${Date.now()}`,
      card: null,
    });
  }
  return {
    ...scene,
    preparationActions:
      scene.preparationActions ??
      [
        ...(scene.servantActions ?? []),
        ...(scene.equipmentActions ?? []),
        ...(scene.commandSpellActions ?? []),
      ],
    servantActions: [],
    equipmentActions: [],
    commandSpellActions: [],
    enemyTarget: scene.enemyTarget ?? null,
    attackPriority,
  };
}

export function CommandEditor({
  projectId,
  partyLineup,
  advancedMode = false,
  grandServants = [],
  grandCardStrategy,
  grandCardPriorityEnabled = false,
  onGrandServantsChange,
  onGrandCardStrategyChange,
}: CommandEditorProps) {
  const [scenes, setScenes] = useState<BattleScene[]>(() =>
    projectId ? [] : [createDefaultScene()]
  );
  const [loaded, setLoaded] = useState(() => !projectId);
  const [activeIndex, setActiveIndex] = useState(0);
  const scenePartyLineups = deriveScenePartyLineups(partyLineup, scenes);

  useEffect(() => {
    if (advancedMode || !projectId) {
      return;
    }
    let cancelled = false;
    invoke<BattleScene[]>("load_battle_scenes", { projectId })
      .then((saved) => {
        if (cancelled) return;
        setScenes(saved.length > 0 ? saved.map(normalizeScene) : [createDefaultScene()]);
        setActiveIndex(0);
      })
      .catch(() => {
        if (cancelled) return;
        setScenes([createDefaultScene()]);
        setActiveIndex(0);
      })
      .finally(() => {
        if (!cancelled) {
          setLoaded(true);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [advancedMode, projectId]);

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
        const fallback = next.length > 0 ? next : [createDefaultScene()];
        saveScenes(next);
        setActiveIndex((current) => Math.min(current, fallback.length - 1));
        return fallback;
      });
    },
    [saveScenes]
  );

  const handleAddScene = useCallback(() => {
    setScenes((prev) => {
      const next = [...prev, createDefaultScene()];
      saveScenes(next);
      setActiveIndex(next.length - 1);
      return next;
    });
  }, [saveScenes]);

  if (advancedMode) {
    return (
      <AdvancedCommandEditor
        projectId={projectId}
        partyLineup={partyLineup}
        grandServants={grandServants}
        grandCardStrategy={grandCardStrategy}
        grandCardPriorityEnabled={grandCardPriorityEnabled}
        onGrandServantsChange={onGrandServantsChange}
        onGrandCardStrategyChange={onGrandCardStrategyChange}
      />
    );
  }

  if (!loaded) return null;
  const activeScene = scenes[activeIndex] ?? scenes[0] ?? createDefaultScene();
  const activeParty = scenePartyLineups[activeIndex] ?? partyLineup;

  return (
    <Flex direction="column" className="command-editor">
      <Flex align="center" justify="center" gap="3" className="battle-scene-nav">
        <IconButton
          type="button"
          variant="surface"
          color="gray"
          aria-label="上一场战斗"
          disabled={activeIndex === 0}
          onClick={() => setActiveIndex((index) => Math.max(0, index - 1))}
        >
          <ChevronLeftIcon width={18} height={18} />
        </IconButton>
        <Text size="4" weight="bold">
          Battle {activeIndex + 1} / {scenes.length}
        </Text>
        <IconButton
          type="button"
          variant="surface"
          color="gray"
          aria-label="下一场战斗"
          disabled={activeIndex >= scenes.length - 1}
          onClick={() =>
            setActiveIndex((index) => Math.min(scenes.length - 1, index + 1))
          }
        >
          <ChevronRightIcon width={18} height={18} />
        </IconButton>
        <IconButton
          type="button"
          variant="surface"
          color="gray"
          aria-label="添加 Battle"
          onClick={handleAddScene}
        >
          <PlusIcon width={16} height={16} />
        </IconButton>
        {scenes.length > 1 && (
          <IconButton
            type="button"
            variant="surface"
            color="red"
            aria-label="删除当前 Battle"
            onClick={() => handleDeleteScene(activeScene.id)}
          >
            <TrashIcon width={16} height={16} />
          </IconButton>
        )}
      </Flex>
      <div className="command-scroll-region">
        <BattleSceneBlock
          key={activeScene.id}
          scene={activeScene}
          partyServants={activeParty}
          onChange={(updated) => handleSceneChange(activeScene.id, updated)}
        />
      </div>
    </Flex>
  );
}
