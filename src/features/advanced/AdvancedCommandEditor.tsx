import { useCallback, useEffect, useMemo, useState } from "react";
import { Avatar, Dialog, Flex, Text, Button, IconButton, Select } from "@radix-ui/themes";
import {
  Cross2Icon,
  PlusIcon,
  TrashIcon,
} from "@radix-ui/react-icons";
import { invoke } from "../../tauri";
import { BattleActorIcon } from "../../components/common/BattleActorIcon";
import { battleActorLabel, servantLabel } from "../../components/common/battleActorLabels";
import { AddRowTrigger } from "../../components/common/AddRowTrigger";
import { FaceChip } from "./AdvancedFaceChip";
import { AdvancedCommandCardButton } from "./AdvancedCommandCardButton";
import { GrandOutputSettings } from "./AdvancedGrandOutputSettings";
import { GrandCardStrategyPanel } from "./AdvancedGrandStrategyPanel";
import {
  COMMAND_SPELL_LABELS,
  EMPTY_STARTUP_ACTIONS,
  SKILL_LABELS,
  createId,
  createDefaultAdvancedTurn,
  createDefaultScene,
  defaultCommandCard,
  mainGrandBackSlot,
  normalizeScene,
  prepSummary,
  type FrontServant,
  type PartySlot,
  type PrepDraft,
  type PrepSource,
} from "./advancedCommandModel";
import {
  deriveMembersAfterPreparationActions,
  memberRefAt,
  partyMembersToServants,
  resolveMemberRefIndex,
  resolvePreparationAction,
  toPartyMembers,
  type PartyMember,
} from "../team/partyServants";
import { useServantFaceImages } from "../team/useServantFaceImages";
import { useServantSkillIcons, type SkillIcons } from "../team/useServantSkillIcons";
import { SkillOptionButtons } from "../../components/common/SkillOptionButtons";
import { EnemyTargetButtons, EnemyTargetSelector } from "../battle/EnemyTargetSelector";
import { servantSlotIndex, skillSlotIndex } from "../battle/battleSceneModel";
import { useServantSkillTargeting } from "../battle/useServantSkillTargeting";
import { useServantSkillSelections } from "../battle/useServantSkillSelections";
import type {
  AdvancedBattleScene,
  AdvancedCommandCardCondition,
  CommandSpellAction,
  EnemyTargetAction,
  EquipmentAction,
  OrderChangeSelection,
  PreparationAction,
  ServantAction,
} from "../../types/command";
import type {
  GrandCardStrategy,
  GrandClass,
  GrandClassDefinition,
  GrandServantConfig,
} from "../../types/project";
import type { Servant } from "../../types/servant";
import orderChangeIcon from "../../../src-tauri/resources/images/icon_order_change.png";

interface AdvancedCommandEditorProps {
  projectId: string | null;
  partyLineup: (Servant | null)[];
  partyMembers?: PartyMember[];
  disableAutoSkillTargetRecognition?: boolean;
  grandServants?: GrandServantConfig[];
  grandClass?: GrandClass;
  grandClassDefinition?: GrandClassDefinition;
  grandCardStrategy?: GrandCardStrategy;
  grandCardPriorityEnabled?: boolean;
  onGrandServantsChange?: (grandServants: GrandServantConfig[]) => void;
  onGrandCardStrategyChange?: (strategy: GrandCardStrategy) => void;
}

function AdvancedInlineFace({
  servant,
  index,
  src,
  isSupport = false,
}: {
  servant: Servant | null;
  index: number;
  src: string | null | undefined;
  isSupport?: boolean;
}) {
  return (
    <BattleActorIcon
      kind="servant"
      src={src}
      label={servantLabel(index, servant)}
      isSupport={isSupport}
      size="inline"
    />
  );
}

