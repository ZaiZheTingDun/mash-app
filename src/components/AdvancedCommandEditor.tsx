import { useCallback, useEffect, useMemo, useState, type CSSProperties } from "react";
import { Dialog, Flex, Text, Button, Select } from "@radix-ui/themes";
import {
  Cross2Icon,
  PlusIcon,
} from "@radix-ui/react-icons";
import { convertFileSrc, invoke } from "../tauri";
import { BattleActorIcon } from "./BattleActorIcon";
import { battleActorLabel, servantLabel } from "./battleActorLabels";
import { FaceChip } from "./AdvancedFaceChip";
import { GrandCardStrategyPanel } from "./AdvancedGrandStrategyPanel";
import {
  COMMAND_SPELL_LABELS,
  COMMAND_BG_BY_SUIT,
  EMPTY_STARTUP_ACTIONS,
  SKILLS,
  SKILL_LABELS,
  autoNpOptionLabel,
  commandCardAria,
  createId,
  createDefaultScene,
  defaultCommandCard,
  mainGrandBackSlot,
  normalizeGrandServants,
  normalizeScene,
  npCardLabel,
  prepSummary,
  priorityLabel,
  servantSlotIndex,
  type FrontServant,
  type PartySlot,
  type PrepDraft,
} from "./advancedCommandModel";
import {
  deriveMembersAfterPreparationActions,
  partyMembersToServants,
  toPartyMembers,
  type PartyMember,
} from "./partyServants";
import type {
  AdvancedBattleScene,
  AdvancedCommandCardCondition,
  CommandSpellAction,
  EquipmentAction,
  OrderChangeSelection,
  PreparationAction,
  ServantAction,
} from "../types/command";
import type {
  GrandCardPriority,
  GrandCardStrategy,
  GrandClass,
  GrandNpCard,
  GrandServantConfig,
} from "../types/project";
import type { Servant } from "../types/servant";
import orderChangeIcon from "../../src-tauri/resources/images/icon_order_change.png";
import commandBgArts from "../../src-tauri/resources/images/command_bg/command_bg_a.png";

interface AdvancedCommandEditorProps {
  projectId: string | null;
  partyLineup: (Servant | null)[];
  partyMembers?: PartyMember[];
  grandServants?: GrandServantConfig[];
  grandClass?: GrandClass;
  grandCardStrategy?: GrandCardStrategy;
  grandCardPriorityEnabled?: boolean;
  onGrandServantsChange?: (grandServants: GrandServantConfig[]) => void;
  onGrandCardStrategyChange?: (strategy: GrandCardStrategy) => void;
}

