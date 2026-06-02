import { useEffect, useMemo, useState } from "react";
import type React from "react";
import { Text } from "@radix-ui/themes";
import { invoke, convertFileSrc } from "../tauri";
import {
  Cross2Icon,
  PersonIcon,
  PlusIcon,
} from "@radix-ui/react-icons";
import orderChangeIcon from "../../src-tauri/resources/images/icon_order_change.png";
import {
  deriveLineupAfterAttackCards,
  deriveLineupAfterPreparationActions,
} from "./partyServants";
import type {
  AttackCard,
  BattleScene,
  CommandSpellAction,
  EquipmentAction,
  OrderChangeSelection,
  PreparationAction,
  ServantAction,
} from "../types/command";
import type { Servant } from "../types/servant";

interface BattleSceneBlockProps {
  scene: BattleScene;
  partyServants: (Servant | null)[];
  onChange: (updated: BattleScene) => void;
}

type PrepSource = "equipment" | "commandSpell" | `servant_${1 | 2 | 3}`;
type AttackSource = `servant_${1 | 2 | 3}`;
type PartySlot = `servant_${1 | 2 | 3 | 4 | 5 | 6}`;
type PrepDraft =
  | { step: "source" }
  | { step: "option"; source: PrepSource }
  | { step: "target"; source: PrepSource; option: string }
  | {
      step: "orderChange";
      source: "equipment";
      option: string;
      front: PartySlot | null;
    };
type AttackDraft =
  | { step: "source"; targetIndex: number | null }
  | { step: "option"; source: AttackSource; targetIndex: number | null };

const FIXED_ATTACK_CARD_COUNT = 3;

const SKILLS = ["skill_1", "skill_2", "skill_3"] as const;
const SKILL_LABELS: Record<string, string> = {
  skill_1: "技能 1",
  skill_2: "技能 2",
  skill_3: "技能 3",
};

const COMMAND_SPELL_LABELS: Record<string, string> = {
  np_release: "宝具解放",
  restore: "灵基修复",
};

const ATTACK_OPTIONS = [
  { value: "np", label: "宝具" },
  { value: "buster", label: "B" },
  { value: "arts", label: "A" },
  { value: "quick", label: "Q" },
  { value: "all", label: "ALL" },
] as const;

const CARD_LABELS: Record<string, string> = {
  np: "宝具",
  buster: "红卡攻击",
  arts: "蓝卡攻击",
  quick: "绿卡攻击",
  all: "任意指令卡",
};

function normalizeAttackPriority(priority: AttackCard[]): AttackCard[] {
  const next = [...priority];
  while (next.length < FIXED_ATTACK_CARD_COUNT) {
    next.push({
      id: createId(`atk_fixed_${next.length + 1}`),
      card: null,
    });
  }
  return next;
}