function AdvancedPreparationActionSummary({
  action,
  partyMembers,
  faces,
  skillIcons,
}: {
  action: PreparationAction;
  partyMembers: PartyMember[];
  faces: Record<string, string | null>;
  skillIcons: Record<string, SkillIcons>;
}) {
  if (action.type === "enemyTarget") {
    return (
      <span className="battle-action-summary" aria-label={prepSummary(action, partyMembersToServants(partyMembers))}>
        <Text size="2" weight="medium" className="battle-action-name">
          选择敌方目标 {action.target?.replace("enemy_", "") ?? "?"}
        </Text>
      </span>
    );
  }
  const partyLineup = partyMembersToServants(partyMembers);
  const resolvedAction = resolvePreparationAction(action, partyMembers);
  // TODO: duplicate
  const targetIndex =
    action.type === "servant"
      ? resolveMemberRefIndex(
          partyMembers,
          action.target,
          action.targetMemberId,
          action.targetServantId,
          action.targetIsSupport
        )
      : action.type === "equipment"
        ? resolveMemberRefIndex(
            partyMembers,
            action.target,
            action.targetMemberId,
            action.targetServantId,
            action.targetIsSupport
          )
        : resolveMemberRefIndex(
            partyMembers,
            action.target,
            action.targetMemberId,
            action.targetServantId,
            action.targetIsSupport
          );
  const orderChangeSlots =
    action.type === "equipment" && action.orderChange
      ? {
          front: resolveMemberRefIndex(
            partyMembers,
            action.orderChange.front,
            action.orderChange.frontMemberId,
            action.orderChange.frontServantId,
            action.orderChange.frontIsSupport
          ),
          back: resolveMemberRefIndex(
            partyMembers,
            action.orderChange.back,
            action.orderChange.backMemberId,
            action.orderChange.backServantId,
            action.orderChange.backIsSupport
          ),
        }
      : null;

  let sourceFace;
  let sourceText: string;
  let actionText: string;
  let skillIconSrc: string | null = null;
  let skillLabel = "技能";
  let skillSlot = 0;

  if (resolvedAction.type === "servant") {
    const source =
      resolveMemberRefIndex(
        partyMembers,
        action.type === "servant" ? action.servant : null,
        action.type === "servant" ? action.servantMemberId : null,
        action.type === "servant" ? action.servantId : null,
        action.type === "servant" ? action.servantIsSupport : false
      ) ?? 0;
    const member = partyMembers[source] ?? { servant: null, isSupport: false };
    const servant = member.servant;
    sourceFace = (
      <AdvancedInlineFace
        servant={servant}
        index={source}
        src={servant ? faces[servant.variantKey] : null}
        isSupport={member.isSupport}
      />
    );
    sourceText = servantLabel(source, servant);
    const idx = skillSlotIndex(resolvedAction.skill);
    const skillEntry = servant && idx >= 0 ? (skillIcons[servant.variantKey]?.[idx] ?? null) : null;
    skillIconSrc = skillEntry?.src ?? null;
    skillLabel = skillEntry?.name || (SKILL_LABELS[resolvedAction.skill ?? ""] ?? "技能");
    skillSlot = idx >= 0 ? idx : 0;
    actionText = `释放 ${skillLabel}`;
    if (resolvedAction.skillSelection) {
      actionText += `并选择 ${resolvedAction.skillSelection.label ?? `选项 ${resolvedAction.skillSelection.index + 1}`}`;
    }
  } else {
    const kind = action.type === "equipment" ? "equipment" : "commandSpell";
    sourceFace = (
      <BattleActorIcon kind={kind} label={battleActorLabel({ kind })} size="inline" />
    );
    sourceText = action.type === "equipment" ? "御主礼装" : "令咒";
    actionText =
      resolvedAction.type === "equipment"
        ? `释放 ${SKILL_LABELS[resolvedAction.skill ?? ""] ?? "技能"}`
        : resolvedAction.type === "commandSpell"
          ? COMMAND_SPELL_LABELS[resolvedAction.spell ?? ""] ?? "行动"
          : "选择敌方目标";
  }

  return (
    <span className="battle-action-summary" aria-label={prepSummary(resolvedAction, partyLineup)}>
      {sourceFace}
      <Text size="2" weight="medium" className="battle-action-name">
        {sourceText}{" "}
        {resolvedAction.type === "servant"
          ? <>释放{" "}<span className="battle-inline-skill-icon" title={skillLabel}><Avatar src={skillIconSrc ?? undefined} fallback={String(skillSlot + 1)} size="1" radius="small" /></span></>
          : actionText}
        {resolvedAction.type === "servant" && resolvedAction.skillSelection
          ? `并选择 ${resolvedAction.skillSelection.label ?? `选项 ${resolvedAction.skillSelection.index + 1}`}`
          : ""}
      </Text>
      {orderChangeSlots?.front != null && orderChangeSlots.back != null ? (
        <>
          <span className="battle-action-to">Order Change</span>
          <AdvancedInlineFace
            servant={partyLineup[orderChangeSlots.front] ?? null}
            index={orderChangeSlots.front}
            src={
              partyLineup[orderChangeSlots.front]
                ? faces[partyLineup[orderChangeSlots.front]!.variantKey]
                : null
            }
            isSupport={partyMembers[orderChangeSlots.front]?.isSupport ?? false}
          />
          <Text size="2" weight="medium" className="battle-action-name">
            {servantLabel(orderChangeSlots.front, partyLineup[orderChangeSlots.front] ?? null)}
          </Text>
          <span className="battle-action-to">↔</span>
          <AdvancedInlineFace
            servant={partyLineup[orderChangeSlots.back] ?? null}
            index={orderChangeSlots.back}
            src={
              partyLineup[orderChangeSlots.back]
                ? faces[partyLineup[orderChangeSlots.back]!.variantKey]
                : null
            }
            isSupport={partyMembers[orderChangeSlots.back]?.isSupport ?? false}
          />
          <Text size="2" weight="medium" className="battle-action-name">
            {servantLabel(orderChangeSlots.back, partyLineup[orderChangeSlots.back] ?? null)}
          </Text>
        </>
      ) : (
        targetIndex != null && (
          <>
            <span className="battle-action-to">to</span>
            <AdvancedInlineFace
              servant={partyLineup[targetIndex] ?? null}
              index={targetIndex}
              src={partyLineup[targetIndex] ? faces[partyLineup[targetIndex]!.variantKey] : null}
              isSupport={partyMembers[targetIndex]?.isSupport ?? false}
            />
            <Text size="2" weight="medium" className="battle-action-name">
              {servantLabel(targetIndex, partyLineup[targetIndex] ?? null)}
            </Text>
          </>
        )
      )}
    </span>
  );
}

