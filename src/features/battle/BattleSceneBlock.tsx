import { useEffect, useMemo, useState } from "react";
import type React from "react";
import { Avatar, Text } from "@radix-ui/themes";
import { Cross2Icon } from "@radix-ui/react-icons";
import orderChangeIcon from "../../../src-tauri/resources/images/icon_order_change.png";
import { convertFileSrc } from "../../tauri";
import { BattleActorIcon } from "../../components/common/BattleActorIcon";
import { battleActorLabel, servantLabel } from "../../components/common/battleActorLabels";
import { useServantFaceImages } from "../team/useServantFaceImages";
import { useServantSkillIcons, type SkillIcons } from "../team/useServantSkillIcons";
import { SkillOptionButtons } from "../../components/common/SkillOptionButtons";
import { AddRowTrigger } from "../../components/common/AddRowTrigger";
import { EnemyTargetButtons, EnemyTargetSelector } from "./EnemyTargetSelector";
import { useServantSkillTargeting } from "./useServantSkillTargeting";
import { useServantSkillSelections } from "./useServantSkillSelections";
import type { CommandStep } from "./CommandWorkspace";
import { ServantChoice } from "../../components/common/ServantChoice";
import { MysticCodeChoice } from "../../components/common/MysticCodeChoice";
import { CommandDraftHeading } from "../../components/common/CommandDraftHeading";
import { ActionRowControls } from "../../components/common/ActionRowControls";
import { CriticalStrategyEditor } from "./CriticalStrategyEditor";
import { GrandCardStrategyPanel } from "../advanced/AdvancedGrandStrategyPanel";
import {
  deriveMembersAfterAttackCards,
  deriveMembersAfterPreparationActions,
  memberRefAt,
  partyMembersToServants,
  resolveMemberRefIndex,
  toPartyMembers,
  type PartyMember,
} from "../team/partyServants";
import {
  ATTACK_OPTIONS,
  CARD_LABELS,
  COMMAND_SPELL_LABELS,
  FIXED_ATTACK_CARD_COUNT,
  SKILL_LABELS,
  skillSlotIndex,
  createId,
  emptyLegacyFields,
  normalizeAttackPriority,
  sourceIndex,
  type AttackDraft,
  type AttackSource,
  type EnemyTarget,
  type PartySlot,
  type PrepDraft,
  type PrepSource,
} from "./battleSceneModel";
import type {
  AttackCard,
  BattleTurn,
  CommandSpellAction,
  EquipmentAction,
  EnemyTargetAction,
  PreparationAction,
  ServantAction,
} from "../../types/command";
import type { Servant } from "../../types/servant";
import { mysticCodeSkill, type MysticCode } from "../../types/mysticCode";

interface BattleSceneBlockProps {
  scene: BattleTurn;
  partyServants: (Servant | null)[];
  partyMembers?: PartyMember[];
  disableAutoSkillTargetRecognition?: boolean;
  mysticCode?: MysticCode | null;
  turnAttackModesEnabled?: boolean;
  step?: CommandStep;
  onSelectEnemy?: () => void;
  onChange: (updated: BattleTurn) => void;
}

function ServantFaceButton({
  servant,
  index,
  faceSrc,
  onClick,
  disabled = false,
  selected = false,
  isSupport = false,
}: {
  servant: Servant | null;
  index: number;
  faceSrc: string | null | undefined;
  onClick: () => void;
  disabled?: boolean;
  selected?: boolean;
  isSupport?: boolean;
}) {
  return <ServantChoice servant={servant} index={index} src={faceSrc} isSupport={isSupport} disabled={disabled} selected={selected} onClick={onClick} className="battle-face-btn" />;
}

function ServantInlineFace({
  servant,
  index,
  faceSrc,
  isSupport = false,
}: {
  servant: Servant | null;
  index: number;
  faceSrc: string | null | undefined;
  isSupport?: boolean;
}) {
  return (
    <BattleActorIcon
      kind="servant"
      src={faceSrc}
      label={servantLabel(index, servant)}
      isSupport={isSupport}
      size="inline"
    />
  );
}