function useServantFaces(partyLineup: (Servant | null)[]) {
  const [faces, setFaces] = useState<Record<string, string | null>>({});
  const requests = useMemo(
    () =>
      partyLineup
        .filter((servant): servant is Servant => Boolean(servant))
        .map((servant) => ({
          key: servant.variantKey,
          servantId: servant.id,
          faceId: servant.faceId ?? null,
        })),
    [partyLineup]
  );

  useEffect(() => {
    let cancelled = false;
    const missing = requests.filter(({ key }) => !(key in faces));
    if (missing.length === 0) return;
    Promise.all(
      missing.map((request) =>
        invoke<string | null>("get_servant_face_path", {
          servantId: request.servantId,
          faceId: request.faceId,
        })
          .then((path) => [request.key, path ? convertFileSrc(path) : null] as const)
          .catch(() => [request.key, null] as const)
      )
    ).then((results) => {
      if (cancelled) return;
      setFaces((prev) => {
        const next = { ...prev };
        for (const [key, src] of results) next[key] = src;
        return next;
      });
    });
    return () => {
      cancelled = true;
    };
  }, [faces, requests]);

  return faces;
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

function GrandOutputSettings({
  partyMembers,
  faces,
  grandServants,
  onChange,
}: {
  partyMembers: PartyMember[];
  faces: Record<string, string | null>;
  grandServants: GrandServantConfig[];
  onChange?: (grandServants: GrandServantConfig[]) => void;
}) {
  const [settingsIndex, setSettingsIndex] = useState<number | null>(null);
  const partyLineup = partyMembersToServants(partyMembers);
  const normalized = normalizeGrandServants(grandServants);
  const selectedSlots = new Set(normalized.map((item) => item.slotIndex));
  const settings = settingsIndex == null ? null : normalized[settingsIndex] ?? null;
  const settingsServant = settings == null ? null : partyLineup[settings.slotIndex] ?? null;

  const persist = (next: GrandServantConfig[]) => {
    onChange?.(normalizeGrandServants(next));
  };
  const addGrandServant = (slotIndex: number) => {
    if (
      normalized.length >= 2 ||
      selectedSlots.has(slotIndex) ||
      partyLineup[slotIndex] == null
    ) {
      return;
    }
    persist([...normalized, { slotIndex, npCard: "auto", priority: "damage" }]);
  };
  const removeGrandServant = (index: number) => {
    persist(normalized.filter((_, itemIndex) => itemIndex !== index));
    setSettingsIndex(null);
  };
  const updateGrandServant = (
    index: number,
    patch: Partial<Pick<GrandServantConfig, "npCard" | "priority">>,
  ) => {
    persist(normalized.map((item, itemIndex) => (itemIndex === index ? { ...item, ...patch } : item)));
  };
  const moveToMain = (index: number) => {
    if (index <= 0) return;
    const next = [...normalized];
    const [item] = next.splice(index, 1);
    next.unshift(item);
    persist(next);
    setSettingsIndex(0);
  };

  return (
    <>
      <div className="advanced-grand-output">
        <div className="advanced-grand-output-row">
          <Text size="2" weight="medium" className="advanced-grand-output-label">冠位</Text>
          <div className="advanced-grand-output-slots">
            {normalized.map((config, index) => {
              const servant = partyLineup[config.slotIndex] ?? null;
              return (
                <button
                  key={`${config.slotIndex}-${index}`}
                  type="button"
                  className="grand-servant-tile"
                  aria-label={`${index === 0 ? "主" : "副"}冠位${servant ? `：${servant.name_cn}` : ""}`}
                  onClick={() => setSettingsIndex(index)}
                >
                  <span className="grand-role-badge">{index === 0 ? "主" : "副"}</span>
                  {servant && faces[servant.variantKey] ? (
                    <img src={faces[servant.variantKey] ?? undefined} alt={servant.name_cn} draggable={false} />
                  ) : (
                    <span className="grand-servant-placeholder">{servant?.name_cn ?? "未选择"}</span>
                  )}
                  <span className="grand-np-badge">{npCardLabel(config.npCard, servant?.noblePhantasmCard)}</span>
                  <span className="grand-priority-badge">{priorityLabel(config.priority)}</span>
                </button>
              );
            })}
            {normalized.length < 2 && <div className="grand-servant-empty">选择冠位从者</div>}
          </div>
        </div>
        <div className="advanced-grand-output-row">
          <Text size="2" weight="medium" className="advanced-grand-output-label">辅助</Text>
          <div className="battle-choice-row">
            {partyLineup.slice(0, 6).map((servant, index) => (
              <FaceChip
                key={index}
                servant={servant}
                index={index}
                src={servant ? faces[servant.variantKey] : null}
                selected={selectedSlots.has(index)}
                disabled={
                  servant == null ||
                  selectedSlots.has(index) ||
                  normalized.length >= 2
                }
                isSupport={partyMembers[index]?.isSupport ?? false}
                onClick={() => addGrandServant(index)}
              />
            ))}
          </div>
        </div>
      </div>

      <Dialog.Root
        open={settings != null}
        onOpenChange={(open) => {
          if (!open) setSettingsIndex(null);
        }}
      >
        <Dialog.Content maxWidth="420px">
          <Dialog.Title>冠位从者设置</Dialog.Title>
          <Dialog.Description className="sr-only">
            设置冠位从者的宝具颜色和出卡策略。
          </Dialog.Description>
          {settings && settingsIndex != null && (
            <Flex direction="column" gap="4">
              <label className="grand-setting-field">
                <Text size="2" weight="medium">宝具颜色</Text>
                <Select.Root
                  value={settings.npCard ?? "auto"}
                  onValueChange={(value) =>
                    updateGrandServant(settingsIndex, {
                      npCard: value as GrandNpCard,
                    })
                  }
                >
                  <Select.Trigger aria-label="宝具颜色" />
                  <Select.Content>
                    <Select.Item value="auto">{autoNpOptionLabel(settingsServant)}</Select.Item>
                    <Select.Item value="buster">红卡</Select.Item>
                    <Select.Item value="arts">蓝卡</Select.Item>
                    <Select.Item value="quick">绿卡</Select.Item>
                  </Select.Content>
                </Select.Root>
              </label>
              <label className="grand-setting-field">
                <Text size="2" weight="medium">出卡策略</Text>
                <Select.Root
                  value={settings.priority ?? "damage"}
                  onValueChange={(value) =>
                    updateGrandServant(settingsIndex, {
                      priority: value as GrandCardPriority,
                    })
                  }
                >
                  <Select.Trigger aria-label="出卡策略" />
                  <Select.Content>
                    <Select.Item value="damage">伤害优先</Select.Item>
                    <Select.Item value="np">NP 优先</Select.Item>
                  </Select.Content>
                </Select.Root>
              </label>
              <Flex justify="between" gap="3">
                <Button type="button" variant="soft" color="red" onClick={() => removeGrandServant(settingsIndex)}>
                  移除
                </Button>
                <Flex gap="3">
                  {settingsIndex > 0 && (
                    <Button type="button" variant="soft" onClick={() => moveToMain(settingsIndex)}>
                      设为主
                    </Button>
                  )}
                  <Dialog.Close>
                    <Button type="button">完成</Button>
                  </Dialog.Close>
                </Flex>
              </Flex>
            </Flex>
          )}
        </Dialog.Content>
      </Dialog.Root>
    </>
  );
}

function AdvancedPreparationActionSummary({
  action,
  partyMembers,
  faces,
}: {
  action: PreparationAction;
  partyMembers: PartyMember[];
  faces: Record<string, string | null>;
}) {
  const partyLineup = partyMembersToServants(partyMembers);
  const targetIndex = servantSlotIndex(action.target);
  const orderChangeSlots =
    action.type === "equipment" && action.orderChange
      ? {
          front: servantSlotIndex(action.orderChange.front),
          back: servantSlotIndex(action.orderChange.back),
        }
      : null;

  let sourceFace;
  let sourceText: string;
  let actionText: string;

  if (action.type === "servant") {
    const source = servantSlotIndex(action.servant) ?? 0;
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
    <span className="battle-action-summary" aria-label={prepSummary(action, partyLineup)}>
      {sourceFace}
      <Text size="2" weight="medium" className="battle-action-name">
        {sourceText} {actionText}
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

function AdvancedCommandCardButton({
  card,
  partyMembers,
  faces,
  onClick,
}: {
  card: AdvancedCommandCardCondition;
  partyMembers: PartyMember[];
  faces: Record<string, string | null>;
  onClick: () => void;
}) {
  const servantIndex = servantSlotIndex(card.servant);
  const member = servantIndex == null ? null : partyMembers[servantIndex] ?? null;
  const servant = member?.servant ?? null;
  const faceSrc = servant ? faces[servant.variantKey] : null;
  const unset = card.servant === "any" && card.suit === "any";
  const grayscale = card.suit === "any";
  const style = {
    "--command-card-bg": `url(${card.suit === "any" ? commandBgArts : COMMAND_BG_BY_SUIT[card.suit]})`,
  } as CSSProperties;

  return (
    <button
      type="button"
      className={`advanced-command-card${grayscale ? " unset" : ""}`}
      style={style}
      aria-label={commandCardAria(card)}
      onClick={onClick}
    >
      {!unset && servantIndex != null && (
        <BattleActorIcon
          kind="servant"
          src={faceSrc}
          label={servantLabel(servantIndex, servant)}
          isSupport={member?.isSupport ?? false}
          size="button"
          className="advanced-command-card-face"
        />
      )}
    </button>
  );
}

function AdvancedStrategyEditor({
  scene,
  partyMembers,
  faces,
  grandServants,
  grandCardStrategy,
  grandCardPriorityEnabled,
  onGrandServantsChange,
  onGrandCardStrategyChange,
  onChange,
}: {
  scene: AdvancedBattleScene;
  partyMembers: PartyMember[];
  faces: Record<string, string | null>;
  grandServants: GrandServantConfig[];
  grandCardStrategy?: GrandCardStrategy;
  grandCardPriorityEnabled: boolean;
  onGrandServantsChange?: (grandServants: GrandServantConfig[]) => void;
  onGrandCardStrategyChange?: (strategy: GrandCardStrategy) => void;
  onChange: (scene: AdvancedBattleScene) => void;
}) {
  const [editingCardSlot, setEditingCardSlot] = useState<number | null>(null);
  const [controlDraft, setControlDraft] = useState<PrepDraft | null>(null);
  const [prepDraft, setPrepDraft] = useState<PrepDraft | null>(null);
  const partyLineup = useMemo(() => partyMembersToServants(partyMembers), [partyMembers]);
  const grandAutoOrderChange = scene.grandAutoOrderChange ?? null;
  const mainGrandSlot = mainGrandBackSlot(grandServants);
  const mainGrandServant = mainGrandSlot == null ? null : partyLineup[mainGrandSlot] ?? null;
  const startupSelectableSlots = useMemo(() => {
    const slots = [0, 1, 2];
    if (mainGrandSlot != null && grandAutoOrderChange === true) {
      slots.push(mainGrandSlot);
    }
    return slots;
  }, [grandAutoOrderChange, mainGrandSlot]);
  const commandConditions = scene.commandConditions ?? [0, 1, 2, 3, 4].map(defaultCommandCard);
  const controlActions = scene.controlActions ?? EMPTY_STARTUP_ACTIONS;
  const startupActions = scene.startupActions ?? EMPTY_STARTUP_ACTIONS;
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
  const startupActionLineups = useMemo(() => {
    const lineups: PartyMember[][] = [];
    let lineup = postControlMembers;
    for (const action of startupActions) {
      lineups.push(lineup);
      lineup = deriveMembersAfterPreparationActions(lineup, [action]);
    }
    return lineups;
  }, [postControlMembers, startupActions]);
  const currentPartyMembers = useMemo(
    () => deriveMembersAfterPreparationActions(postControlMembers, startupActions),
    [postControlMembers, startupActions]
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
    onChange({ ...scene, startupActions: actions });
  };

  const updateControlActions = (actions: PreparationAction[]) => {
    onChange({ ...scene, controlActions: actions });
  };

  const makePrepAction = (
    draft: Extract<PrepDraft, { step: "target" }>,
    target: string | null
  ): PreparationAction => {
    if (draft.source === "equipment") {
      return {
        type: "equipment",
        id: createId("eq"),
        skill: draft.option,
        target,
        orderChange: null,
      } satisfies EquipmentAction;
    }
    if (draft.source === "commandSpell") {
      return {
        type: "commandSpell",
        id: createId("cs"),
        spell: draft.option as CommandSpellAction["spell"],
        target,
      } satisfies CommandSpellAction;
    }
    return {
      type: "servant",
      id: createId("sa"),
      servant: draft.source,
      skill: draft.option,
      target,
    } satisfies ServantAction;
  };

  const finishPrepAction = (
    draft: Extract<PrepDraft, { step: "target" }>,
    target: string | null
  ) => {
    updateStartupActions([...startupActions, makePrepAction(draft, target)]);
    setPrepDraft(null);
  };

  const finishControlAction = (
    draft: Extract<PrepDraft, { step: "target" }>,
    target: string | null
  ) => {
    updateControlActions([...controlActions, makePrepAction(draft, target)]);
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
          back,
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
          back,
        } satisfies OrderChangeSelection,
      } satisfies EquipmentAction,
    ]);
    setControlDraft(null);
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
                主冠位从者配置在后排，是否自动换位至前排？
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
                {mainGrandServant ? ` ${mainGrandServant.name_cn} ` : "主冠位从者"}
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
              />
            </div>
          ))}
          {!controlDraft ? (
            <button
              type="button"
              className="battle-add-trigger"
              onClick={() => setControlDraft({ step: "source" })}
            >
              <span className="advanced-delete-spacer" aria-hidden />
              <span className="battle-plus-box">
                <PlusIcon width={16} height={16} />
              </span>
              <Text size="2" weight="medium">添加控制行动</Text>
            </button>
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
                    : SKILLS.map((skill) => (
                        <button
                          type="button"
                          className="battle-option-btn"
                          key={skill}
                          onClick={() => setControlDraft({ step: "target", source: controlDraft.source, option: skill })}
                        >
                          {SKILL_LABELS[skill]}
                        </button>
                      ))}
                </>
              ) : controlDraft.step === "target" ? (
                <>
                  <button type="button" className="battle-option-btn" onClick={() => finishControlAction(controlDraft, null)}>
                    无目标
                  </button>
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
        <div className="battle-phase-label">启动阶段</div>
        <div className="advanced-rule-section">
          {startupActions.map((action, index) => (
            <div className="battle-action-row committed advanced-action-row" key={action.id}>
              <button
                type="button"
                className="advanced-inline-delete"
                aria-label="删除启动行动"
                onClick={() => updateStartupActions(startupActions.filter((_, i) => i !== index))}
              >
                <Cross2Icon width={13} height={13} />
              </button>
              <AdvancedPreparationActionSummary
                action={action}
                partyMembers={startupActionLineups[index] ?? partyMembers}
                faces={faces}
              />
            </div>
          ))}
          {!prepDraft ? (
            <button
              type="button"
              className="battle-add-trigger"
              onClick={() => setPrepDraft({ step: "source" })}
            >
              <span className="advanced-delete-spacer" aria-hidden />
              <span className="battle-plus-box">
                <PlusIcon width={16} height={16} />
              </span>
              <Text size="2" weight="medium">添加启动行动</Text>
            </button>
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
                    : SKILLS.map((skill) => (
                        <button
                          type="button"
                          className="battle-option-btn"
                          key={skill}
                          onClick={() => setPrepDraft({ step: "target", source: prepDraft.source, option: skill })}
                        >
                          {SKILL_LABELS[skill]}
                        </button>
                      ))}
                </>
              ) : prepDraft.step === "target" ? (
                <>
                  <button type="button" className="battle-option-btn" onClick={() => finishPrepAction(prepDraft, null)}>
                    无目标
                  </button>
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
                  onValueChange={(value) =>
                    updateCommandCard(editingCard.slot, (prev) => ({
                      ...prev,
                      servant: value as AdvancedCommandCardCondition["servant"],
                      minCritChance: null,
                    }))
                  }
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
  grandServants = [],
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
  const [loaded, setLoaded] = useState(() => !projectId);
  const initialPartyMembers = useMemo(
    () => partyMembers ?? toPartyMembers(partyLineup),
    [partyMembers, partyLineup]
  );
  const initialPartyLineup = useMemo(
    () => partyMembersToServants(initialPartyMembers),
    [initialPartyMembers]
  );
  const faces = useServantFaces(initialPartyLineup);

  useEffect(() => {
    if (!projectId) return;
    let cancelled = false;
    invoke<AdvancedBattleScene[]>("load_advanced_battle_scenes", { projectId })
      .then((saved) => {
        if (cancelled) return;
        const first = saved.length > 0 ? normalizeScene(saved[0]) : createDefaultScene();
        setScene(first);
      })
      .catch(() => {
        if (cancelled) return;
        setScene(createDefaultScene());
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
          grandServants={grandServants}
          grandCardStrategy={grandCardStrategy}
          grandCardPriorityEnabled={grandCardPriorityEnabled}
          onGrandServantsChange={onGrandServantsChange}
          onGrandCardStrategyChange={onGrandCardStrategyChange}
          onChange={handleSceneChange}
        />
      </div>
    </Flex>
  );
}