function createId(prefix: string): string {
  return `${prefix}_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;
}

function emptyLegacyFields(scene: BattleScene): BattleScene {
  return {
    ...scene,
    servantActions: [],
    equipmentActions: [],
    commandSpellActions: [],
  };
}

function servantLabel(index: number, servant: Servant | null): string {
  return servant?.name_cn || `从者 ${index + 1}`;
}

function servantSlotIndex(source: string | null | undefined): number | null {
  const match = source?.match(/^servant_([1-6])$/);
  return match ? Number(match[1]) - 1 : null;
}

function sourceIndex(source: PrepSource | AttackSource): number | null {
  const index = servantSlotIndex(source);
  return index != null && index < 3 ? index : null;
}

function orderChangeSummary(
  orderChange: OrderChangeSelection,
  partyServants: (Servant | null)[]
): string | null {
  const frontIndex = servantSlotIndex(orderChange.front);
  const backIndex = servantSlotIndex(orderChange.back);
  if (frontIndex == null || backIndex == null) return null;
  return `${servantLabel(frontIndex, partyServants[frontIndex] ?? null)} ↔ ${servantLabel(backIndex, partyServants[backIndex] ?? null)}`;
}

function useServantFaces(partyServants: (Servant | null)[]) {
  const [faces, setFaces] = useState<Record<string, string | null>>({});
  const requests = useMemo(
    () =>
      partyServants
        .filter((servant): servant is Servant => Boolean(servant))
        .map((servant) => ({
          variantKey: servant.variantKey,
          servantId: servant.id,
          faceId: servant.faceId ?? null,
        }))
        .sort((a, b) => a.variantKey.localeCompare(b.variantKey)),
    [partyServants]
  );
  const key = JSON.stringify(requests);

  useEffect(() => {
    const parsed = JSON.parse(key) as typeof requests;
    const missing = parsed.filter(({ variantKey }) => !(variantKey in faces));
    if (missing.length === 0) return;
    let cancelled = false;
    Promise.all(
      missing.map((request) =>
        invoke<string | null>("get_servant_face_path", {
          servantId: request.servantId,
          faceId: request.faceId,
        })
          .then((path) => [request.variantKey, path ? convertFileSrc(path) : null] as const)
          .catch(() => [request.variantKey, null] as const)
      )
    ).then((results) => {
      if (cancelled) return;
      setFaces((prev) => {
        const next = { ...prev };
        for (const [variantKey, src] of results) next[variantKey] = src;
        return next;
      });
    });
    return () => {
      cancelled = true;
    };
    // Keep this effect keyed by the request set; including `faces`
    // would re-run every time the cache is filled.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  return faces;
}

function ServantFaceButton({
  servant,
  index,
  faceSrc,
  onClick,
  disabled = false,
  selected = false,
}: {
  servant: Servant | null;
  index: number;
  faceSrc: string | null | undefined;
  onClick: () => void;
  disabled?: boolean;
  selected?: boolean;
}) {
  const label = servantLabel(index, servant);
  return (
    <button
      type="button"
      className={`battle-face-btn${selected ? " selected" : ""}`}
      aria-label={label}
      disabled={disabled}
      onClick={onClick}
    >
      {faceSrc ? (
        <img src={faceSrc} alt="" draggable={false} />
      ) : (
        <PersonIcon width={24} height={24} aria-hidden />
      )}
    </button>
  );
}

function ServantInlineFace({
  servant,
  index,
  faceSrc,
}: {
  servant: Servant | null;
  index: number;
  faceSrc: string | null | undefined;
}) {
  return (
    <span className="battle-inline-face" aria-label={servantLabel(index, servant)}>
      {faceSrc ? (
        <img src={faceSrc} alt="" draggable={false} />
      ) : (
        <PersonIcon width={18} height={18} aria-hidden />
      )}
    </span>
  );
}

function PreparationActionSummary({
  action,
  partyServants,
  faces,
}: {
  action: PreparationAction;
  partyServants: (Servant | null)[];
  faces: Record<string, string | null>;
}) {
  const targetIndex = sourceIndex((action.target ?? "") as PrepSource);
  const orderChangeSlots =
    action.type === "equipment" && action.orderChange
      ? {
          front: servantSlotIndex(action.orderChange.front),
          back: servantSlotIndex(action.orderChange.back),
        }
      : null;
  let sourceFace: React.ReactNode;
  let sourceText: string;
  let actionText: string;

  if (action.type === "servant") {
    const src = sourceIndex((action.servant ?? "servant_1") as PrepSource) ?? 0;
    const servant = partyServants[src] ?? null;
    sourceFace = (
      <ServantInlineFace
        servant={servant}
        index={src}
        faceSrc={servant ? faces[servant.variantKey] : null}
      />
    );
    sourceText = servantLabel(src, servant);
    actionText = `释放 ${SKILL_LABELS[action.skill ?? ""] ?? "技能"}`;
  } else {
    sourceFace = (
      <span className="battle-inline-square">
        {action.type === "equipment" ? "御主" : "令咒"}
      </span>
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
      aria-label={actionSummary(action, partyServants)}
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

function AttackActionFace({
  card,
  partyServants,
  faces,
}: {
  card: AttackCard;
  partyServants: (Servant | null)[];
  faces: Record<string, string | null>;
}) {
  const match = card.card?.match(/^servant_([1-3])_/);
  if (!match) return null;
  const index = Number(match[1]) - 1;
  const servant = partyServants[index] ?? null;
  return (
    <ServantInlineFace
      servant={servant}
      index={index}
      faceSrc={servant ? faces[servant.variantKey] : null}
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
  partyServants: (Servant | null)[]
): string {
  if (action.type === "servant") {
    const src = sourceIndex((action.servant ?? "servant_1") as PrepSource) ?? 0;
    const target = sourceIndex((action.target ?? "") as PrepSource);
    return target == null
      ? `${servantLabel(src, partyServants[src] ?? null)} 释放 ${SKILL_LABELS[action.skill ?? ""] ?? "技能"}`
      : `${servantLabel(src, partyServants[src] ?? null)} 释放 ${SKILL_LABELS[action.skill ?? ""] ?? "技能"} to ${servantLabel(target, partyServants[target] ?? null)}`;
  }

  if (action.type === "equipment") {
    if (action.orderChange) {
      const summary = orderChangeSummary(action.orderChange, partyServants);
      return summary
        ? `御主礼装 释放 ${SKILL_LABELS[action.skill ?? ""] ?? "技能"} Order Change ${summary}`
        : `御主礼装 释放 ${SKILL_LABELS[action.skill ?? ""] ?? "技能"} Order Change`;
    }
    const target = sourceIndex((action.target ?? "") as PrepSource);
    return target == null
      ? `御主礼装 释放 ${SKILL_LABELS[action.skill ?? ""] ?? "技能"}`
      : `御主礼装 释放 ${SKILL_LABELS[action.skill ?? ""] ?? "技能"} to ${servantLabel(target, partyServants[target] ?? null)}`;
  }

  const target = sourceIndex((action.target ?? "") as PrepSource);
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
  onChange,
}: BattleSceneBlockProps) {
  const [prepDraft, setPrepDraft] = useState<PrepDraft | null>(null);
  const [attackDraft, setAttackDraft] = useState<AttackDraft | null>(null);
  const faces = useServantFaces(partyServants);
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
    const lineups: (Servant | null)[][] = [];
    let lineup = partyServants;
    for (const action of preparationActions) {
      lineups.push(lineup);
      lineup = deriveLineupAfterPreparationActions(lineup, [action]);
    }
    return lineups;
  }, [partyServants, preparationActions]);
  const currentPartyServants = useMemo(
    () => deriveLineupAfterPreparationActions(partyServants, preparationActions),
    [partyServants, preparationActions]
  );
  const attackPriority = useMemo(
    () => normalizeAttackPriority(scene.attackPriority ?? []),
    [scene.attackPriority]
  );
  const attackActionLineups = useMemo(
    () =>
      attackPriority.map((_, index) =>
        deriveLineupAfterAttackCards(
          currentPartyServants,
          attackPriority.slice(0, index)
        )
      ),
    [attackPriority, currentPartyServants]
  );
  const currentAttackPartyServants = useMemo(
    () => deriveLineupAfterAttackCards(currentPartyServants, attackPriority),
    [attackPriority, currentPartyServants]
  );

  const updatePreparationActions = (next: PreparationAction[]) => {
    onChange(emptyLegacyFields({ ...scene, preparationActions: next }));
  };

  const updateAttackPriority = (next: AttackCard[]) => {
    onChange(emptyLegacyFields({ ...scene, attackPriority: next }));
  };

  const finishPrepAction = (draft: Extract<PrepDraft, { step: "target" }>, target: string | null) => {
    let action: PreparationAction;
    if (draft.source === "equipment") {
      action = {
        type: "equipment",
        id: createId("eq"),
        skill: draft.option,
        target,
        orderChange: null,
      } satisfies EquipmentAction;
    } else if (draft.source === "commandSpell") {
      action = {
        type: "commandSpell",
        id: createId("cs"),
        spell: draft.option as CommandSpellAction["spell"],
        target,
      } satisfies CommandSpellAction;
    } else {
      action = {
        type: "servant",
        id: createId("sa"),
        servant: draft.source,
        skill: draft.option,
        target,
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
        back,
      },
    } satisfies EquipmentAction;
    updatePreparationActions([...preparationActions, action]);
    setPrepDraft(null);
  };

  const finishAttackAction = (source: AttackSource, option: string) => {
    const next = [...attackPriority];
    const card = `${source}_${option}`;
    if (attackDraft?.targetIndex != null) {
      next[attackDraft.targetIndex] = {
        ...(next[attackDraft.targetIndex] ?? { id: createId("atk") }),
        card,
      };
    } else {
      next.push({ id: createId("atk"), card });
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
    draftPartyServants: (Servant | null)[]
  ) =>
    draft.step === "source" ? (
      <div className="battle-choice-row inline">
        {draftPartyServants.slice(0, 3).map((servant, index) => (
          <ServantFaceButton
            key={index}
            servant={servant}
            index={index}
            faceSrc={servant ? faces[servant.variantKey] : null}
            onClick={() =>
              setAttackDraft({
                step: "option",
                source: `servant_${index + 1}` as AttackSource,
                targetIndex: draft.targetIndex,
              })
            }
          />
        ))}
      </div>
    ) : (
      <div className="battle-choice-row inline">
        <ServantFaceButton
          servant={draftPartyServants[sourceIndex(draft.source) ?? 0] ?? null}
          index={sourceIndex(draft.source) ?? 0}
          faceSrc={
            draftPartyServants[sourceIndex(draft.source) ?? 0]
              ? faces[
                  draftPartyServants[sourceIndex(draft.source) ?? 0]!
                    .variantKey
                ]
              : null
          }
          onClick={() =>
            setAttackDraft({
              step: "source",
              targetIndex: draft.targetIndex,
            })
          }
        />
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
                partyServants={preparationActionLineups[index] ?? partyServants}
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
                {currentPartyServants.slice(0, 3).map((servant, index) => (
                  <ServantFaceButton
                    key={index}
                    servant={servant}
                    index={index}
                    faceSrc={servant ? faces[servant.variantKey] : null}
                    onClick={() =>
                      setPrepDraft({
                        step: "option",
                        source: `servant_${index + 1}` as PrepSource,
                      })
                    }
                  />
                ))}
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
                    <ServantFaceButton
                      servant={currentPartyServants[sourceIndex(prepDraft.source) ?? 0] ?? null}
                      index={sourceIndex(prepDraft.source) ?? 0}
                      faceSrc={
                        currentPartyServants[sourceIndex(prepDraft.source) ?? 0]
                          ? faces[
                              currentPartyServants[sourceIndex(prepDraft.source) ?? 0]!
                                .variantKey
                            ]
                          : null
                      }
                      onClick={() => setPrepDraft({ step: "source" })}
                    />
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
                    : SKILLS.map((skill) => (
                        <button
                          type="button"
                          key={skill}
                          className="battle-option-btn"
                          onClick={() =>
                            setPrepDraft({
                              step: "target",
                              source: prepDraft.source,
                              option: skill,
                            })
                          }
                        >
                          {SKILL_LABELS[skill]}
                        </button>
                      ))}
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
                {currentPartyServants.slice(0, 3).map((servant, index) => (
                  <ServantFaceButton
                    key={index}
                    servant={servant}
                    index={index}
                    faceSrc={servant ? faces[servant.variantKey] : null}
                    onClick={() => finishPrepAction(prepDraft, `servant_${index + 1}`)}
                  />
                ))}
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
                {Array.from({ length: 6 }, (_, index) => currentPartyServants[index] ?? null).map((servant, index) => {
                  const slot = `servant_${index + 1}` as PartySlot;
                  const needsFront = prepDraft.front == null;
                  const selectable = Boolean(servant) && (needsFront ? index < 3 : index >= 3);
                  return (
                    <ServantFaceButton
                      key={index}
                      servant={servant}
                      index={index}
                      faceSrc={servant ? faces[servant.variantKey] : null}
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
        <div className="battle-phase-label">攻击阶段</div>
        <div className="battle-action-list">
          {attackPriority.map((card, index) => {
            const rowDraft = attackDraft?.targetIndex === index ? attackDraft : null;
            const attackPartyServants =
              attackActionLineups[index] ?? currentPartyServants;
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
                  renderAttackDraft(rowDraft, attackPartyServants)
                ) : (
                  <>
                    <AttackActionFace
                      card={card}
                      partyServants={attackPartyServants}
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
              renderAttackDraft(attackDraft, currentAttackPartyServants)
            )}
          </div>
        </div>
      </section>
    </div>
  );
}