function PreparationActionSummary({
  action,
  partyMembers,
  faces,
  skillIcons,
  mysticCode,
}: {
  action: PreparationAction;
  partyMembers: PartyMember[];
  faces: Record<string, string | null>;
  skillIcons: Record<string, SkillIcons>;
  mysticCode?: MysticCode | null;
}) {
  if (action.type === "enemyTarget") {
    return (
      <span className="battle-action-summary" aria-label={actionSummary(action, partyMembers)}>
        <Text size="2" weight="medium" className="battle-action-name">
          选择敌方目标 {action.target?.replace("enemy_", "") ?? "?"}
        </Text>
      </span>
    );
  }
  const partyServants = partyMembersToServants(partyMembers);
  const targetIndex = frontMemberIndex(
    partyMembers,
    action.target,
    action.type === "servant" ||
      action.type === "equipment" ||
      action.type === "commandSpell"
      ? action.targetMemberId
      : null,
    action.type === "servant" ||
      action.type === "equipment" ||
      action.type === "commandSpell"
      ? action.targetServantId
      : null,
    action.type === "servant" ||
      action.type === "equipment" ||
      action.type === "commandSpell"
      ? action.targetIsSupport
      : false
  );
  const orderChangeSlots =
    action.type === "equipment" && action.orderChange
      ? {
        front: memberIndex(
          partyMembers,
          action.orderChange.front,
          action.orderChange.frontMemberId,
          action.orderChange.frontServantId,
          action.orderChange.frontIsSupport
        ),
        back: memberIndex(
          partyMembers,
          action.orderChange.back,
          action.orderChange.backMemberId,
          action.orderChange.backServantId,
          action.orderChange.backIsSupport
        ),
      }
      : null;
  let sourceFace: React.ReactNode;
  let sourceText: string;
  let actionText: string;
  let skillIconSrc: string | null = null;
  let skillLabel = "技能";
  let skillSlot = 0;

  if (action.type === "servant") {
    const src = frontMemberIndex(
      partyMembers,
      action.servant,
      action.servantMemberId,
      action.servantId,
      action.servantIsSupport
    );
    const member =
      src == null
        ? { servant: null, isSupport: false }
        : partyMembers[src] ?? { servant: null, isSupport: false };
    const servant = member.servant;
    sourceFace = (
      <ServantInlineFace
        servant={servant}
        index={src ?? 0}
        faceSrc={servant ? faces[servant.variantKey] : null}
        isSupport={member.isSupport}
      />
    );
    sourceText = src == null ? "从者" : servantLabel(src, servant);
    const idx = skillSlotIndex(action.skill);
    const skillEntry = servant && idx >= 0 ? (skillIcons[servant.variantKey]?.[idx] ?? null) : null;
    skillIconSrc = skillEntry?.src ?? null;
    skillLabel = skillEntry?.name || (SKILL_LABELS[action.skill ?? ""] ?? "技能");
    skillSlot = idx >= 0 ? idx : 0;
    actionText = `释放 ${skillLabel}`;
    if (action.skillSelection) {
      actionText += `并选择 ${action.skillSelection.label ?? `选项 ${action.skillSelection.index + 1}`}`;
    }
  } else {
    const kind = action.type === "equipment" ? "equipment" : "commandSpell";
    sourceFace = (
      <BattleActorIcon kind={kind} label={battleActorLabel({ kind })} size="inline" />
    );
    sourceText = action.type === "equipment" ? (mysticCode?.name ?? "御主礼装") : "令咒";
    actionText =
      action.type === "equipment"
        ? `释放 ${mysticCodeSkill(mysticCode, action.skill ?? "")?.name ?? SKILL_LABELS[action.skill ?? ""] ?? "技能"}`
        : COMMAND_SPELL_LABELS[action.spell ?? ""] ?? "行动";
    if (action.type === "equipment") {
      const entry = mysticCodeSkill(mysticCode, action.skill ?? "");
      skillIconSrc = entry?.iconPath ? convertFileSrc(entry.iconPath) : null;
      skillLabel = entry?.name ?? SKILL_LABELS[action.skill ?? ""] ?? "技能";
      skillSlot = Math.max(0, skillSlotIndex(action.skill));
    }
  }

  return (
    <span
      className="battle-action-summary"
      aria-label={actionSummary(action, partyMembers)}
    >
      <span className="command-row-source">{sourceFace}<Text size="2" weight="medium" className="battle-action-name">
        {sourceText}{" "}{action.type === "commandSpell" ? "" : action.type === "equipment" ? actionText : "释放"}
      </Text></span>
      <span className="command-row-skill">
        {action.type === "commandSpell" ? <Text size="2">{actionText}</Text> : <span className="battle-inline-skill-icon" title={skillLabel}><Avatar src={skillIconSrc ?? undefined} fallback={String(skillSlot + 1)} size="1" radius="small" /></span>}
        {action.type === "servant" && action.skillSelection ? <small>并选择 {action.skillSelection.label ?? `选项 ${action.skillSelection.index + 1}`}</small> : null}
      </span>
      <span className="command-row-outcome">
      {orderChangeSlots?.front != null && orderChangeSlots.back != null ? (
        <>

          <ServantInlineFace
            servant={partyServants[orderChangeSlots.front] ?? null}
            index={orderChangeSlots.front}
            faceSrc={
              partyServants[orderChangeSlots.front]
                ? faces[partyServants[orderChangeSlots.front]!.variantKey]
                : null
            }
            isSupport={partyMembers[orderChangeSlots.front]?.isSupport ?? false}
          />
          <Text size="2" weight="medium" className="battle-action-name">
            {servantLabel(orderChangeSlots.front, partyServants[orderChangeSlots.front] ?? null)}
          </Text>
          <span className="battle-action-to">↔</span>
          <ServantInlineFace
            servant={partyServants[orderChangeSlots.back] ?? null}
            index={orderChangeSlots.back}
            faceSrc={
              partyServants[orderChangeSlots.back]
                ? faces[partyServants[orderChangeSlots.back]!.variantKey]
                : null
            }
            isSupport={partyMembers[orderChangeSlots.back]?.isSupport ?? false}
          />
          <Text size="2" weight="medium" className="battle-action-name">
            {servantLabel(orderChangeSlots.back, partyServants[orderChangeSlots.back] ?? null)}
          </Text>
        </>
      ) : (
        targetIndex != null && (
          <>
            <span className="battle-action-to">to</span>
            <ServantInlineFace
              servant={partyServants[targetIndex] ?? null}
              index={targetIndex}
              faceSrc={
                partyServants[targetIndex]
                  ? faces[partyServants[targetIndex]!.variantKey]
                  : null
              }
              isSupport={partyMembers[targetIndex]?.isSupport ?? false}
            />
            <Text size="2" weight="medium" className="battle-action-name">
              {servantLabel(targetIndex, partyServants[targetIndex] ?? null)}
            </Text>
          </>
        )
      )}
      </span>
    </span>
  );
}

