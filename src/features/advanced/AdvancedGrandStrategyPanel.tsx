import { useState, type CSSProperties } from "react";
import { Dialog, Flex, Button } from "@radix-ui/themes";
import {
  ChevronDownIcon,
  ChevronUpIcon,
  Cross2Icon,
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
import { FaceChip } from "./AdvancedFaceChip";
import { OptionCardRadioGroup } from "../../components/common/OptionCardRadioGroup";
import { SectionHeading } from "../../components/common/SectionHeading";
import {
  COMMAND_BG_BY_RULE_COLOR,
  DEFAULT_GRAND_CHAIN_PRIORITY,
  RULE_COLOR_DIALOG_DESCRIPTIONS,
  RULE_COLOR_DIALOG_LABELS,
  RULE_COLOR_LABELS,
  RULE_COLOR_OPTIONS,
  RULE_KIND_DIALOG_DESCRIPTIONS,
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
  GrandRuleKind,
} from "../../types/project";
import type { Servant } from "../../types/servant";

function ruleCardColorClass(color: GrandRuleColor): string {
  return color === "buster" || color === "arts" || color === "quick" ? ` ${color}` : "";
}

function ruleCardLabel(slot: GrandCardRuleSlotConfig, servant: Servant | null, slotIndex: number): string {
  const servantText = slot.grandServant ? "冠位从者" : servant?.name_cn ?? "未选择从者";
  const kindText = RULE_KIND_LABELS[slot.kind];
  const colorText = RULE_COLOR_LABELS[slot.color];
  return `第 ${slotIndex + 1} 张，${servantText}，${kindText}，${colorText}`;
}

function GrandRuleCardButton({
  slot,
  slotIndex,
  partyMembers,
  faces,
  onClick,
}: {
  slot: GrandCardRuleSlotConfig;
  slotIndex: number;
  partyMembers: PartyMember[];
  faces: Record<string, string | null>;
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
  const missingServant = !usesGrandServant && slot.servantId != null && servant == null;
  const color = slot.kind === "np" && servant?.noblePhantasmCard ? servant.noblePhantasmCard : slot.color;
  const faceSrc = servant ? faces[servant.variantKey] : null;
  const warningText = missingServant ? "失效" : null;
  const style: CSSProperties | undefined =
    color === "buster" || color === "arts" || color === "quick"
      ? { backgroundImage: `url(${COMMAND_BG_BY_RULE_COLOR[color]})` }
      : undefined;

  return (
    <button
      type="button"
      className={`grand-rule-card${ruleCardColorClass(color)}${missingServant ? " invalid" : ""}`}
      aria-label={ruleCardLabel({ ...slot, color }, servant, slotIndex)}
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
        <span className="grand-rule-card-empty">+</span>
      )}
      <span className={`grand-rule-card-meta${warningText ? " warning" : ""}`}>
        <span>{RULE_KIND_LABELS[slot.kind]}</span>
        {warningText ? <span>{warningText}</span> : null}
      </span>
    </button>
  );
}

