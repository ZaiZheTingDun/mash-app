import { useState, useEffect, useCallback } from "react";
import { Flex, IconButton, Text } from "@radix-ui/themes";
import { invoke } from "../../tauri";
import { AdvancedCommandEditor } from "../advanced/AdvancedCommandEditor";
import { TurnHelpTooltip } from "../../components/common/TurnHelpTooltip";
import { BattleSceneBlock } from "./BattleSceneBlock";
import {
  deriveTurnPartyMembers,
  partyMembersToServants,
  toPartyMembers,
  type PartyMember,
} from "../team/partyServants";
import {
  ChevronLeftIcon,
  ChevronRightIcon,
  PlusIcon,
  TrashIcon,
} from "@radix-ui/react-icons";
import type { BattleScene, BattleTurn, AttackCard } from "../../types/command";
import type { GrandCardStrategy, GrandClass, GrandClassDefinition, GrandServantConfig } from "../../types/project";
import type { Servant } from "../../types/servant";

interface CommandEditorProps {
  projectId: string | null;
  partyLineup: (Servant | null)[];
  partyMembers?: PartyMember[];
  advancedMode?: boolean;
  disableAutoSkillTargetRecognition?: boolean;
  grandServants?: GrandServantConfig[];
  grandClass?: GrandClass;
  grandClassDefinition?: GrandClassDefinition;
  grandCardStrategy?: GrandCardStrategy;
  grandCardPriorityEnabled?: boolean;
  turnAttackModesEnabled?: boolean;
  onGrandServantsChange?: (grandServants: GrandServantConfig[]) => void;
  onGrandCardStrategyChange?: (strategy: GrandCardStrategy) => void;
}

let nextSceneId = 1;
let nextTurnId = 1;
const FIXED_ATTACK_CARD_COUNT = 3;

function createSceneId(): string {
  return `scene_${nextSceneId++}_${Date.now()}`;
}

function createTurnId(): string {
  return `turn_${nextTurnId++}_${Date.now()}`;
}

function createDefaultAttackPriority(): AttackCard[] {
  return Array.from({ length: FIXED_ATTACK_CARD_COUNT }, (_, index) => ({
    id: `atk_fixed_${index + 1}_${Date.now()}`,
    card: null,
  }));
}

function createDefaultTurn(): BattleTurn {
  return {
    id: createTurnId(),
    preparationActions: [],
    servantActions: [],
    equipmentActions: [],
    commandSpellActions: [],
    enemyTarget: null,
    attackPriority: createDefaultAttackPriority(),
    attackMode: "normal",
    criticalStrategy: {
      memberPriority: [],
      chainPriority: ["mighty", "buster", "arts", "quick"],
    },
    advancedCardStrategy: { customRules: [] },
  };
}

function createDefaultScene(): BattleScene {
  return {
    id: createSceneId(),
    turns: [createDefaultTurn()],
  };
}

function normalizeTurn(turn: BattleTurn): BattleTurn {
  const attackPriority = [...(turn.attackPriority ?? [])];
  while (attackPriority.length < FIXED_ATTACK_CARD_COUNT) {
    attackPriority.push({
      id: `atk_fixed_${attackPriority.length + 1}_${Date.now()}`,
      card: null,
    });
  }
  return {
    ...turn,
    preparationActions:
      turn.preparationActions ??
      [
        ...(turn.servantActions ?? []),
        ...(turn.equipmentActions ?? []),
        ...(turn.commandSpellActions ?? []),
      ],
    servantActions: [],
    equipmentActions: [],
    commandSpellActions: [],
    enemyTarget: turn.enemyTarget ?? null,
    attackPriority,
    attackMode: turn.attackMode ?? "normal",
    criticalStrategy: {
      memberPriority: turn.criticalStrategy?.memberPriority ?? [],
      chainPriority: turn.criticalStrategy?.chainPriority ?? ["mighty", "buster", "arts", "quick"],
    },
    advancedCardStrategy: {
      customRules: turn.advancedCardStrategy?.customRules ?? [],
    },
  };
}