function memberIndex(
  members: PartyMember[],
  fallbackValue: string | null | undefined,
  memberId: string | null | undefined,
  servantId: number | null | undefined,
  isSupport: boolean | null | undefined
): number | null {
  return resolveMemberRefIndex(members, fallbackValue, memberId, servantId, isSupport);
}

function frontMemberIndex(
  members: PartyMember[],
  fallbackValue: string | null | undefined,
  memberId: string | null | undefined,
  servantId: number | null | undefined,
  isSupport: boolean | null | undefined
): number | null {
  const index = memberIndex(members, fallbackValue, memberId, servantId, isSupport);
  return index != null && index >= 0 && index < 3 ? index : null;
}

function AttackActionFace({
  card,
  partyMembers,
  faces,
}: {
  card: AttackCard;
  partyMembers: PartyMember[];
  faces: Record<string, string | null>;
}) {
  const match = card.card?.match(/^servant_([1-3])_/);
  if (!match) return null;
  const index = Number(match[1]) - 1;
  const member = partyMembers[index] ?? { servant: null, isSupport: false };
  const servant = member.servant;
  return (
    <ServantInlineFace
      servant={servant}
      index={index}
      faceSrc={servant ? faces[servant.variantKey] : null}
      isSupport={member.isSupport}
    />
  );
}

function ActionDeleteButton({ onClick, disabled = false }: { onClick: () => void; disabled?: boolean }) {
  return (
    <button
      type="button"
      className="battle-action-delete"
      aria-label="删除行动"
      disabled={disabled}
      onClick={onClick}
    >
      <Cross2Icon width={13} height={13} />
    </button>
  );
}

function ActionClearButton({ onClick }: { onClick: () => void }) {
  return (
    <button
      type="button"
      className="battle-action-clear"
      aria-label="清除指令卡"
      onClick={onClick}
    >
      <Cross2Icon width={13} height={13} />
    </button>
  );
}

function DraftCancelButton({
  visible,
  onClick,
}: {
  visible: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className={`battle-draft-cancel${visible ? "" : " placeholder"}`}
      aria-label={visible ? "撤销添加行动" : undefined}
      aria-hidden={visible ? undefined : true}
      tabIndex={visible ? 0 : -1}
      onClick={visible ? onClick : undefined}
    >
      <Cross2Icon width={13} height={13} />
    </button>
  );
}