function AdvancedStrategyEditor({
  scene,
  partyMembers,
  faces,
  skillIcons,
  skillTargetStatus,
  skillSelection,
  disableAutoSkillTargetRecognition,
  grandServants,
  grandClassDefinition,
  grandCardStrategy,
  grandCardPriorityEnabled,
  onGrandServantsChange,
  onGrandCardStrategyChange,
  activeTurnIndex,
  onActiveTurnIndexChange,
  onChange,
}: {
  scene: AdvancedBattleScene;
  partyMembers: PartyMember[];
  faces: Record<string, string | null>;
  skillIcons: Record<string, SkillIcons>;
  skillTargetStatus: ReturnType<typeof useServantSkillTargeting>;
  skillSelection: ReturnType<typeof useServantSkillSelections>;
  disableAutoSkillTargetRecognition: boolean;
  grandServants: GrandServantConfig[];
  grandClassDefinition?: GrandClassDefinition;
  grandCardStrategy?: GrandCardStrategy;
  grandCardPriorityEnabled: boolean;
  onGrandServantsChange?: (grandServants: GrandServantConfig[]) => void;
  onGrandCardStrategyChange?: (strategy: GrandCardStrategy) => void;
  activeTurnIndex: number;
  onActiveTurnIndexChange: (index: number) => void;
  onChange: (scene: AdvancedBattleScene) => void;
}) {
  const [editingCardSlot, setEditingCardSlot] = useState<number | null>(null);
  const [controlDraft, setControlDraft] = useState<PrepDraft | null>(null);
  const [prepDraft, setPrepDraft] = useState<PrepDraft | null>(null);
  const partyLineup = useMemo(() => partyMembersToServants(partyMembers), [partyMembers]);
  const grandAutoOrderChange = scene.grandAutoOrderChange ?? null;
  const mainGrandSlot = mainGrandBackSlot(grandServants, grandClassDefinition);
  const mainGrandServant = mainGrandSlot == null ? null : partyLineup[mainGrandSlot] ?? null;
  const orderChangeConfig = grandServants.find((config) => config.slotIndex === mainGrandSlot);
  const orderChangeRole = orderChangeConfig?.role ?? orderChangeConfig?.lancerRole;
  const orderChangeLabel = `${grandClassDefinition?.roles.find((role) => role.role === orderChangeRole)?.label ?? "主"}冠位从者`;
  const startupSelectableSlots = useMemo(() => {
    const slots = [0, 1, 2];
    if (mainGrandSlot != null && grandAutoOrderChange === true) {
      slots.push(mainGrandSlot);
    }
    return slots;
  }, [grandAutoOrderChange, mainGrandSlot]);
  const commandConditions = scene.commandConditions ?? [0, 1, 2, 3, 4].map(defaultCommandCard);
  const controlActions = scene.controlActions ?? EMPTY_STARTUP_ACTIONS;
  const turns = useMemo(
    () => scene.turns?.length ? scene.turns : [createDefaultAdvancedTurn()],
    [scene.turns]
  );
  const activeTurn = turns[activeTurnIndex] ?? turns[0];
  const startupActions = activeTurn.actions;
  const previousTurnActions = useMemo(
    () => turns.slice(0, activeTurnIndex).flatMap((turn) => turn.actions),
    [activeTurnIndex, turns]
  );
  const controlActionLineups = useMemo(() => {
    const lineups: PartyMember[][] = [];
    let lineup = partyMembers;
    for (const action of controlActions) {
      lineups.push(lineup);
      lineup = deriveMembersAfterPreparationActions(lineup, [action]);
    }
    return lineups;
  }, [partyMembers, controlActions]);
  const postControlMembers = useMemo(
    () => deriveMembersAfterPreparationActions(partyMembers, controlActions),
    [partyMembers, controlActions]
  );
  const turnStartMembers = useMemo(
    () => deriveMembersAfterPreparationActions(postControlMembers, previousTurnActions),
    [postControlMembers, previousTurnActions]
  );
  const startupActionLineups = useMemo(() => {
    const lineups: PartyMember[][] = [];
    let lineup = turnStartMembers;
    for (const action of startupActions) {
      lineups.push(lineup);
      lineup = deriveMembersAfterPreparationActions(lineup, [action]);
    }
    return lineups;
  }, [turnStartMembers, startupActions]);
  const currentPartyMembers = useMemo(
    () => deriveMembersAfterPreparationActions(turnStartMembers, startupActions),
    [turnStartMembers, startupActions]
  );
  const editingCard =
    editingCardSlot == null
      ? null
      : commandConditions.find((card) => card.slot === editingCardSlot) ??
        defaultCommandCard(editingCardSlot);

  const updateCommandCard = (
    slot: number,
    updater: (card: AdvancedCommandCardCondition) => AdvancedCommandCardCondition
  ) => {
    onChange({
      ...scene,
      commandConditions: commandConditions.map((card) =>
        card.slot === slot ? updater(card) : card
      ),
    });
  };

  const updateStartupActions = (actions: PreparationAction[]) => {
    onChange({
      ...scene,
      turns: turns.map((turn) =>
        turn.id === activeTurn.id ? { ...turn, actions } : turn
      ),
      startupActions: [],
    });
  };

  const addTurn = () => {
    const nextTurns = [...turns, createDefaultAdvancedTurn()];
    onChange({ ...scene, turns: nextTurns, startupActions: [] });
    onActiveTurnIndexChange(nextTurns.length - 1);
    setPrepDraft(null);
  };

  const deleteTurn = () => {
    if (activeTurnIndex === 0) return;
    const nextTurns = turns.filter((_, index) => index !== activeTurnIndex);
    onChange({ ...scene, turns: nextTurns, startupActions: [] });
    onActiveTurnIndexChange(activeTurnIndex - 1);
    setPrepDraft(null);
  };

  const updateControlActions = (actions: PreparationAction[]) => {
    onChange({ ...scene, controlActions: actions });
  };

  const targetRef = (members: PartyMember[], target: string | null) => {
    const ref = memberRefAt(members, target);
    return {
      targetMemberId: ref.memberId,
      targetServantId: ref.servantId,
      targetIsSupport: ref.isSupport,
    };
  };

  const servantRef = (members: PartyMember[], servant: string | null) => {
    const ref = memberRefAt(members, servant);
    return {
      servantMemberId: ref.memberId,
      servantId: ref.servantId,
      servantIsSupport: ref.isSupport,
    };
  };

  const orderChangeRef = (
    members: PartyMember[],
    key: "front" | "back",
    slot: PartySlot
  ) => {
    const ref = memberRefAt(members, slot);
    return {
      [`${key}MemberId`]: ref.memberId,
      [`${key}ServantId`]: ref.servantId,
      [`${key}IsSupport`]: ref.isSupport,
    };
  };

  const makePrepAction = (
    draft: Extract<PrepDraft, { step: "target" }>,
    target: string | null,
    members: PartyMember[]
  ): PreparationAction => {
    if (draft.source === "enemyTarget") {
      return {
        type: "enemyTarget",
        id: createId("enemy_target"),
        target,
      } satisfies EnemyTargetAction;
    }
    if (draft.source === "equipment") {
      return {
        type: "equipment",
        id: createId("eq"),
        skill: draft.option,
        target,
        ...targetRef(members, target),
        orderChange: null,
      } satisfies EquipmentAction;
    }
    if (draft.source === "commandSpell") {
      return {
        type: "commandSpell",
        id: createId("cs"),
        spell: draft.option as CommandSpellAction["spell"],
        target,
        ...targetRef(members, target),
      } satisfies CommandSpellAction;
    }
    return {
      type: "servant",
      id: createId("sa"),
      servant: draft.source,
      ...servantRef(members, draft.source),
      skill: draft.option,
      skillSelection: draft.skillSelection ?? null,
      target,
      ...targetRef(members, target),
    } satisfies ServantAction;
  };

  const finishPrepAction = (
    draft: Extract<PrepDraft, { step: "target" }>,
    target: string | null
  ) => {
    updateStartupActions([...startupActions, makePrepAction(draft, target, currentPartyMembers)]);
    setPrepDraft(null);
  };

  const finishControlAction = (
    draft: Extract<PrepDraft, { step: "target" }>,
    target: string | null
  ) => {
    updateControlActions([...controlActions, makePrepAction(draft, target, postControlMembers)]);
    setControlDraft(null);
  };

  const finishOrderChangeAction = (
    draft: Extract<PrepDraft, { step: "orderChange" }>,
    back: PartySlot
  ) => {
    if (draft.front == null) return;
    updateStartupActions([
      ...startupActions,
      {
        type: "equipment",
        id: createId("eq"),
        skill: draft.option,
        target: null,
        orderChange: {
          front: draft.front,
          ...orderChangeRef(currentPartyMembers, "front", draft.front),
          back,
          ...orderChangeRef(currentPartyMembers, "back", back),
        } satisfies OrderChangeSelection,
      } satisfies EquipmentAction,
    ]);
    setPrepDraft(null);
  };

  const finishControlOrderChangeAction = (
    draft: Extract<PrepDraft, { step: "orderChange" }>,
    back: PartySlot
  ) => {
    if (draft.front == null) return;
    updateControlActions([
      ...controlActions,
      {
        type: "equipment",
        id: createId("eq"),
        skill: draft.option,
        target: null,
        orderChange: {
          front: draft.front,
          ...orderChangeRef(postControlMembers, "front", draft.front),
          back,
          ...orderChangeRef(postControlMembers, "back", back),
        } satisfies OrderChangeSelection,
      } satisfies EquipmentAction,
    ]);
    setControlDraft(null);
  };

  const selectControlSkill = (source: PrepSource, skill: string) => {
    if (source === "equipment") {
      setControlDraft({ step: "target", source, option: skill });
      return;
    }
    const servant = postControlMembers[servantSlotIndex(source) ?? 0]?.servant ?? null;
    const selection = skillSelection(servant, skill);
    if (selection) {
      setControlDraft({
        step: "skillSelection",
        source,
        option: skill,
        selectionType: selection.selectionType,
        options: selection.options,
      });
      return;
    }
    const status = disableAutoSkillTargetRecognition
      ? "unknown"
      : skillTargetStatus(servant, skill);
    const draft = { step: "target", source, option: skill } satisfies Extract<
      PrepDraft,
      { step: "target" }
    >;
    if (status === "noTarget") {
      finishControlAction(draft, null);
      return;
    }
    setControlDraft(
      status === "needsTarget"
        ? { ...draft, allowNoTarget: false }
        : status === "mixed"
          ? { ...draft, allowNoTarget: true }
          : draft
    );
  };

  const selectStartupSkill = (source: PrepSource, skill: string) => {
    if (source === "equipment") {
      setPrepDraft({ step: "target", source, option: skill });
      return;
    }
    const servant = currentPartyMembers[servantSlotIndex(source) ?? 0]?.servant ?? null;
    const selection = skillSelection(servant, skill);
    if (selection) {
      setPrepDraft({
        step: "skillSelection",
        source,
        option: skill,
        selectionType: selection.selectionType,
        options: selection.options,
      });
      return;
    }
    const status = disableAutoSkillTargetRecognition
      ? "unknown"
      : skillTargetStatus(servant, skill);
    const draft = { step: "target", source, option: skill } satisfies Extract<
      PrepDraft,
      { step: "target" }
    >;
    if (status === "noTarget") {
      finishPrepAction(draft, null);
      return;
    }
    setPrepDraft(
      status === "needsTarget"
        ? { ...draft, allowNoTarget: false }
        : status === "mixed"
          ? { ...draft, allowNoTarget: true }
          : draft
    );
  };

  const finishControlSkillSelection = (
    draft: Extract<PrepDraft, { step: "skillSelection" }>,
    option: { index: number; label: string }
  ) => {
    const targetDraft = {
      step: "target",
      source: draft.source,
      option: draft.option,
      skillSelection: {
        type: draft.selectionType,
        index: option.index,
        optionCount: draft.options.length,
        label: option.label,
      },
    } satisfies Extract<PrepDraft, { step: "target" }>;
    const servant = postControlMembers[servantSlotIndex(draft.source) ?? 0]?.servant ?? null;
    const status = disableAutoSkillTargetRecognition
      ? "unknown"
      : skillTargetStatus(servant, draft.option);
    if (status === "noTarget") {
      finishControlAction(targetDraft, null);
      return;
    }
    setControlDraft(
      status === "needsTarget"
        ? { ...targetDraft, allowNoTarget: false }
        : status === "mixed"
          ? { ...targetDraft, allowNoTarget: true }
          : targetDraft
    );
  };

  const finishStartupSkillSelection = (
    draft: Extract<PrepDraft, { step: "skillSelection" }>,
    option: { index: number; label: string }
  ) => {
    const targetDraft = {
      step: "target",
      source: draft.source,
      option: draft.option,
      skillSelection: {
        type: draft.selectionType,
        index: option.index,
        optionCount: draft.options.length,
        label: option.label,
      },
    } satisfies Extract<PrepDraft, { step: "target" }>;
    const servant = currentPartyMembers[servantSlotIndex(draft.source) ?? 0]?.servant ?? null;
    const status = disableAutoSkillTargetRecognition
      ? "unknown"
      : skillTargetStatus(servant, draft.option);
    if (status === "noTarget") {
      finishPrepAction(targetDraft, null);
      return;
    }
    setPrepDraft(
      status === "needsTarget"
        ? { ...targetDraft, allowNoTarget: false }
        : status === "mixed"
          ? { ...targetDraft, allowNoTarget: true }
          : targetDraft
    );
  };

  return (
    <div className="advanced-strategy-editor">
      <section className="battle-phase advanced-strategy-section">
        <div className="battle-phase-label">主力输出</div>
        <div className="advanced-main-output-grid">
          <span className="advanced-delete-spacer" aria-hidden />
          <GrandOutputSettings
            partyMembers={partyMembers}
            faces={faces}
            grandServants={grandServants}
            grandClassDefinition={grandClassDefinition}
            onChange={onGrandServantsChange}
          />
        </div>
      </section>

      <section className="battle-phase advanced-strategy-section">
        <div className="battle-phase-label">启动条件</div>
        <div className="advanced-condition-row">
          <span className="advanced-delete-spacer" aria-hidden />
          {mainGrandSlot != null && grandAutoOrderChange == null ? (
            <div className="advanced-grand-order-choice">
              <Text size="2" weight="medium">
                {orderChangeLabel}配置在后排，是否自动换位至前排？
              </Text>
              <div className="battle-choice-row">
                <button
                  type="button"
                  className="battle-option-btn"
                  onClick={() => onChange({ ...scene, grandAutoOrderChange: true })}
                >
                  是
                </button>
                <button
                  type="button"
                  className="battle-option-btn"
                  onClick={() => onChange({ ...scene, grandAutoOrderChange: false })}
                >
                  否
                </button>
              </div>
            </div>
          ) : mainGrandSlot != null && grandAutoOrderChange === true ? (
            <div className="advanced-grand-order-note">
              <Text size="2" weight="medium">
                第一回合会自动将
                {mainGrandServant ? ` ${mainGrandServant.name_cn} ` : orderChangeLabel}
                和前排指令卡最多的从者交换。
              </Text>
              <button
                type="button"
                className="battle-option-btn"
                onClick={() => onChange({ ...scene, grandAutoOrderChange: false })}
              >
                改为配置指令卡
              </button>
            </div>
          ) : (
            <div className="advanced-manual-startup-condition">
              <div className="advanced-card-row">
                {commandConditions.map((card) => (
                  <AdvancedCommandCardButton
                    key={card.slot}
                    card={card}
                    partyMembers={partyMembers}
                    faces={faces}
                    onClick={() => setEditingCardSlot(card.slot)}
                  />
                ))}
              </div>
              {mainGrandSlot != null && grandAutoOrderChange === false && (
                <button
                  type="button"
                  className="battle-option-btn"
                  onClick={() => onChange({ ...scene, grandAutoOrderChange: true })}
                >
                  改为自动换位
                </button>
              )}
            </div>
          )}
        </div>
      </section>

      <section className="battle-phase advanced-strategy-section">
        <div className="battle-phase-label">控制栏</div>
        <div className="advanced-rule-section">
          {controlActions.map((action, index) => (
            <div className="battle-action-row committed advanced-action-row" key={action.id}>
              <button
                type="button"
                className="advanced-inline-delete"
                aria-label="删除控制行动"
                onClick={() => updateControlActions(controlActions.filter((_, i) => i !== index))}
              >
                <Cross2Icon width={13} height={13} />
              </button>
              <AdvancedPreparationActionSummary
                action={action}
                partyMembers={controlActionLineups[index] ?? partyMembers}
                faces={faces}
                skillIcons={skillIcons}
              />
            </div>
          ))}
          {!controlDraft ? (
            <AddRowTrigger
              leading={<span className="advanced-delete-spacer" aria-hidden />}
              onClick={() => setControlDraft({ step: "source" })}
            >
              添加控制行动
            </AddRowTrigger>
          ) : (
            <div className="battle-choice-row">
              <span className="advanced-delete-spacer" aria-hidden />
              {controlDraft.step === "source" ? (
                <>
                  {postControlMembers
                    .slice(0, 3)
                    .map((member, index) => {
                      const servant = member.servant;
                      return (
                        <FaceChip
                          key={index}
                          servant={servant}
                          index={index}
                          src={servant ? faces[servant.variantKey] : null}
                          isSupport={member.isSupport}
                          onClick={() =>
                            setControlDraft({
                              step: "option",
                              source: `servant_${index + 1}` as FrontServant,
                            })
                          }
                        />
                      );
                    })}
                  <button type="button" className="battle-option-btn" onClick={() => setControlDraft({ step: "option", source: "equipment" })}>
                    御主礼装
                  </button>
                  <button type="button" className="battle-option-btn" onClick={() => setControlDraft({ step: "option", source: "commandSpell" })}>
                    令咒
                  </button>
                  <button type="button" className="battle-option-btn" onClick={() => setControlDraft({ step: "target", source: "enemyTarget", option: "select" })}>
                    敌方目标
                  </button>
                </>
              ) : controlDraft.step === "option" ? (
                <>
                  {controlDraft.source === "commandSpell"
                    ? Object.entries(COMMAND_SPELL_LABELS).map(([value, label]) => (
                        <button
                          type="button"
                          className="battle-option-btn"
                          key={value}
                          onClick={() => setControlDraft({ step: "target", source: controlDraft.source, option: value })}
                        >
                          {label}
                        </button>
                      ))
                    : (
                      <SkillOptionButtons
                        servant={
                          controlDraft.source !== "equipment"
                            ? (postControlMembers[servantSlotIndex(controlDraft.source) ?? 0]?.servant ?? null)
                            : null
                        }
                        skillIcons={skillIcons}
                        onSelect={(skill) => selectControlSkill(controlDraft.source, skill)}
                      />
                    )}
                </>
              ) : controlDraft.step === "skillSelection" ? (
                <>
                  {controlDraft.options.map((option) => (
                    <button
                      type="button"
                      className="battle-option-btn"
                      key={option.index}
                      onClick={() => finishControlSkillSelection(controlDraft, option)}
                    >
                      {option.label}
                    </button>
                  ))}
                </>
              ) : controlDraft.step === "target" ? (
                <>
                  {controlDraft.source === "enemyTarget" ? (
                    <EnemyTargetButtons
                      ariaLabel="选择敌方目标"
                      onChange={(target) => { if (target) finishControlAction(controlDraft, target); }}
                    />
                  ) : <>
                  {controlDraft.allowNoTarget !== false && (
                    <button type="button" className="battle-option-btn" onClick={() => finishControlAction(controlDraft, null)}>
                      无目标
                    </button>
                  )}
                  {postControlMembers
                    .slice(0, 3)
                    .map((member, index) => {
                      const servant = member.servant;
                      return (
                        <FaceChip
                          key={index}
                          servant={servant}
                          index={index}
                          src={servant ? faces[servant.variantKey] : null}
                          isSupport={member.isSupport}
                          onClick={() => finishControlAction(controlDraft, `servant_${index + 1}`)}
                        />
                      );
                    })}
                  {controlDraft.source === "equipment" && (
                    <>
                      <span className="battle-choice-separator" aria-hidden />
                      <button
                        type="button"
                        className="battle-option-btn order-change"
                        aria-label="Order Change"
                        onClick={() =>
                          setControlDraft({
                            step: "orderChange",
                            source: "equipment",
                            option: controlDraft.option,
                            front: null,
                          })
                        }
                      >
                        <img src={orderChangeIcon} alt="" draggable={false} />
                      </button>
                    </>
                  )}
                  {controlDraft.allowNoTarget === true && (
                    <Text size="1" color="gray" className="battle-targeting-mode-hint">
                      该技能存在可选择目标与无需选择目标两种形态
                    </Text>
                  )}
                  </>}
                </>
              ) : (
                <>
                  {Array.from(
                    { length: 6 },
                    (_, index) => postControlMembers[index] ?? { servant: null, isSupport: false }
                  ).map((member, index) => {
                    const servant = member.servant;
                    const slot = `servant_${index + 1}` as PartySlot;
                    const needsFront = controlDraft.front == null;
                    const selectable = Boolean(servant) && (needsFront ? index < 3 : index >= 3);
                    return (
                      <FaceChip
                        key={index}
                        servant={servant}
                        index={index}
                        src={servant ? faces[servant.variantKey] : null}
                        active={selectable || controlDraft.front === slot}
                        isSupport={member.isSupport}
                        onClick={() => {
                          if (!selectable) return;
                          if (needsFront) {
                            setControlDraft({ ...controlDraft, front: slot });
                          } else {
                            finishControlOrderChangeAction(controlDraft, slot);
                          }
                        }}
                      />
                    );
                  })}
                  <Text size="2" weight="medium" className="battle-order-change-hint">
                    {controlDraft.front == null ? "选择前排" : "选择后排"}
                  </Text>
                </>
              )}
            </div>
          )}
        </div>
      </section>

      <section className="battle-phase advanced-strategy-section">
        <div className="battle-phase-label">使用技能</div>
        <div className="advanced-rule-section">
          <Flex align="center" gap="2" className="advanced-turn-controls">
            <Text size="2" weight="bold">Turn:</Text>
            {turns.map((turn, index) => (
              <button
                type="button"
                key={turn.id}
                className={`battle-turn-tab${activeTurn.id === turn.id ? " is-selected" : ""}`}
                aria-label={`Turn ${index + 1}`}
                onClick={() => {
                  onActiveTurnIndexChange(index);
                  setPrepDraft(null);
                }}
              >
                {index + 1}
              </button>
            ))}
            <IconButton
              type="button"
              variant="surface"
              color="gray"
              aria-label="添加 Turn"
              onClick={addTurn}
            >
              <PlusIcon width={16} height={16} />
            </IconButton>
            <IconButton
              type="button"
              variant="surface"
              color="red"
              aria-label="删除当前 Turn"
              disabled={activeTurnIndex === 0}
              onClick={deleteTurn}
            >
              <TrashIcon width={16} height={16} />
            </IconButton>
          </Flex>
          {startupActions.map((action, index) => (
            <div className="battle-action-row committed advanced-action-row" key={action.id}>
              <button
                type="button"
                className="advanced-inline-delete"
                aria-label="删除行动"
                onClick={() => updateStartupActions(startupActions.filter((_, i) => i !== index))}
              >
                <Cross2Icon width={13} height={13} />
              </button>
              <AdvancedPreparationActionSummary
                action={action}
                partyMembers={startupActionLineups[index] ?? partyMembers}
                faces={faces}
                skillIcons={skillIcons}
              />
            </div>
          ))}
          {!prepDraft ? (
            <AddRowTrigger
              leading={<span className="advanced-delete-spacer" aria-hidden />}
              onClick={() => setPrepDraft({ step: "source" })}
            >
              添加行动
            </AddRowTrigger>
          ) : (
            <div className="battle-choice-row">
              <span className="advanced-delete-spacer" aria-hidden />
              {prepDraft.step === "source" ? (
                <>
                  {startupSelectableSlots.map((index) => {
                    const member = currentPartyMembers[index] ?? { servant: null, isSupport: false };
                    const servant = member.servant;
                    return (
                      <FaceChip
                        key={index}
                        servant={servant}
                        index={index}
                        src={servant ? faces[servant.variantKey] : null}
                        isSupport={member.isSupport}
                        onClick={() =>
                          setPrepDraft({
                            step: "option",
                            source: `servant_${index + 1}` as PartySlot,
                          })
                        }
                      />
                    );
                  })}
                  <button type="button" className="battle-option-btn" onClick={() => setPrepDraft({ step: "option", source: "equipment" })}>
                    御主礼装
                  </button>
                  <button type="button" className="battle-option-btn" onClick={() => setPrepDraft({ step: "option", source: "commandSpell" })}>
                    令咒
                  </button>
                  <button type="button" className="battle-option-btn" onClick={() => setPrepDraft({ step: "target", source: "enemyTarget", option: "select" })}>
                    敌方目标
                  </button>
                </>
              ) : prepDraft.step === "option" ? (
                <>
                  {prepDraft.source === "commandSpell"
                    ? Object.entries(COMMAND_SPELL_LABELS).map(([value, label]) => (
                        <button
                          type="button"
                          className="battle-option-btn"
                          key={value}
                          onClick={() => setPrepDraft({ step: "target", source: prepDraft.source, option: value })}
                        >
                          {label}
                        </button>
                      ))
                    : (
                      <SkillOptionButtons
                        servant={
                          prepDraft.source !== "equipment"
                            ? (currentPartyMembers[servantSlotIndex(prepDraft.source) ?? 0]?.servant ?? null)
                            : null
                        }
                        skillIcons={skillIcons}
                        onSelect={(skill) => selectStartupSkill(prepDraft.source, skill)}
                      />
                    )}
                </>
              ) : prepDraft.step === "skillSelection" ? (
                <>
                  {prepDraft.options.map((option) => (
                    <button
                      type="button"
                      className="battle-option-btn"
                      key={option.index}
                      onClick={() => finishStartupSkillSelection(prepDraft, option)}
                    >
                      {option.label}
                    </button>
                  ))}
                </>
              ) : prepDraft.step === "target" ? (
                <>
                  {prepDraft.source === "enemyTarget" ? (
                    <EnemyTargetButtons
                      ariaLabel="选择敌方目标"
                      onChange={(target) => { if (target) finishPrepAction(prepDraft, target); }}
                    />
                  ) : <>
                  {prepDraft.allowNoTarget !== false && (
                    <button type="button" className="battle-option-btn" onClick={() => finishPrepAction(prepDraft, null)}>
                      无目标
                    </button>
                  )}
                  {startupSelectableSlots.map((index) => {
                    const member = currentPartyMembers[index] ?? { servant: null, isSupport: false };
                    const servant = member.servant;
                    return (
                      <FaceChip
                        key={index}
                        servant={servant}
                        index={index}
                        src={servant ? faces[servant.variantKey] : null}
                        isSupport={member.isSupport}
                        onClick={() => finishPrepAction(prepDraft, `servant_${index + 1}`)}
                      />
                    );
                  })}
                  {prepDraft.source === "equipment" && (
                    <>
                      <span className="battle-choice-separator" aria-hidden />
                      <button
                        type="button"
                        className="battle-option-btn order-change"
                        aria-label="Order Change"
                        onClick={() =>
                          setPrepDraft({
                            step: "orderChange",
                            source: "equipment",
                            option: prepDraft.option,
                            front: null,
                          })
                        }
                      >
                        <img src={orderChangeIcon} alt="" draggable={false} />
                      </button>
                    </>
                  )}
                  {prepDraft.allowNoTarget === true && (
                    <Text size="1" color="gray" className="battle-targeting-mode-hint">
                      该技能存在可选择目标与无需选择目标两种形态
                    </Text>
                  )}
                  </>}
                </>
              ) : (
                <>
                  {Array.from(
                    { length: 6 },
                    (_, index) => currentPartyMembers[index] ?? { servant: null, isSupport: false }
                  ).map((member, index) => {
                    const servant = member.servant;
                    const slot = `servant_${index + 1}` as PartySlot;
                    const needsFront = prepDraft.front == null;
                    const selectable =
                      Boolean(servant) &&
                      (needsFront
                        ? index < 3 || (grandAutoOrderChange === true && index === mainGrandSlot)
                        : index >= 3);
                    return (
                      <FaceChip
                        key={index}
                        servant={servant}
                        index={index}
                        src={servant ? faces[servant.variantKey] : null}
                        active={selectable || prepDraft.front === slot}
                        isSupport={member.isSupport}
                        onClick={() => {
                          if (!selectable) return;
                          if (needsFront) {
                            setPrepDraft({ ...prepDraft, front: slot });
                          } else {
                            finishOrderChangeAction(prepDraft, slot);
                          }
                        }}
                      />
                    );
                  })}
                  <Text size="2" weight="medium" className="battle-order-change-hint">
                    {prepDraft.front == null ? "选择前排" : "选择后排"}
                  </Text>
                </>
              )}
            </div>
          )}
        </div>
      </section>

      <EnemyTargetSelector
        className="advanced-strategy-section"
        value={scene.enemyTarget}
        onChange={(enemyTarget) => onChange({ ...scene, enemyTarget })}
      />

      {grandCardPriorityEnabled && (
        <GrandCardStrategyPanel
          strategy={grandCardStrategy}
          partyMembers={partyMembers}
          faces={faces}
          onChange={onGrandCardStrategyChange}
        />
      )}

      <Dialog.Root
        open={editingCardSlot != null && editingCard != null}
        onOpenChange={(open) => {
          if (!open) setEditingCardSlot(null);
        }}
      >
        <Dialog.Content maxWidth="360px">
          <Dialog.Title>设置指令卡</Dialog.Title>
          <Dialog.Description className="sr-only">
            设置指令卡匹配的从者和色卡条件。
          </Dialog.Description>
          {editingCard && (
            <div className="advanced-card-editor">
              <label>
                从者
                <Select.Root
                  value={editingCard.servant}
                  onValueChange={(value) => {
                    const ref =
                      value === "any"
                        ? { memberId: null, servantId: null, isSupport: false }
                        : memberRefAt(partyMembers, value);
                    updateCommandCard(editingCard.slot, (prev) => ({
                      ...prev,
                      servant: value as AdvancedCommandCardCondition["servant"],
                      memberId: ref.memberId,
                      servantId: ref.servantId,
                      isSupport: ref.isSupport,
                      minCritChance: null,
                    }));
                  }}
                >
                  <Select.Trigger aria-label="从者" />
                  <Select.Content>
                    <Select.Item value="any">任意</Select.Item>
                    <Select.Item value="servant_1">{servantLabel(0, partyLineup[0] ?? null)}</Select.Item>
                    <Select.Item value="servant_2">{servantLabel(1, partyLineup[1] ?? null)}</Select.Item>
                    <Select.Item value="servant_3">{servantLabel(2, partyLineup[2] ?? null)}</Select.Item>
                  </Select.Content>
                </Select.Root>
              </label>
              <label>
                色卡
                <Select.Root
                  value={editingCard.suit}
                  onValueChange={(value) =>
                    updateCommandCard(editingCard.slot, (prev) => ({
                      ...prev,
                      suit: value as AdvancedCommandCardCondition["suit"],
                      minCritChance: null,
                    }))
                  }
                >
                  <Select.Trigger aria-label="色卡" />
                  <Select.Content>
                    <Select.Item value="any">任意</Select.Item>
                    <Select.Item value="buster">红卡</Select.Item>
                    <Select.Item value="arts">蓝卡</Select.Item>
                    <Select.Item value="quick">绿卡</Select.Item>
                  </Select.Content>
                </Select.Root>
              </label>
            </div>
          )}
          <Flex justify="end" mt="4">
            <Dialog.Close>
              <Button type="button">完成</Button>
            </Dialog.Close>
          </Flex>
        </Dialog.Content>
      </Dialog.Root>
    </div>
  );
}