function normalizeScene(scene: BattleScene): BattleScene {
  const legacyTurn =
    scene.preparationActions ||
      scene.servantActions ||
      scene.equipmentActions ||
      scene.commandSpellActions ||
      scene.attackPriority ||
      scene.enemyTarget != null
      ? [
        normalizeTurn({
          id: `${scene.id}_turn_1`,
          preparationActions: scene.preparationActions ?? [],
          servantActions: scene.servantActions ?? [],
          equipmentActions: scene.equipmentActions ?? [],
          commandSpellActions: scene.commandSpellActions ?? [],
          enemyTarget: scene.enemyTarget ?? null,
          attackPriority: scene.attackPriority ?? [],
        }),
      ]
      : [];
  const turns = (scene.turns?.length ? scene.turns : legacyTurn).map(normalizeTurn);
  return {
    id: scene.id,
    turns: turns.length > 0 ? turns : [createDefaultTurn()],
  };
}

export function CommandEditor({
  projectId,
  partyLineup,
  partyMembers,
  advancedMode = false,
  disableAutoSkillTargetRecognition = false,
  grandServants = [],
  grandClassDefinition,
  grandCardStrategy,
  grandCardPriorityEnabled = false,
  turnAttackModesEnabled = false,
  onGrandServantsChange,
  onGrandCardStrategyChange,
}: CommandEditorProps) {
  const [scenes, setScenes] = useState<BattleScene[]>(() =>
    projectId ? [] : [createDefaultScene()]
  );
  const [loaded, setLoaded] = useState(() => !projectId);
  const [activeIndex, setActiveIndex] = useState(0);
  const [activeTurnIndex, setActiveTurnIndex] = useState(0);
  const initialPartyMembers = partyMembers ?? toPartyMembers(partyLineup);

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
        setActiveTurnIndex(0);
      })
      .catch(() => {
        if (cancelled) return;
        setScenes([createDefaultScene()]);
        setActiveIndex(0);
        setActiveTurnIndex(0);
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

  const handleDeleteScene = useCallback(
    (sceneId: string) => {
      setScenes((prev) => {
        const next = prev.filter((s) => s.id !== sceneId);
        const fallback = next.length > 0 ? next : [createDefaultScene()];
        saveScenes(next);
        setActiveIndex((current) => Math.min(current, fallback.length - 1));
        setActiveTurnIndex(0);
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
      setActiveTurnIndex(0);
      return next;
    });
  }, [saveScenes]);

  const handleTurnChange = useCallback(
    (sceneId: string, turnId: string, updatedTurn: BattleTurn) => {
      setScenes((prev) => {
        const next = prev.map((scene) =>
          scene.id === sceneId
            ? {
              ...scene,
              turns: scene.turns.map((turn) =>
                turn.id === turnId ? updatedTurn : turn
              ),
            }
            : scene
        );
        saveScenes(next);
        return next;
      });
    },
    [saveScenes]
  );

  const handleAddTurn = useCallback(() => {
    setScenes((prev) => {
      const next = prev.map((scene, index) => {
        if (index !== activeIndex) return scene;
        const turns = [...scene.turns, createDefaultTurn()];
        setActiveTurnIndex(turns.length - 1);
        return { ...scene, turns };
      });
      saveScenes(next);
      return next;
    });
  }, [activeIndex, saveScenes]);

  const handleDeleteTurn = useCallback(() => {
    setScenes((prev) => {
      const next = prev.map((scene, index) => {
        if (index !== activeIndex || activeTurnIndex === 0) return scene;
        const turns = scene.turns.filter((_, turnIndex) => turnIndex !== activeTurnIndex);
        const fallbackTurns = turns.length > 0 ? turns : [createDefaultTurn()];
        setActiveTurnIndex(Math.max(0, activeTurnIndex - 1));
        return { ...scene, turns: fallbackTurns };
      });
      saveScenes(next);
      return next;
    });
  }, [activeIndex, activeTurnIndex, saveScenes]);

  if (advancedMode) {
    return (
      <AdvancedCommandEditor
        projectId={projectId}
        partyLineup={partyLineup}
        partyMembers={initialPartyMembers}
        disableAutoSkillTargetRecognition={disableAutoSkillTargetRecognition}
        grandServants={grandServants}
        grandClassDefinition={grandClassDefinition}
        grandCardStrategy={grandCardStrategy}
        grandCardPriorityEnabled={grandCardPriorityEnabled}
        onGrandServantsChange={onGrandServantsChange}
        onGrandCardStrategyChange={onGrandCardStrategyChange}
      />
    );
  }

  if (!loaded) return null;
  const activeScene = scenes[activeIndex] ?? scenes[0] ?? createDefaultScene();
  const activeTurn = activeScene.turns[activeTurnIndex] ?? activeScene.turns[0] ?? createDefaultTurn();
  const activePartyMembers = deriveTurnPartyMembers(
    initialPartyMembers,
    scenes,
    activeIndex,
    activeTurnIndex
  );
  const activeParty = partyMembersToServants(activePartyMembers);

  return (
    <Flex direction="column" className="command-editor">
      <Flex align="center" justify="center" gap="3" className="battle-scene-nav">
        <IconButton
          type="button"
          variant="surface"
          color="gray"
          aria-label="上一场战斗"
          disabled={activeIndex === 0}
          onClick={() => {
            setActiveIndex((index) => Math.max(0, index - 1));
            setActiveTurnIndex(0);
          }}
        >
          <ChevronLeftIcon width={18} height={18} />
        </IconButton>
        <Text size="4" weight="bold">
          第 {activeIndex + 1}/{scenes.length} 面
        </Text>
        <IconButton
          type="button"
          variant="surface"
          color="gray"
          aria-label="下一场战斗"
          disabled={activeIndex >= scenes.length - 1}
          onClick={() => {
            setActiveIndex((index) => Math.min(scenes.length - 1, index + 1));
            setActiveTurnIndex(0);
          }}
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
      <Flex align="center" justify="between" gap="3" className="battle-turn-nav">
        <Flex align="center" gap="2" wrap="wrap">
          <Text size="2" weight="bold">当前面轮次</Text>
          <TurnHelpTooltip />
          {activeScene.turns.map((turn, index) => (
            <button
              type="button"
              key={turn.id}
              className={`battle-turn-tab${activeTurn.id === turn.id ? " is-selected" : ""}`}
              aria-label={`Turn ${index + 1}`}
              onClick={() => setActiveTurnIndex(index)}
            >
              {index + 1}
            </button>
          ))}
          <IconButton
            type="button"
            variant="surface"
            color="gray"
            aria-label="添加 Turn"
            onClick={handleAddTurn}
          >
            <PlusIcon width={16} height={16} />
          </IconButton>
        </Flex>
        <IconButton
          type="button"
          variant="surface"
          color="red"
          aria-label="删除当前 Turn"
          disabled={activeTurnIndex === 0}
          onClick={handleDeleteTurn}
        >
          <TrashIcon width={16} height={16} />
        </IconButton>
      </Flex>
      <div className="command-scroll-region">
        <BattleSceneBlock
          key={`${activeScene.id}:${activeTurn.id}`}
          scene={activeTurn}
          partyServants={activeParty}
          partyMembers={activePartyMembers}
          disableAutoSkillTargetRecognition={disableAutoSkillTargetRecognition}
          turnAttackModesEnabled={turnAttackModesEnabled}
          onChange={(updated) => handleTurnChange(activeScene.id, activeTurn.id, updated)}
        />
      </div>
    </Flex>
  );
}