function actionSummary(
  action: PreparationAction,
  partyMembers: PartyMember[]
): string {
  if (action.type === "enemyTarget") {
    return `选择敌方目标 ${action.target?.replace("enemy_", "") ?? "?"}`;
  }
  const partyServants = partyMembersToServants(partyMembers);
  if (action.type === "servant") {
    const src = frontMemberIndex(
      partyMembers,
      action.servant,
      action.servantMemberId,
      action.servantId,
      action.servantIsSupport
    );
    const target = frontMemberIndex(
      partyMembers,
      action.target,
      action.targetMemberId,
      action.targetServantId,
      action.targetIsSupport
    );
    const sourceLabel = src == null ? "从者" : servantLabel(src, partyServants[src] ?? null);
    const selection = action.skillSelection
      ? `并选择 ${action.skillSelection.label ?? `选项 ${action.skillSelection.index + 1}`}`
      : "";
    return target == null
      ? `${sourceLabel} 释放 ${SKILL_LABELS[action.skill ?? ""] ?? "技能"}${selection}`
      : `${sourceLabel} 释放 ${SKILL_LABELS[action.skill ?? ""] ?? "技能"}${selection} to ${servantLabel(target, partyServants[target] ?? null)}`;
  }

  if (action.type === "equipment") {
    if (action.orderChange) {
      const front = memberIndex(
        partyMembers,
        action.orderChange.front,
        action.orderChange.frontMemberId,
        action.orderChange.frontServantId,
        action.orderChange.frontIsSupport
      );
      const back = memberIndex(
        partyMembers,
        action.orderChange.back,
        action.orderChange.backMemberId,
        action.orderChange.backServantId,
        action.orderChange.backIsSupport
      );
      const summary =
        front == null || back == null
          ? null
          : `${servantLabel(front, partyServants[front] ?? null)} ↔ ${servantLabel(back, partyServants[back] ?? null)}`;
      return summary
        ? `御主礼装 释放 ${SKILL_LABELS[action.skill ?? ""] ?? "技能"} Order Change ${summary}`
        : `御主礼装 释放 ${SKILL_LABELS[action.skill ?? ""] ?? "技能"} Order Change`;
    }
    const target = frontMemberIndex(
      partyMembers,
      action.target,
      action.targetMemberId,
      action.targetServantId,
      action.targetIsSupport
    );
    return target == null
      ? `御主礼装 释放 ${SKILL_LABELS[action.skill ?? ""] ?? "技能"}`
      : `御主礼装 释放 ${SKILL_LABELS[action.skill ?? ""] ?? "技能"} to ${servantLabel(target, partyServants[target] ?? null)}`;
  }

  const target = frontMemberIndex(
    partyMembers,
    action.target,
    action.targetMemberId,
    action.targetServantId,
    action.targetIsSupport
  );
  return target == null
    ? `令咒 ${COMMAND_SPELL_LABELS[action.spell ?? ""] ?? "行动"}`
    : `令咒 ${COMMAND_SPELL_LABELS[action.spell ?? ""] ?? "行动"} to ${servantLabel(target, partyServants[target] ?? null)}`;
}

function attackSummary(card: AttackCard, partyServants: (Servant | null)[]): string {
  const match = card.card?.match(/^servant_([1-3])_(np|buster|arts|quick|all)$/);
  if (!match) return "未设置攻击";
  const index = Number(match[1]) - 1;
  const kind = match[2];
  return `${servantLabel(index, partyServants[index] ?? null)} ${CARD_LABELS[kind]}`;
}

function attackSlotLabel(index: number): string {
  return index < FIXED_ATTACK_CARD_COUNT ? `指令卡${["一", "二", "三"][index]}` : "";
}