export function AdvancedCommandEditor({
  projectId,
  partyLineup,
  partyMembers,
  disableAutoSkillTargetRecognition = false,
  grandServants = [],
  grandClassDefinition,
  grandCardStrategy,
  grandCardPriorityEnabled = false,
  onGrandServantsChange,
  onGrandCardStrategyChange,
}: AdvancedCommandEditorProps) {
  // Coronation mode is single-scene by design — the runner only ever
  // executes one battle. We still persist as an array on disk so the
  // backend schema (`save_advanced_battle_scenes`) stays compatible
  // with existing project files; older multi-scene drafts collapse to
  // the first scene.
  const [scene, setScene] = useState<AdvancedBattleScene>(() => createDefaultScene());
  const [activeTurnIndex, setActiveTurnIndex] = useState(0);
  const [loaded, setLoaded] = useState(() => !projectId);
  const initialPartyMembers = useMemo(
    () => partyMembers ?? toPartyMembers(partyLineup),
    [partyMembers, partyLineup]
  );
  const initialPartyLineup = useMemo(
    () => partyMembersToServants(initialPartyMembers),
    [initialPartyMembers]
  );
  const faces = useServantFaceImages(initialPartyLineup);
  const skillIcons = useServantSkillIcons(initialPartyLineup);
  const skillTargetStatus = useServantSkillTargeting(initialPartyLineup);
  const skillSelection = useServantSkillSelections(initialPartyLineup);

  useEffect(() => {
    if (!projectId) return;
    let cancelled = false;
    invoke<AdvancedBattleScene[]>("load_advanced_battle_scenes", { projectId })
      .then((saved) => {
        if (cancelled) return;
        const first = saved.length > 0 ? normalizeScene(saved[0]) : createDefaultScene();
        setScene(first);
        setActiveTurnIndex(0);
      })
      .catch(() => {
        if (cancelled) return;
        setScene(createDefaultScene());
        setActiveTurnIndex(0);
      })
      .finally(() => {
        if (!cancelled) setLoaded(true);
      });
    return () => {
      cancelled = true;
    };
  }, [projectId]);

  const handleSceneChange = useCallback(
    (updatedScene: AdvancedBattleScene) => {
      setScene(updatedScene);
      if (!projectId) return;
      invoke("save_advanced_battle_scenes", {
        projectId,
        scenes: [updatedScene],
      }).catch(console.error);
    },
    [projectId]
  );

  if (!loaded) return null;

  return (
    <Flex direction="column" className="command-editor advanced-command-editor">
      <div className="command-scroll-region">
      <AdvancedStrategyEditor
          scene={scene}
          partyMembers={initialPartyMembers}
          faces={faces}
          skillIcons={skillIcons}
          skillTargetStatus={skillTargetStatus}
          skillSelection={skillSelection}
          disableAutoSkillTargetRecognition={disableAutoSkillTargetRecognition}
        grandServants={grandServants}
        grandClassDefinition={grandClassDefinition}
          grandCardStrategy={grandCardStrategy}
          grandCardPriorityEnabled={grandCardPriorityEnabled}
          onGrandServantsChange={onGrandServantsChange}
          onGrandCardStrategyChange={onGrandCardStrategyChange}
          activeTurnIndex={activeTurnIndex}
          onActiveTurnIndexChange={setActiveTurnIndex}
          onChange={handleSceneChange}
        />
      </div>
    </Flex>
  );
}
