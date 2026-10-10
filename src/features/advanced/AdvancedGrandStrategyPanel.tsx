import { Fragment, useEffect, useRef, useState, type CSSProperties } from "react";
import { Dialog, Flex, Button } from "@radix-ui/themes";
import {
  ChevronDownIcon,
  ChevronUpIcon,
  Cross2Icon,
  DragHandleDots2Icon,
  PlusIcon,
} from "@radix-ui/react-icons";
import {
  DndContext,
  PointerSensor,
  closestCenter,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  arrayMove,
  useSortable,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { BattleActorIcon } from "../../components/common/BattleActorIcon";
import { servantLabel } from "../../components/common/battleActorLabels";
import { ServantChoice } from "../../components/common/ServantChoice";
import { CommandDraftHeading } from "../../components/common/CommandDraftHeading";
import { AttackCardOptionButtons } from "../../components/common/AttackCardOptionButtons";
import { SectionHeading } from "../../components/common/SectionHeading";
import {
  COMMAND_BG_BY_RULE_COLOR,
  DEFAULT_GRAND_CHAIN_PRIORITY,
  RULE_COLOR_DIALOG_LABELS,
  RULE_COLOR_LABELS,
  RULE_COLOR_OPTIONS,
  RULE_KIND_DIALOG_LABELS,
  RULE_KIND_LABELS,
  RULE_KIND_OPTIONS,
  createDefaultCustomRule,
  defaultRuleSlot,
  normalizeCustomRule,
  normalizeCustomRules,
  normalizeGrandCardStrategy,
} from "./advancedCommandModel";
import {
  partyMembersToServants,
  type PartyMember,
} from "../team/partyServants";
import type {
  GrandCardRuleConfig,
  GrandCardRuleSlotConfig,
  GrandCardStrategy,
  GrandRuleColor,
} from "../../types/project";
import type { Servant } from "../../types/servant";

function ruleCardColorClass(color: GrandRuleColor): string {
  return color === "buster" || color === "arts" || color === "quick" ? ` ${color}` : "";
}

function ruleCardLabel(slot: GrandCardRuleSlotConfig, servant: Servant | null, slotIndex: number): string {
  const servantText = slot.grandServant ? "冠位从者" : servant?.name_cn ?? "任意从者";
  const kindText = RULE_KIND_LABELS[slot.kind];
  const colorText = RULE_COLOR_LABELS[slot.color];
  return `第 ${slotIndex + 1} 张，${servantText}，${kindText}，${colorText}`;
}

function GrandRuleCardButton({
  slot,
  slotIndex,
  partyMembers,
  faces,
  isEditing,
  onClick,
}: {
  slot: GrandCardRuleSlotConfig;
  slotIndex: number;
  partyMembers: PartyMember[];
  faces: Record<string, string | null>;
  isEditing: boolean;
  onClick: () => void;
}) {
  const partyLineup = partyMembersToServants(partyMembers);
  const memberIndex =
    slot.slotIndex != null
      ? slot.slotIndex
      : slot.servantId == null
        ? -1
        : partyLineup.findIndex((servant) => servant?.id === slot.servantId);
  const member = memberIndex >= 0 ? partyMembers[memberIndex] : null;
  const servant = member?.servant ?? null;
  const usesGrandServant = slot.grandServant === true;
  const color = slot.kind === "np" && servant?.noblePhantasmCard ? servant.noblePhantasmCard : slot.color;
  const faceSrc = servant ? faces[servant.variantKey] : null;
  const style: CSSProperties | undefined =
    color === "buster" || color === "arts" || color === "quick"
      ? { backgroundImage: `url(${COMMAND_BG_BY_RULE_COLOR[color]})` }
      : undefined;

  return (
    <button
      type="button"
      className={`grand-rule-card${ruleCardColorClass(color)}${isEditing ? " is-editing" : ""}`}
      aria-label={ruleCardLabel({ ...slot, color }, servant, slotIndex)}
      aria-expanded={isEditing}
      style={style}
      onClick={onClick}
    >
      {usesGrandServant ? (
        <span className="grand-rule-card-grand">冠</span>
      ) : servant ? (
        <BattleActorIcon
          kind="servant"
          src={faceSrc}
          label={servantLabel(memberIndex, servant)}
          isSupport={member?.isSupport ?? false}
          size="button"
          className="grand-rule-card-face"
        />
      ) : (
        <span className="grand-rule-card-empty">任</span>
      )}
      <span className="grand-rule-card-meta">
        <span className="command-rule-servant-name">{usesGrandServant ? "冠位从者" : servant?.name_cn ?? "任意从者"}</span>
        <span>{RULE_KIND_LABELS[slot.kind]}{slot.kind !== "np" && ` · ${RULE_COLOR_LABELS[color]}`}</span>
      </span>
    </button>
  );
}

function GrandRuleServantChoices({ editingCard, partyMembers, faces, allowGrandServant, selectedOnly = false, onSelect }: {
  editingCard: GrandCardRuleSlotConfig;
  partyMembers: PartyMember[];
  faces: Record<string, string | null>;
  allowGrandServant: boolean;
  selectedOnly?: boolean;
  onSelect: (patch: Partial<GrandCardRuleSlotConfig>) => void;
}) {
  const grandSelected = editingCard.grandServant === true;
  const anySelected = !grandSelected && editingCard.memberId == null && editingCard.slotIndex == null && editingCard.servantId == null;
  const specialChoice = (grand: boolean) => {
    const selected = grand ? grandSelected : anySelected;
    if (selectedOnly && !selected) return null;
    const label = grand ? "冠位从者" : "任意从者";
    return <button type="button" className={`command-actor-choice${selectedOnly ? " selected" : ""}`}
      aria-label={label} aria-pressed={selected} onClick={() => onSelect({ grandServant: grand, memberId: null, slotIndex: null, servantId: null, isSupport: false })}>
      <span className="command-empty-face" aria-hidden="true"><i className="command-diamond" /></span>
      <span className="command-actor-copy"><b>{label}</b></span>
    </button>;
  };
  return <>
    {Array.from({ length: 6 }, (_, index) => partyMembers[index] ?? { servant: null, isSupport: false }).map((member, index) => {
      const servant = member.servant;
      const selected = !grandSelected && servant != null &&
        (editingCard.memberId != null ? editingCard.memberId === member.memberId : editingCard.slotIndex === index) &&
        editingCard.servantId === servant.id && editingCard.isSupport === member.isSupport;
      if (selectedOnly && !selected) return null;
      return <ServantChoice key={index} servant={servant} index={index} src={servant ? faces[servant.variantKey] : null}
        isSupport={member.isSupport} selected={selectedOnly && selected}
        onClick={() => onSelect({ grandServant: false, memberId: member.memberId ?? null, slotIndex: index, servantId: servant?.id ?? null, isSupport: member.isSupport })} />;
    })}
    {allowGrandServant && specialChoice(true)}
    {specialChoice(false)}
  </>;
}

function SortableGrandRuleRow({
  rule,
  index,
  partyMembers,
  faces,
  onDelete,
  editingSlotIndex,
  onEditSlot,
}: {
  rule: GrandCardRuleConfig;
  index: number;
  partyMembers: PartyMember[];
  faces: Record<string, string | null>;
  onDelete: () => void;
  editingSlotIndex: number | null;
  onEditSlot: (slotIndex: number) => void;
}) {
  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id: rule.id });
  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
  } as CSSProperties;

  return (
    <div
      ref={setNodeRef}
      className={`grand-rule-row${isDragging ? " dragging" : ""}`}
      style={style}
      {...attributes}
      {...listeners}
    >
      <span className="command-rule-label"><DragHandleDots2Icon /><small>{String(index + 1).padStart(2, "0")}</small><span>{rule.name || `规则 ${index + 1}`}</span></span>
      <button
        type="button"
        className="advanced-inline-delete"
        aria-label={`删除${rule.name || `规则 ${index + 1}`}`}
        onPointerDown={(event) => event.stopPropagation()}
        onClick={(event) => {
          event.stopPropagation();
          onDelete();
        }}
      >
        <Cross2Icon width={13} height={13} />
      </button>
      <div className="grand-rule-cards" aria-label={rule.name || `规则 ${index + 1}`}>
        {rule.slots.map((slot, slotIndex) => (
          <GrandRuleCardButton
            key={slotIndex}
            slot={slot}
            slotIndex={slotIndex}
            partyMembers={partyMembers}
            faces={faces}
            isEditing={editingSlotIndex === slotIndex}
            onClick={() => onEditSlot(slotIndex)}
          />
        ))}
      </div>
      <div className="grand-rule-row-spacer" aria-hidden />
    </div>
  );
}