export function BattleSceneBlock({
  scene,
  partyServants,
  partyMembers,
  disableAutoSkillTargetRecognition = false,
  mysticCode = null,
  turnAttackModesEnabled = false,
  step,
  onSelectEnemy,
  onChange,
}: BattleSceneBlockProps) {
  const [editingPrepIndex, setEditingPrepIndex] = useState<number | null>(null);
  const [prepDraft, setPrepDraft] = useState<PrepDraft | null>(null);
  const [attackDraft, setAttackDraft] = useState<AttackDraft | null>(null);
  const hasDraft = Boolean(prepDraft || attackDraft);
  useEffect(() => {
    if (!hasDraft) return;
    const cancel = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      setPrepDraft(null);
      setEditingPrepIndex(null);
      setAttackDraft(null);
    };
    document.addEventListener("keydown", cancel);
    return () => document.removeEventListener("keydown", cancel);
  }, [hasDraft]);
  useEffect(() => { if (hasDraft) document.querySelector(".command-content .battle-add-row, .command-content .battle-choice-row")?.scrollIntoView({ block: "nearest" }); }, [hasDraft, prepDraft?.step, attackDraft?.step]);
  const initialPartyMembers = useMemo(
    () => partyMembers ?? toPartyMembers(partyServants),
    [partyMembers, partyServants]
  );
  const initialPartyServants = partyMembersToServants(initialPartyMembers);
  const faces = useServantFaceImages(initialPartyServants);
  const skillIcons = useServantSkillIcons(initialPartyServants);
  const skillTargetStatus = useServantSkillTargeting(initialPartyServants);
  const skillSelection = useServantSkillSelections(initialPartyServants);
  const mysticSkillEntries: SkillIcons = [1, 2, 3].map((slot) => {
    const skill = mysticCode?.skills.find((entry) => entry.slot === slot);
    return {
      src: skill?.iconPath ? convertFileSrc(skill.iconPath) : null,
      name: skill?.name ?? "",
    };
  }) as SkillIcons;
  const attackMode = turnAttackModesEnabled ? (scene.attackMode ?? "normal") : "normal";
  const preparationActions = useMemo(
    () =>
      scene.preparationActions ??
      [
        ...(scene.servantActions ?? []),
        ...(scene.equipmentActions ?? []),
        ...(scene.commandSpellActions ?? []),
      ],
    [
      scene.commandSpellActions,
      scene.equipmentActions,
      scene.preparationActions,
      scene.servantActions,
    ]
  );
  const preparationActionLineups = useMemo(() => {
    const lineups: PartyMember[][] = [];
    let lineup = initialPartyMembers;
    for (const action of preparationActions) {
      lineups.push(lineup);
      lineup = deriveMembersAfterPreparationActions(lineup, [action]);
    }
    return lineups;
  }, [initialPartyMembers, preparationActions]);
  const currentPartyMembers = useMemo(
    () => deriveMembersAfterPreparationActions(initialPartyMembers, preparationActions),
    [initialPartyMembers, preparationActions]
  );
  const draftPartyMembers = editingPrepIndex == null ? currentPartyMembers : preparationActionLineups[editingPrepIndex] ?? initialPartyMembers;
  const commitPrep = (action: PreparationAction) => {
    updatePreparationActions(editingPrepIndex == null ? [...preparationActions, action] : preparationActions.map((existing,index) => index === editingPrepIndex ? {...action,id:existing.id} : existing));
    setPrepDraft(null); setEditingPrepIndex(null);
  };
  const attackPriority = useMemo(
    () => normalizeAttackPriority(scene.attackPriority ?? []),
    [scene.attackPriority]
  );
  const attackActionLineups = useMemo(
    () =>
      attackPriority.map((_, index) =>
        deriveMembersAfterAttackCards(
          currentPartyMembers,
          attackPriority.slice(0, index)
        )
      ),
    [attackPriority, currentPartyMembers]
  );
  const currentAttackPartyMembers = useMemo(
    () => deriveMembersAfterAttackCards(currentPartyMembers, attackPriority),
    [attackPriority, currentPartyMembers]
  );
  const updatePreparationActions = (next: PreparationAction[]) => {
    onChange(emptyLegacyFields({ ...scene, preparationActions: next }));
  };

  const updateAttackPriority = (next: AttackCard[]) => {
    onChange(emptyLegacyFields({ ...scene, attackPriority: next }));
  };

  const updateAttackMode = (attackMode: "normal" | "critical" | "advanced") => {
    setAttackDraft(null);
    onChange(emptyLegacyFields({ ...scene, attackMode }));
  };

  const updateEnemyTarget = (enemyTarget: EnemyTarget | null) => {
    onChange(emptyLegacyFields({ ...scene, enemyTarget }));
  };

  const targetRef = (target: string | null) => {
    const ref = memberRefAt(draftPartyMembers, target);
    return {
      targetMemberId: ref.memberId,
      targetServantId: ref.servantId,
      targetIsSupport: ref.isSupport,
    };
  };

  const servantRef = (servant: string | null) => {
    const ref = memberRefAt(draftPartyMembers, servant);
    return {
      servantMemberId: ref.memberId,
      servantId: ref.servantId,
      servantIsSupport: ref.isSupport,
    };
  };

  const orderChangeRef = (key: "front" | "back", slot: PartySlot) => {
    const ref = memberRefAt(draftPartyMembers, slot);
    return {
      [`${key}MemberId`]: ref.memberId,
      [`${key}ServantId`]: ref.servantId,
      [`${key}IsSupport`]: ref.isSupport,
    };
  };

  const finishPrepAction = (draft: Extract<PrepDraft, { step: "target" }>, target: string | null) => {
    let action: PreparationAction;
    if (draft.source === "enemyTarget") {
      action = {
        type: "enemyTarget",
        id: createId("enemy_target"),
        target,
      } satisfies EnemyTargetAction;
    } else if (draft.source === "equipment") {
      action = {
        type: "equipment",
        id: createId("eq"),
        skill: draft.option,
        target,
        ...targetRef(target),
        orderChange: null,
      } satisfies EquipmentAction;
    } else if (draft.source === "commandSpell") {
      action = {
        type: "commandSpell",
        id: createId("cs"),
        spell: draft.option as CommandSpellAction["spell"],
        target,
        ...targetRef(target),
      } satisfies CommandSpellAction;
    } else {
      action = {
        type: "servant",
        id: createId("sa"),
        servant: draft.source,
        ...servantRef(draft.source),
        skill: draft.option,
        skillSelection: draft.skillSelection ?? null,
        target,
        ...targetRef(target),
      } satisfies ServantAction;
    }
    commitPrep(action);
  };

  const selectPrepSkill = (source: PrepSource, skill: string) => {
    if (source === "equipment") {
      const meta = mysticCodeSkill(mysticCode, skill);
      const draft = { step: "target", source, option: skill } satisfies Extract<PrepDraft, { step: "target" }>;
      const status = disableAutoSkillTargetRecognition ? "unknown" : meta?.targetingMode ?? "unknown";
      if (status === "noTarget") {
        finishPrepAction(draft, null);
      } else if (status === "orderChange") {
        setPrepDraft({ step: "orderChange", source, option: skill, front: null });
      } else {
        setPrepDraft(status === "needsTarget" ? { ...draft, allowNoTarget: false } : draft);
      }
      return;
    }
    const servant = draftPartyMembers[sourceIndex(source) ?? 0]?.servant ?? null;
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

  const finishSkillSelection = (
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
    const servant = draftPartyMembers[sourceIndex(draft.source) ?? 0]?.servant ?? null;
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

  const finishOrderChangeAction = (
    draft: Extract<PrepDraft, { step: "orderChange" }>,
    back: PartySlot
  ) => {
    if (draft.front == null) return;
    const action = {
      type: "equipment",
      id: createId("eq"),
      skill: draft.option,
      target: null,
      orderChange: {
        front: draft.front,
        ...orderChangeRef("front", draft.front),
        back,
        ...orderChangeRef("back", back),
      },
    } satisfies EquipmentAction;
    commitPrep(action);
  };

  const finishAttackAction = (source: AttackSource, option: string) => {
    const next = [...attackPriority];
    const card = `${source}_${option}`;
    const ref = memberRefAt(currentPartyMembers, source);
    const patch = {
      card,
      memberId: ref.memberId,
      servantId: ref.servantId,
      isSupport: ref.isSupport,
    };
    if (attackDraft?.targetIndex != null) {
      next[attackDraft.targetIndex] = {
        ...(next[attackDraft.targetIndex] ?? { id: createId("atk") }),
        ...patch,
      };
    } else {
      next.push({ id: createId("atk"), ...patch });
    }
    updateAttackPriority(next);
    setAttackDraft(null);
  };

  const clearAttackAction = (index: number) => {
    const next = [...attackPriority];
    if (!next[index]) return;
    next[index] = { ...next[index], card: null };
    updateAttackPriority(next);
    if (attackDraft?.targetIndex === index) {
      setAttackDraft(null);
    }
  };

  const renderAttackDraft = (
    draft: AttackDraft,
    draftPartyMembers: PartyMember[]
  ) =>
    draft.step === "source" ? (
      <div className="battle-choice-row inline">
        {draftPartyMembers.slice(0, 3).map((member, index) => {
          const servant = member.servant;
          return (
            <ServantFaceButton
              key={index}
              servant={servant}
              index={index}
              faceSrc={servant ? faces[servant.variantKey] : null}
              isSupport={member.isSupport}
              onClick={() =>
                setAttackDraft({
                  step: "option",
                  source: `servant_${index + 1}` as AttackSource,
                  targetIndex: draft.targetIndex,
                })
              }
            />
          );
        })}
      </div>
    ) : (
      <div className="battle-choice-row inline">
        {(() => {
          const index = sourceIndex(draft.source) ?? 0;
          const member = draftPartyMembers[index] ?? { servant: null, isSupport: false };
          const servant = member.servant;
          return (
            <ServantFaceButton
              servant={servant}
              index={index}
              faceSrc={servant ? faces[servant.variantKey] : null}
              isSupport={member.isSupport}
              onClick={() =>
                setAttackDraft({
                  step: "source",
                  targetIndex: draft.targetIndex,
                })
              }
            />
          );
        })()}
        <div className="battle-option-group">
          {ATTACK_OPTIONS.map((option) => (
            <button
              type="button"
              key={option.value}
              className="battle-option-btn"
              onClick={() => finishAttackAction(draft.source, option.value)}
            >
              {option.label}
            </button>
          ))}
        </div>
      </div>
    );

  return (
    <div className="battle-scene-editor">
      {(step == null || step === "prep") && <section className="battle-phase">
        <div className="battle-action-list">
          {preparationActions.map((action, index) => (
            <div className="battle-action-row committed" key={action.id}>
              <ActionDeleteButton
                disabled={prepDraft != null}
                onClick={() =>
                  updatePreparationActions(
                    preparationActions.filter((_, i) => i !== index)
                  )
                }
              />
              <ActionRowControls index={index} count={preparationActions.length} disabled={prepDraft != null}
                onEdit={() => {setEditingPrepIndex(index);setPrepDraft({step:"source"});}}
                onMove={direction => {const next=[...preparationActions];[next[index],next[index+direction]]=[next[index+direction],next[index]];updatePreparationActions(next);}} />
              <PreparationActionSummary
                action={action}
                partyMembers={preparationActionLineups[index] ?? initialPartyMembers}
                faces={faces}
                skillIcons={skillIcons}
                mysticCode={mysticCode}
              />
            </div>
          ))}
          <div className={`battle-add-row${prepDraft ? " command-inline-draft" : ""}`}>
            {prepDraft && <CommandDraftHeading title={editingPrepIndex != null ? "编辑技能指令" : prepDraft.step !== "source" && prepDraft.source === "commandSpell" ? "使用令咒" : "添加技能指令"} hint={prepDraft.step === "source" ? "选择前排从者或御主礼装" : prepDraft.step === "option" ? "选择技能" : prepDraft.step === "target" ? "选择目标" : prepDraft.step === "orderChange" ? "从前排和后排各选择一名从者" : "选择技能选项"} onCancel={() => {setPrepDraft(null);setEditingPrepIndex(null);}} />}
            {!prepDraft ? (
              <div className="command-entry-actions">
                <AddRowTrigger iconSize={16} transparentIconBackground onClick={() => setPrepDraft({ step: "source" })}>添加技能指令</AddRowTrigger>
                <button type="button" onClick={() => setPrepDraft({ step: "option", source: "commandSpell" })}><i className="command-diamond" />使用令咒</button>
                <button type="button" onClick={() => onSelectEnemy ? onSelectEnemy() : setPrepDraft({ step: "target", source: "enemyTarget", option: "select" })}><i className="command-diamond" />选择敌方目标</button>
              </div>
            ) : prepDraft.step === "source" ? (
              <div className="battle-choice-row">
                {draftPartyMembers.slice(0, 3).map((member, index) => {
                  const servant = member.servant;
                  return (
                    <ServantFaceButton
                      key={index}
                      servant={servant}
                      index={index}
                      faceSrc={servant ? faces[servant.variantKey] : null}
                      isSupport={member.isSupport}
                      onClick={() =>
                        setPrepDraft({
                          step: "option",
                          source: `servant_${index + 1}` as PrepSource,
                        })
                      }
                    />
                  );
                })}
                <MysticCodeChoice code={mysticCode} onClick={() => setPrepDraft({ step: "option", source: "equipment" })} />
              </div>
            ) : prepDraft.step === "option" ? (
              <div className="battle-choice-row">
                {prepDraft.source !== "equipment" &&
                  prepDraft.source !== "commandSpell" && (
                    (() => {
                      const index = sourceIndex(prepDraft.source) ?? 0;
                      const member = draftPartyMembers[index] ?? { servant: null, isSupport: false };
                      const servant = member.servant;
                      return (
                        <ServantFaceButton
                          servant={servant}
                          index={index}
                          faceSrc={servant ? faces[servant.variantKey] : null}
                          isSupport={member.isSupport}
                          onClick={() => setPrepDraft({ step: "source" })}
                        />
                      );
                    })()
                  )}
                {prepDraft.source === "equipment" && (
                  <MysticCodeChoice code={mysticCode} selected onClick={() => setPrepDraft({ step: "source" })} />
                )}
                {prepDraft.source === "commandSpell" && (
                  <button
                    type="button"
                    className="battle-square-btn selected"
                    onClick={() => setPrepDraft({ step: "source" })}
                  >
                    令咒
                  </button>
                )}
                <div className="battle-option-group">
                  {prepDraft.source === "commandSpell"
                    ? Object.entries(COMMAND_SPELL_LABELS).map(([value, label]) => (
                      <button
                        type="button"
                        key={value}
                        className="battle-option-btn"
                        onClick={() =>
                          setPrepDraft({
                            step: "target",
                            source: prepDraft.source,
                            option: value,
                          })
                        }
                      >
                        {label}
                      </button>
                    ))
                    : (
                      <SkillOptionButtons
                        servant={
                          prepDraft.source !== "equipment"
                            ? (draftPartyMembers[sourceIndex(prepDraft.source) ?? 0]?.servant ?? null)
                            : null
                        }
                skillIcons={skillIcons}
                entries={prepDraft.source === "equipment" ? mysticSkillEntries : undefined}
                onSelect={(skill) => selectPrepSkill(prepDraft.source, skill)}
                      />
                    )}
                </div>
              </div>
            ) : prepDraft.step === "skillSelection" ? (
              <div className="battle-choice-row">
                <div className="battle-option-group">
                  {prepDraft.options.map((option) => (
                    <button
                      type="button"
                      key={option.index}
                      className="battle-option-btn"
                      onClick={() => finishSkillSelection(prepDraft, option)}
                    >
                      {option.label}
                    </button>
                  ))}
                </div>
              </div>
            ) : prepDraft.step === "target" ? (
              <div className="battle-choice-row">
                {prepDraft.source === "enemyTarget" ? (
                  <EnemyTargetButtons
                    ariaLabel="选择敌方目标"
                    onChange={(target) => {
                      if (target) finishPrepAction(prepDraft, target);
                    }}
                  />
                ) : <>
                {prepDraft.allowNoTarget !== false && (
                  <button
                    type="button"
                    className="battle-option-btn"
                    onClick={() => finishPrepAction(prepDraft, null)}
                  >
                    无目标
                  </button>
                )}
                {draftPartyMembers.slice(0, 3).map((member, index) => {
                  const servant = member.servant;
                  return (
                    <ServantFaceButton
                      key={index}
                      servant={servant}
                      index={index}
                      faceSrc={servant ? faces[servant.variantKey] : null}
                      isSupport={member.isSupport}
                      onClick={() => finishPrepAction(prepDraft, `servant_${index + 1}`)}
                    />
                  );
                })}
                {prepDraft.source === "equipment" &&
                  (disableAutoSkillTargetRecognition ||
                    mysticCodeSkill(mysticCode, prepDraft.option)?.targetingMode == null ||
                    mysticCodeSkill(mysticCode, prepDraft.option)?.targetingMode === "unknown") && (
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
              </div>
            ) : (
              <div className="battle-choice-row order-change">
                {Array.from(
                  { length: 6 },
                  (_, index) => draftPartyMembers[index] ?? { servant: null, isSupport: false }
                ).map((member, index) => {
                  const servant = member.servant;
                  const slot = `servant_${index + 1}` as PartySlot;
                  const needsFront = prepDraft.front == null;
                  const selectable = Boolean(servant) && (needsFront ? index < 3 : index >= 3);
                  return (
                    <ServantFaceButton
                      key={index}
                      servant={servant}
                      index={index}
                      faceSrc={servant ? faces[servant.variantKey] : null}
                      isSupport={member.isSupport}
                      disabled={!selectable}
                      selected={prepDraft.front === slot}
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
              </div>
            )}
          </div>
        </div>
      </section>}

      {(step == null || step === "enemy") && <EnemyTargetSelector
        value={scene.enemyTarget}
        onChange={updateEnemyTarget}
      />}

      {(step == null || step === "attack") && <section className={`battle-phase${turnAttackModesEnabled ? " battle-attack-phase" : ""}`}>
        {turnAttackModesEnabled ? (
          <div className="battle-attack-heading">
            <div className="command-mode-options" role="group" aria-label="攻击模式">
              {([['normal', '普通模式'], ['critical', '暴击模式'], ['advanced', '高级模式']] as const).map(([mode, label]) => <button type="button" key={mode} aria-pressed={attackMode === mode} onClick={() => updateAttackMode(mode)}><i className="command-diamond" />{label}</button>)}
            </div>
            <div className="battle-phase-label">攻击阶段</div>
          </div>
        ) : (
          <div className="battle-phase-label">攻击阶段</div>
        )}
        {attackMode === "normal" ? (
        <div className="battle-action-list">
          {attackPriority.map((card, index) => {
            const rowDraft = attackDraft?.targetIndex === index ? attackDraft : null;
            const attackPartyMembers =
              attackActionLineups[index] ?? currentPartyMembers;
            const attackPartyServants = partyMembersToServants(attackPartyMembers);
            return (
              <div className="battle-action-row committed" key={card.id}>
                {index >= FIXED_ATTACK_CARD_COUNT ? (
                  <ActionDeleteButton
                    onClick={() =>
                      updateAttackPriority(attackPriority.filter((_, i) => i !== index))
                    }
                  />
                ) : (
                  card.card ? (
                    <ActionClearButton onClick={() => clearAttackAction(index)} />
                  ) : (
                    <span className="battle-action-delete-placeholder" aria-hidden />
                  )
                )}
                {index < FIXED_ATTACK_CARD_COUNT && (
                  <span className="battle-attack-slot-label">{attackSlotLabel(index)}</span>
                )}
                {rowDraft ? (
                  renderAttackDraft(rowDraft, attackPartyMembers)
                ) : (
                  <>
                    <AttackActionFace
                      card={card}
                      partyMembers={attackPartyMembers}
                      faces={faces}
                    />
                    <button
                      type="button"
                      className="battle-attack-edit"
                      onClick={() => setAttackDraft({ step: "source", targetIndex: index })}
                    >
                      <Text size="2" weight="medium">
                        {attackSummary(card, attackPartyServants)}
                      </Text>
                    </button>
                  </>
                )}
              </div>
            );
          })}
          <div className="battle-add-row">
            <DraftCancelButton
              visible={attackDraft?.targetIndex === null}
              onClick={() => setAttackDraft(null)}
            />
            {attackDraft?.targetIndex !== null ? (
              <AddRowTrigger
                onClick={() => setAttackDraft({ step: "source", targetIndex: null })}
              >
                添加一项新的行动
              </AddRowTrigger>
            ) : !attackDraft ? (
              <AddRowTrigger
                onClick={() => setAttackDraft({ step: "source", targetIndex: null })}
              >
                添加一项新的行动
              </AddRowTrigger>
            ) : (
              renderAttackDraft(attackDraft, currentAttackPartyMembers)
            )}
          </div>
        </div>
        ) : attackMode === "critical" ? (
          <CriticalStrategyEditor
            strategy={scene.criticalStrategy}
            partyMembers={currentPartyMembers}
            faces={faces}
            onChange={(criticalStrategy) =>
              onChange(emptyLegacyFields({ ...scene, criticalStrategy }))
            }
          />
        ) : (
          <GrandCardStrategyPanel
            strategy={{ customRules: scene.advancedCardStrategy?.customRules ?? [] }}
            partyMembers={currentPartyMembers}
            faces={faces}
            allowGrandServant={false}
            embedded
            onChange={(advancedCardStrategy) =>
              onChange(emptyLegacyFields({
                ...scene,
                advancedCardStrategy: {
                  customRules: advancedCardStrategy.customRules ?? [],
                },
              }))
            }
          />
        )}
      </section>}
    </div>
  );
}