function GrandRuleEditorServantPicker({
  editingCard,
  partyMembers,
  faces,
  onSelect,
}: {
  editingCard: GrandCardRuleSlotConfig;
  partyMembers: PartyMember[];
  faces: Record<string, string | null>;
  onSelect: (patch: Partial<GrandCardRuleSlotConfig>) => void;
}) {
  const grandSelected = editingCard.grandServant === true;
  return (
    <div className="grand-rule-editor-section">
      <SectionHeading className="grand-rule-section-heading">从者</SectionHeading>
      <div className="grand-rule-editor-servants">
        <button
          type="button"
          className={`grand-rule-editor-grand-option${grandSelected ? " selected" : ""}`}
          aria-pressed={grandSelected}
          onClick={() =>
            onSelect({
              grandServant: !grandSelected,
              memberId: null,
              slotIndex: null,
              servantId: null,
              isSupport: false,
            })
          }
        >
          冠位从者
        </button>
        {partyMembers.map((member, index) => {
          const servant = member.servant;
          const selected =
            !grandSelected &&
            servant != null &&
            (editingCard.memberId != null
              ? editingCard.memberId === member.memberId
              : editingCard.slotIndex === index) &&
            editingCard.servantId === servant.id &&
            editingCard.isSupport === member.isSupport;
          return (
            <div className="grand-rule-editor-servant-cell" key={`${servant?.variantKey ?? "empty"}-${index}`}>
              {index === 3 && <span className="battle-choice-separator" aria-hidden />}
              <FaceChip
                servant={servant}
                index={index}
                src={servant ? faces[servant.variantKey] : null}
                selected={selected}
                disabled={!servant}
                isSupport={member.isSupport}
                onClick={() =>
                  onSelect({
                    grandServant: false,
                    memberId: selected ? null : member.memberId ?? null,
                    slotIndex: selected ? null : index,
                    servantId: selected ? null : servant?.id ?? null,
                    isSupport: selected ? false : member.isSupport,
                  })
                }
              />
            </div>
          );
        })}
      </div>
    </div>
  );
}

function GrandRuleEditorKindPicker({
  value,
  onChange,
}: {
  value: GrandRuleKind;
  onChange: (kind: GrandRuleKind) => void;
}) {
  return (
    <div className="grand-rule-editor-section">
      <SectionHeading className="grand-rule-section-heading">指令卡类型</SectionHeading>
      <OptionCardRadioGroup
        value={value}
        className="grand-rule-kind-cards"
        onValueChange={(kind) => onChange(kind as GrandRuleKind)}
        options={RULE_KIND_OPTIONS.map((kind) => ({
          value: kind,
          title: RULE_KIND_DIALOG_LABELS[kind],
          description: RULE_KIND_DIALOG_DESCRIPTIONS[kind],
          className: "grand-rule-kind-card",
        }))}
      />
    </div>
  );
}

function GrandRuleEditorColorPicker({
  value,
  onChange,
}: {
  value: GrandRuleColor;
  onChange: (color: GrandRuleColor) => void;
}) {
  return (
    <div className="grand-rule-editor-section">
      <SectionHeading className="grand-rule-section-heading">指令卡颜色</SectionHeading>
      <OptionCardRadioGroup
        value={value}
        className="grand-rule-color-cards"
        onValueChange={(color) => onChange(color as GrandRuleColor)}
        options={RULE_COLOR_OPTIONS.map((color) => ({
          value: color,
          title: RULE_COLOR_DIALOG_LABELS[color],
          ariaLabel: RULE_COLOR_LABELS[color],
          description: RULE_COLOR_DIALOG_DESCRIPTIONS[color],
          visual:
            color === "any" ? (
              <span className="grand-rule-color-wheel" />
            ) : (
              <span
                className={`grand-rule-color-swatch ${color}`}
                style={{ backgroundImage: `url(${COMMAND_BG_BY_RULE_COLOR[color]})` }}
              />
            ),
          className: `grand-rule-color-card ${color}`,
        }))}
      />
    </div>
  );
}

