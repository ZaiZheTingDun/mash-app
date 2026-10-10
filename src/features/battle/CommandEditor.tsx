import type { MysticCodeGender } from "../../types/appUiSettings";
import { useEffect, useState } from "react";
import { Text } from "@radix-ui/themes";
import { AdvancedCommandEditor } from "../advanced/AdvancedCommandEditor";
import { CommandWorkspace, type CommandStep } from "./CommandWorkspace";
import { useCommandDocument } from "./useCommandDocument";
import { BattleSceneBlock } from "./BattleSceneBlock";
import {
  deriveTurnPartyMembers,
  partyMembersToServants,
  toPartyMembers,
  type PartyMember,
} from "../team/partyServants";
import type { BattleScene, BattleTurn } from "../../types/command";
import type { GrandCardStrategy, GrandClass, GrandClassDefinition, GrandServantConfig } from "../../types/project";
import type { Servant } from "../../types/servant";
import type { MysticCode } from "../../types/mysticCode";

interface CommandEditorProps {
  projectId: string | null;
  onBusyChange?: (busy: boolean) => void;
  partyLineup: (Servant | null)[];
  partyMembers?: PartyMember[];
  advancedMode?: boolean;
  disableAutoSkillTargetRecognition?: boolean;
  mysticCode?: MysticCode | null;
  homeMasterCode?: MysticCode | null;
  mysticCodeGender?: MysticCodeGender;
  grandServants?: GrandServantConfig[];
  grandClass?: GrandClass;
  grandClassDefinition?: GrandClassDefinition;
  grandCardStrategy?: GrandCardStrategy;
  grandCardPriorityEnabled?: boolean;
  turnAttackModesEnabled?: boolean;
  onGrandServantsChange?: (grandServants: GrandServantConfig[]) => void;
  onGrandCardStrategyChange?: (strategy: GrandCardStrategy) => void;
}

export function CommandEditor({
  projectId,
  onBusyChange,
  partyLineup,
  partyMembers,
  advancedMode = false,
  disableAutoSkillTargetRecognition = false,
  mysticCode = null,
  homeMasterCode = null,
  mysticCodeGender = "female",
  grandServants = [],
  grandClassDefinition,
  grandCardStrategy,
  grandCardPriorityEnabled = false,
  turnAttackModesEnabled = false,
  onGrandServantsChange,
  onGrandCardStrategyChange,
}: CommandEditorProps) {
  const [step, setStep] = useState<CommandStep>("prep");
  const editor = useCommandDocument<BattleScene>(advancedMode ? null : projectId, false);
  useEffect(() => {
    if (advancedMode) return;
    onBusyChange?.(editor.busy || !editor.document);
    return () => onBusyChange?.(false);
  }, [advancedMode, editor.busy, editor.document, onBusyChange]);
  const initialPartyMembers = partyMembers ?? toPartyMembers(partyLineup);

  if (advancedMode) {
    return (
      <AdvancedCommandEditor
        onBusyChange={onBusyChange}
        projectId={projectId}
        partyLineup={partyLineup}
        partyMembers={initialPartyMembers}
        disableAutoSkillTargetRecognition={disableAutoSkillTargetRecognition}
        mysticCode={mysticCode} homeMasterCode={homeMasterCode} mysticCodeGender={mysticCodeGender}
        grandServants={grandServants}
        grandClassDefinition={grandClassDefinition}
        grandCardStrategy={grandCardStrategy}
        grandCardPriorityEnabled={grandCardPriorityEnabled}
        onGrandServantsChange={onGrandServantsChange}
        onGrandCardStrategyChange={onGrandCardStrategyChange}
      />
    );
  }

  const document = editor.document;
  if (!document) return <Text color={editor.error ? "red" : "gray"}>{editor.error ?? (projectId ? "加载指令…" : "请先选择队伍")}</Text>;
  const { scenes, wave: activeIndex, turn: activeTurnIndex } = document;
  const activeScene = scenes[activeIndex];
  const activeTurn = activeScene.turns[activeTurnIndex];
  const activePartyMembers = deriveTurnPartyMembers(initialPartyMembers, scenes, activeIndex, activeTurnIndex);
  const configured = (turn: BattleTurn) => Boolean(turn.preparationActions?.length || turn.enemyTarget || turn.attackPriority.some(card => card.card) || turn.attackMode !== "normal");
  return <CommandWorkspace
    wave={activeIndex} waveCount={scenes.length} turn={activeTurnIndex} turns={activeScene.turns}
    step={step} onStep={setStep} busy={editor.busy} error={editor.error} canUndo={document.canUndo}
    onWave={wave => editor.navigate(wave, 0)} onTurn={turn => editor.navigate(activeIndex, turn)}
    onAddWave={() => void editor.mutate({ type: "addWave" })} onDeleteWave={() => void editor.mutate({ type: "deleteWave" })}
    onAddTurn={() => void editor.mutate({ type: "addTurn" })} onDeleteTurn={() => void editor.mutate({ type: "deleteTurn" })}
    onUndo={() => void editor.mutate({ type: "undo" })}
    configuredWave={activeScene.turns.some(configured)} configuredTurn={configured(activeTurn)}
  >
    <BattleSceneBlock key={`${activeScene.id}:${activeTurn.id}:${step}`} scene={activeTurn}
      partyServants={partyMembersToServants(activePartyMembers)} partyMembers={activePartyMembers}
      disableAutoSkillTargetRecognition={disableAutoSkillTargetRecognition} mysticCode={mysticCode} homeMasterCode={homeMasterCode} mysticCodeGender={mysticCodeGender}
      turnAttackModesEnabled={turnAttackModesEnabled} step={step}
      onChange={turn => void editor.mutate({ type: "updateTurn", turn })} />
  </CommandWorkspace>;
}
