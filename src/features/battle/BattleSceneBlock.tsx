import { useMemo, useState } from "react";
import type React from "react";
import { Button, Text } from "@radix-ui/themes";
import {
  Cross2Icon,
  PlusIcon,
} from "@radix-ui/react-icons";
import orderChangeIcon from "../../../src-tauri/resources/images/icon_order_change.png";
import { BattleActorIcon } from "../../components/common/BattleActorIcon";
import { battleActorLabel, servantLabel } from "../../components/common/battleActorLabels";
import { useServantFaceImages } from "../team/useServantFaceImages";
import { useServantSkillIcons } from "../team/useServantSkillIcons";
import { SkillOptionButtons } from "../../components/common/SkillOptionButtons";
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
  ENEMY_TARGETS,
  FIXED_ATTACK_CARD_COUNT,
  SKILL_LABELS,
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
  PreparationAction,
  ServantAction,
} from "../../types/command";
import type { Servant } from "../../types/servant";

interface BattleSceneBlockProps {
  scene: BattleTurn;
  partyServants: (Servant | null)[];
  partyMembers?: PartyMember[];
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
  const label = servantLabel(index, servant);
  return (
    <Button
      type="button"
      className={`battle-face-btn${selected ? " selected" : ""}`}
      aria-label={label}
      disabled={disabled}
      onClick={onClick}
      size="4"
    >
      <BattleActorIcon kind="servant" src={faceSrc} label={label} isSupport={isSupport} size="button" />
    </Button>
  );
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
}: {
  action: PreparationAction;
  partyMembers: PartyMember[];
  faces: Record<string, string | null>;
}) {
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
    actionText = `释放 ${SKILL_LABELS[action.skill ?? ""] ?? "技能"}`;
  } else {
    const kind = action.type === "equipment" ? "equipment" : "commandSpell";
    sourceFace = (
      <BattleActorIcon kind={kind} label={battleActorLabel({ kind })} size="inline" />
    );
    sourceText = action.type === "equipment" ? "御主礼装" : "令咒";
    actionText =
      action.type === "equipment"
        ? `释放 ${SKILL_LABELS[action.skill ?? ""] ?? "技能"}`
        : COMMAND_SPELL_LABELS[action.spell ?? ""] ?? "行动";
  }

  return (
    <span
      className="battle-action-summary"
      aria-label={actionSummary(action, partyMembers)}
    >
      {sourceFace}
      <Text size="2" weight="medium" className="battle-action-name">
        {sourceText} {actionText}
      </Text>
      {orderChangeSlots?.front != null && orderChangeSlots.back != null ? (
        <>
          <span className="battle-action-to">Order Change</span>
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

function ActionDeleteButton({ onClick }: { onClick: () => void }) {
  return (
    <button
      type="button"
      className="battle-action-delete"
      aria-label="删除行动"
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
    return target == null
      ? `${sourceLabel} 释放 ${SKILL_LABELS[action.skill ?? ""] ?? "技能"}`
      : `${sourceLabel} 释放 ${SKILL_LABELS[action.skill ?? ""] ?? "技能"} to ${servantLabel(target, partyServants[target] ?? null)}`;
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
  onChange,
}: BattleSceneBlockProps) {
  const [prepDraft, setPrepDraft] = useState<PrepDraft | null>(null);
  const [attackDraft, setAttackDraft] = useState<AttackDraft | null>(null);
  const initialPartyMembers = useMemo(
    () => partyMembers ?? toPartyMembers(partyServants),
    [partyMembers, partyServants]
  );
  const initialPartyServants = partyMembersToServants(initialPartyMembers);
  const faces = useServantFaceImages(initialPartyServants);
  const skillIcons = useServantSkillIcons(initialPartyServants);
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

  const updateEnemyTarget = (enemyTarget: EnemyTarget | null) => {
    onChange(emptyLegacyFields({ ...scene, enemyTarget }));
  };

  const targetRef = (target: string | null) => {
    const ref = memberRefAt(currentPartyMembers, target);
    return {
      targetMemberId: ref.memberId,
      targetServantId: ref.servantId,
      targetIsSupport: ref.isSupport,
    };
  };

  const servantRef = (servant: string | null) => {
    const ref = memberRefAt(currentPartyMembers, servant);
    return {
      servantMemberId: ref.memberId,
      servantId: ref.servantId,
      servantIsSupport: ref.isSupport,
    };
  };

  const orderChangeRef = (key: "front" | "back", slot: PartySlot) => {
    const ref = memberRefAt(currentPartyMembers, slot);
    return {
      [`${key}MemberId`]: ref.memberId,
      [`${key}ServantId`]: ref.servantId,
      [`${key}IsSupport`]: ref.isSupport,
    };
  };

  const finishPrepAction = (draft: Extract<PrepDraft, { step: "target" }>, target: string | null) => {
    let action: PreparationAction;
    if (draft.source === "equipment") {
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
        target,
        ...targetRef(target),
      } satisfies ServantAction;
    }
    updatePreparationActions([...preparationActions, action]);
    setPrepDraft(null);
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
    updatePreparationActions([...preparationActions, action]);
    setPrepDraft(null);
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
      <section className="battle-phase">
        <div className="battle-phase-label">准备阶段</div>
        <div className="battle-action-list">
          {preparationActions.map((action, index) => (
            <div className="battle-action-row committed" key={action.id}>
              <ActionDeleteButton
                onClick={() =>
                  updatePreparationActions(
                    preparationActions.filter((_, i) => i !== index)
                  )
                }
              />
              <PreparationActionSummary
                action={action}
                partyMembers={preparationActionLineups[index] ?? initialPartyMembers}
                faces={faces}
              />
            </div>
          ))}
          <div className="battle-add-row">
            <DraftCancelButton
              visible={prepDraft != null}
              onClick={() => setPrepDraft(null)}
            />
            {!prepDraft ? (
              <button
                type="button"
                className="battle-add-trigger"
                onClick={() => setPrepDraft({ step: "source" })}
              >
                <span className="battle-plus-box">
                  <PlusIcon width={16} height={16} />
                </span>
                <Text size="2" weight="medium">
                  添加一项新的行动
                </Text>
              </button>
            ) : prepDraft.step === "source" ? (
              <div className="battle-choice-row">
                {currentPartyMembers.slice(0, 3).map((member, index) => {
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
                <span className="battle-choice-separator" aria-hidden />
                <button
                  type="button"
                  className="battle-square-btn"
                  onClick={() => setPrepDraft({ step: "option", source: "equipment" })}
                >
                  御主<br />礼装
                </button>
                <button
                  type="button"
                  className="battle-square-btn"
                  onClick={() =>
                    setPrepDraft({ step: "option", source: "commandSpell" })
                  }
                >
                  令咒
                </button>
              </div>
            ) : prepDraft.step === "option" ? (
              <div className="battle-choice-row">
                {prepDraft.source !== "equipment" &&
                  prepDraft.source !== "commandSpell" && (
                    (() => {
                      const index = sourceIndex(prepDraft.source) ?? 0;
                      const member = currentPartyMembers[index] ?? { servant: null, isSupport: false };
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
                  <button
                    type="button"
                    className="battle-square-btn selected"
                    onClick={() => setPrepDraft({ step: "source" })}
                  >
                    御主<br />礼装
                  </button>
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
                            ? (currentPartyMembers[sourceIndex(prepDraft.source) ?? 0]?.servant ?? null)
                            : null
                        }
                        skillIcons={skillIcons}
                        onSelect={(skill) => setPrepDraft({ step: "target", source: prepDraft.source, option: skill })}
                      />
                    )}
                </div>
              </div>
            ) : prepDraft.step === "target" ? (
              <div className="battle-choice-row">
                <button
                  type="button"
                  className="battle-option-btn"
                  onClick={() => finishPrepAction(prepDraft, null)}
                >
                  无目标
                </button>
                {currentPartyMembers.slice(0, 3).map((member, index) => {
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
              </div>
            ) : (
              <div className="battle-choice-row order-change">
                {Array.from(
                  { length: 6 },
                  (_, index) => currentPartyMembers[index] ?? { servant: null, isSupport: false }
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
      </section>

      <section className="battle-phase">
        <div className="battle-phase-label">敌方目标选择</div>
        <div className="battle-action-list">
          <div className="battle-action-row committed">
            <span className="battle-action-delete-placeholder" aria-hidden />
            <div
              className="battle-enemy-target-row"
              role="group"
              aria-label="敌方目标选择"
            >
              <div className="battle-enemy-target-grid">
                {ENEMY_TARGETS.map((target) => (
                  <button
                    type="button"
                    key={target.value}
                    aria-label={target.label}
                    className={`battle-enemy-target${scene.enemyTarget === target.value ? " selected" : ""}`}
                    onClick={() =>
                      updateEnemyTarget(
                        scene.enemyTarget === target.value ? null : target.value
                      )
                    }
                  >
                    <span>{target.text}</span>
                  </button>
                ))}
              </div>
            </div>
          </div>
        </div>
      </section>

      <section className="battle-phase">
        <div className="battle-phase-label">攻击阶段</div>
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
              <button
                type="button"
                className="battle-add-trigger"
                onClick={() => setAttackDraft({ step: "source", targetIndex: null })}
              >
                <span className="battle-plus-box">
                  <PlusIcon width={16} height={16} />
                </span>
                <Text size="2" weight="medium">
                  添加一项新的行动
                </Text>
              </button>
            ) : !attackDraft ? (
              <button
                type="button"
                className="battle-add-trigger"
                onClick={() => setAttackDraft({ step: "source", targetIndex: null })}
              >
                <span className="battle-plus-box">
                  <PlusIcon width={16} height={16} />
                </span>
                <Text size="2" weight="medium">
                  添加一项新的行动
                </Text>
              </button>
            ) : (
              renderAttackDraft(attackDraft, currentAttackPartyMembers)
            )}
          </div>
        </div>
      </section>
    </div>
  );
}