function SortableGrandRuleRow({
  rule,
  index,
  partyMembers,
  faces,
  onDelete,
  onEditSlot,
}: {
  rule: GrandCardRuleConfig;
  index: number;
  partyMembers: PartyMember[];
  faces: Record<string, string | null>;
  onDelete: () => void;
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
  onChange,
}: {
  strategy?: GrandCardStrategy;
  partyMembers: PartyMember[];
  faces: Record<string, string | null>;
  onChange?: (strategy: GrandCardStrategy) => void;
}) {
  const [open, setOpen] = useState(false);
  const [editingSlot, setEditingSlot] = useState<{ ruleId: string; slotIndex: number } | null>(null);
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 6 } }));
  const normalized = normalizeGrandCardStrategy(strategy);
  const customRules = normalized.customRules ?? [];
  const partyLineup = partyMembersToServants(partyMembers);
  const editingRule = editingSlot
    ? customRules.find((rule) => rule.id === editingSlot.ruleId) ?? null
    : null;
  const editingCard =
    editingRule && editingSlot
      ? editingRule.slots[editingSlot.slotIndex] ?? defaultRuleSlot()
      : null;
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
    const rule = createDefaultCustomRule();
    persist({ customRules: [...customRules, rule] });
    setEditingSlot({ ruleId: rule.id, slotIndex: 0 });
  };

  const updateRule = (ruleId: string, updater: (rule: GrandCardRuleConfig) => GrandCardRuleConfig) => {
    updateRules(customRules.map((rule) => (rule.id === ruleId ? normalizeCustomRule(updater(rule), 0) : rule)));
  };

  const resetCustomRules = () => {
    persist({
      customRules: [],
      chainPriority: [...DEFAULT_GRAND_CHAIN_PRIORITY],
    });
    setEditingSlot(null);
  };

  const handleDragEnd = (event: DragEndEvent) => {
    const { active, over } = event;
    if (!over || active.id === over.id) return;
    const oldIndex = customRules.findIndex((rule) => rule.id === active.id);
    const newIndex = customRules.findIndex((rule) => rule.id === over.id);
    if (oldIndex < 0 || newIndex < 0) return;
    updateRules(arrayMove(customRules, oldIndex, newIndex));
  };

  const updateEditingSlot = (patch: Partial<GrandCardRuleSlotConfig>) => {
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
      {open && (
        <div className="grand-strategy-body">
          <DndContext sensors={sensors} collisionDetection={closestCenter} onDragEnd={handleDragEnd}>
            <SortableContext items={customRules.map((rule) => rule.id)} strategy={verticalListSortingStrategy}>
              <div className="grand-rule-list">
                {customRules.map((rule, index) => (
                  <SortableGrandRuleRow
                    key={rule.id}
                    rule={rule}
                    index={index}
                    partyMembers={partyMembers}
                    faces={faces}
                    onDelete={() => updateRules(customRules.filter((item) => item.id !== rule.id))}
                    onEditSlot={(slotIndex) => setEditingSlot({ ruleId: rule.id, slotIndex })}
                  />
                ))}
              </div>
            </SortableContext>
          </DndContext>
          <Flex gap="3" wrap="wrap">
            <Button type="button" variant="soft" onClick={addRule}>
              添加规则
            </Button>
            <Button type="button" variant="soft" color="gray" onClick={resetCustomRules}>
              恢复默认
            </Button>
          </Flex>
        </div>
      )}

      <Dialog.Root
        open={editingCard != null}
        onOpenChange={(dialogOpen) => {
          if (!dialogOpen) setEditingSlot(null);
        }}
      >
        <Dialog.Content maxWidth="640px" className="grand-rule-editor-dialog">
          <Dialog.Title>设置策略</Dialog.Title>
          {editingCard && (
            <div className="grand-rule-editor">
              <GrandRuleEditorServantPicker
                editingCard={editingCard}
                partyMembers={partyMembers}
                faces={faces}
                onSelect={(patch) => updateEditingSlot(patch)}
              />
              <GrandRuleEditorKindPicker
                value={editingCard.kind}
                onChange={(kind) => updateEditingSlot({ kind })}
              />
              {editingCard.kind !== "np" && (
                <GrandRuleEditorColorPicker
                  value={editingCard.color}
                  onChange={(color) => updateEditingSlot({ color })}
                />
              )}
            </div>
          )}
          <Flex justify="end" mt="4">
            <Dialog.Close>
              <Button type="button">完成</Button>
            </Dialog.Close>
          </Flex>
        </Dialog.Content>
      </Dialog.Root>
    </section>
  );
}