export function GrandCardStrategyPanel({
  strategy,
  partyMembers,
  faces,
  allowGrandServant = true,
  embedded = false,
  onChange,
}: {
  strategy?: GrandCardStrategy;
  partyMembers: PartyMember[];
  faces: Record<string, string | null>;
  allowGrandServant?: boolean;
  embedded?: boolean;
  onChange?: (strategy: GrandCardStrategy) => void;
}) {
  const [open, setOpen] = useState(false);
  const [cardDraft, setCardDraft] = useState<GrandCardRuleSlotConfig | null>(null);
  const inlineRef = useRef<HTMLElement>(null);
  const [editorStep, setEditorStep] = useState<"source" | "kind" | "color">("source");
  const [editingSlot, setEditingSlot] = useState<{ ruleId: string; slotIndex: number } | null>(null);
  useEffect(() => { if (editingSlot) inlineRef.current?.scrollIntoView({block:"nearest"}); }, [editingSlot]);
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 6 } }));
  const normalized = normalizeGrandCardStrategy(strategy);
  const customRules = normalized.customRules ?? [];
  const partyLineup = partyMembersToServants(partyMembers);
  const editingRule = editingSlot
    ? customRules.find((rule) => rule.id === editingSlot.ruleId) ?? null
    : null;
  const editingCard = cardDraft ?? (
    editingRule && editingSlot
      ? editingRule.slots[editingSlot.slotIndex] ?? defaultRuleSlot()
      : null);
  const editingServant =
    editingCard?.slotIndex == null
      ? null
      : editingCard.memberId != null
        ? partyMembers.find((member) => member.memberId === editingCard.memberId)?.servant ?? null
        : partyLineup[editingCard.slotIndex] ?? null;

  const persist = (patch: Partial<GrandCardStrategy>) => {
    onChange?.({
      ...normalized,
      ...patch,
    });
  };

  const updateRules = (rules: GrandCardRuleConfig[]) => {
    persist({ customRules: normalizeCustomRules(rules) });
  };

  const addRule = () => {
    updateRules([...customRules, createDefaultCustomRule()]);
  };

  const updateRule = (ruleId: string, updater: (rule: GrandCardRuleConfig) => GrandCardRuleConfig) => {
    updateRules(customRules.map((rule) => (rule.id === ruleId ? normalizeCustomRule(updater(rule), 0) : rule)));
  };

  const resetCustomRules = () => {
    persist({
      customRules: [],
      chainPriority: [...DEFAULT_GRAND_CHAIN_PRIORITY],
    });
    setEditingSlot(null); setCardDraft(null);
  };

  const handleDragEnd = (event: DragEndEvent) => {
    const { active, over } = event;
    if (!over || active.id === over.id) return;
    const oldIndex = customRules.findIndex((rule) => rule.id === active.id);
    const newIndex = customRules.findIndex((rule) => rule.id === over.id);
    if (oldIndex < 0 || newIndex < 0) return;
    updateRules(arrayMove(customRules, oldIndex, newIndex));
  };

  const commitEditingSlot = (patch: Partial<GrandCardRuleSlotConfig>) => {
    if (!editingSlot) return;
    updateRule(editingSlot.ruleId, (rule) => ({
      ...rule,
      slots: rule.slots.map((slot, index) => {
        if (index !== editingSlot.slotIndex) return slot;
        const next = { ...slot, ...patch };
        if (patch.memberId !== undefined || patch.slotIndex !== undefined || patch.servantId !== undefined) {
          const servant =
            next.memberId != null
              ? partyMembers.find((member) => member.memberId === next.memberId)?.servant ?? null
              : next.slotIndex == null
              ? null
              : partyLineup[next.slotIndex] ?? null;
          if (next.kind === "np" && servant?.noblePhantasmCard) {
            next.color = servant.noblePhantasmCard;
          }
        }
        if (patch.kind === "np" && editingServant?.noblePhantasmCard) {
          next.color = editingServant.noblePhantasmCard;
        }
        if (patch.kind === "any" && next.color == null) {
          next.color = "any";
        }
        return next;
      }),
    }));
  };

  const closeEditor = () => { setEditingSlot(null); setCardDraft(null); };
  const editSlot = (rule: GrandCardRuleConfig, slotIndex: number) => {
    setCardDraft({ ...rule.slots[slotIndex] });
    setEditorStep("source");
    setEditingSlot({ ruleId: rule.id, slotIndex });
  };
  const updateEditingSlot = (patch: Partial<GrandCardRuleSlotConfig>) => setCardDraft(previous => previous ? { ...previous, ...patch } : previous);
  const saveSlot = (patch: Partial<GrandCardRuleSlotConfig>) => {
    if (editingCard) commitEditingSlot({ ...editingCard, ...patch });
    closeEditor();
  };
  const editorFields = editingCard && <div className="command-inline-draft command-rule-draft">
    <CommandDraftHeading title="设置策略" hint={editorStep === "source" ? "选择从者" : editorStep === "kind" ? "选择指令卡类型" : "选择指令卡颜色"}
      onCancel={closeEditor} reselectLabel={editorStep === "color" ? "重选类型" : "重选"}
      onReselect={editorStep !== "source" ? () => setEditorStep(editorStep === "color" ? "kind" : "source") : undefined} />
    <div className={`battle-choice-row${editorStep === "source" ? " command-rule-sources" : ""}`}>
      <GrandRuleServantChoices editingCard={editingCard} partyMembers={partyMembers} faces={faces} allowGrandServant={allowGrandServant}
        selectedOnly={editorStep !== "source"} onSelect={patch => {
          if (editorStep !== "source") { setEditorStep("source"); return; }
          updateEditingSlot(patch);
          setEditorStep("kind");
        }} />
      {editorStep === "kind" && <AttackCardOptionButtons label="指令卡类型"
        options={RULE_KIND_OPTIONS.map(kind => ({ value: kind, label: RULE_KIND_DIALOG_LABELS[kind], iconIndex: kind === "any" ? null : kind === "np" ? 0 : 4 }))}
        onSelect={kind => {
          if (kind === "np") saveSlot({ kind });
          else { updateEditingSlot({ kind }); setEditorStep("color"); }
        }} />}
      {editorStep === "color" && <AttackCardOptionButtons label="指令卡颜色" className="command-rule-colors"
          options={RULE_COLOR_OPTIONS.map(color => ({ value: color, label: RULE_COLOR_DIALOG_LABELS[color], iconIndex: ({ any: 4, buster: 1, arts: 2, quick: 3 })[color] }))}
          onSelect={color => saveSlot({ color })} />}
    </div>
  </div>;
  const inlineEditor = editingCard && <section ref={inlineRef} className="command-rule-inline-editor" aria-label="设置策略">{editorFields}</section>;

  const strategyBody = (
    <div className="grand-strategy-body">
      <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
        <SortableContext items={customRules.map((rule) => rule.id)} strategy={verticalListSortingStrategy}>
          <div className="grand-rule-list">
            {customRules.map((rule, index) => (
              <Fragment key={rule.id}><SortableGrandRuleRow
                rule={rule}
                index={index}
                partyMembers={partyMembers}
                faces={faces}
                editingSlotIndex={editingSlot?.ruleId === rule.id ? editingSlot.slotIndex : null}
                onDelete={() => { if (editingSlot?.ruleId === rule.id) closeEditor(); updateRules(customRules.filter((item) => item.id !== rule.id)); }}
                onEditSlot={slotIndex => editSlot(rule, slotIndex)}
              />{embedded && editingSlot?.ruleId === rule.id && inlineEditor}</Fragment>
            ))}
          </div>
        </SortableContext>
      </DndContext>
      <Flex gap="3" wrap="wrap" className="command-rule-actions">
        <Button type="button" variant="soft" disabled={editingSlot != null} onClick={addRule}>
          {embedded && <PlusIcon />}
          添加规则
        </Button>
        {!embedded && <Button type="button" variant="soft" color="gray" className="command-rule-reset" onClick={resetCustomRules}>
          恢复默认
        </Button>}
      </Flex>
    </div>
  );

  const editorDialog = (
    <Dialog.Root
      open={editingCard != null}
      onOpenChange={(dialogOpen) => {
        if (!dialogOpen) closeEditor();
      }}
    >
      <Dialog.Content maxWidth="640px" className="grand-rule-editor-dialog">
        <Dialog.Title className="sr-only">设置策略</Dialog.Title>
        {editorFields}
      </Dialog.Content>
    </Dialog.Root>
  );

  if (embedded) {
    return (
      <div className="grand-card-strategy-section embedded">
        <SectionHeading rail english="CARD STRATEGY" accessory={<Button type="button" variant="ghost" color="gray" className="command-rule-reset" onClick={resetCustomRules}>恢复默认</Button>}>指令卡策略</SectionHeading>
        {strategyBody}
      </div>
    );
  }

  return (
    <section className="battle-phase advanced-strategy-section grand-card-strategy-section">
      <button
        type="button"
        className="grand-strategy-summary"
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        <span className="grand-strategy-chevron" aria-hidden>
          {open ? <ChevronUpIcon width={16} height={16} /> : <ChevronDownIcon width={16} height={16} />}
        </span>
        <span className="grand-strategy-title">指令卡策略</span>
      </button>
      {open && strategyBody}
      {editorDialog}
    </section>
  );
}
