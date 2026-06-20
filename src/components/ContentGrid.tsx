import { useState } from "react";
import {
  Box,
  Button,
  Flex,
  Text,
} from "@radix-ui/themes";
import { PlusIcon, PersonIcon } from "@radix-ui/react-icons";
import {
  DndContext,
  closestCenter,
  PointerSensor,
  useSensor,
  useSensors,
  type DragEndEvent,
} from "@dnd-kit/core";
import {
  SortableContext,
  useSortable,
  arrayMove,
  rectSortingStrategy,
} from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { ServantSelectDialog } from "./ServantSelectDialog";
import { CraftEssenceSelectDialog } from "./CraftEssenceSelectDialog";
import { useCeCards, usePortraits } from "./contentGridAssets";
import {
  CraftEssenceOverlay,
  GrandCraftEssenceOverlay,
} from "./ContentGridOverlays";
import {
  SupportRequirementSummary,
  SupportSettingsDialog,
} from "./SupportSettingsDialog";
import {
  grandClassToServantClass,
  hasConfiguredLevels,
  normalizeSupportAppendSkillLevels,
  normalizeSupportGrandCraftEssenceIds,
  normalizeSupportGrandCraftEssenceMlbRequired,
  normalizeSupportSkillLevels,
} from "./supportSettingsModel";
import type { Servant } from "../types/servant";
import type { CraftEssence } from "../types/craftEssence";
import type {
  Project,
  SupportGrandBondCeMode,
  SupportGrandCraftEssenceIds,
  SupportGrandCraftEssenceMlbRequired,
  SupportAppendSkillLevelMins,
  SupportSkillLevelMins,
} from "../types/project";
import mlbIconSrc from "../../src-tauri/resources/images/icon_mlb_mark.png";
import grandBondIconSrc from "../../src-tauri/resources/images/icon_grand_bond_ce.png";
import grandBondNpIconSrc from "../../src-tauri/resources/images/icon_grand_bond_ce_np.png";

export interface SlotItem {
  id: string;
  type: "servant" | "support";
  servant: Servant | null;
  /**
   * Pinned craft essence for this slot. Persisted on the project but
   * only consumed by the runner for the support slot today (party-slot
   * CEs are stored for future auto-equip work). Renders as a small
   * picker tile beneath each servant slot.
  */
  craftEssence: CraftEssence | null;
  craftEssenceMlbRequired?: boolean;
}

interface ContentGridProps {
  servants: Servant[];
  craftEssences: CraftEssence[];
  slots: SlotItem[];
  onSlotsChange: (slots: SlotItem[]) => void;
  /**
   * Active project, the source of truth for the pinned support servant.
   * When `null`, the support slot still renders but selecting a servant
   * is a no-op until a project is created/selected.
  */
  activeProject: Project | null;
  onUpdateActiveProject: (next: Project) => Promise<void> | void;
}

interface SortableSlotProps {
  slot: SlotItem;
  portraitSrc: string | null | undefined;
  ceCardSrc: string | null | undefined;
  supportGrandMode: boolean;
  supportGrandCraftEssences: (CraftEssence | null)[];
  supportGrandCeCardSrcs: (string | null | undefined)[];
  supportGrandCeMlbRequired: SupportGrandCraftEssenceMlbRequired;
  supportGrandBondCeMode: SupportGrandBondCeMode;
  mlbIconSrc: string | null | undefined;
  bondIconSrc: string | null | undefined;
  bondNpIconSrc: string | null | undefined;
  supportNpLevel: number | null | undefined;
  supportSkillLevels: SupportSkillLevelMins;
  supportAppendSkillLevels: SupportAppendSkillLevelMins;
  onSelect: () => void;
  onCeSelect: () => void;
  onCeClear: () => void;
  onGrandModeToggle: () => void;
  onGrandCeSelect: (index: number) => void;
  onGrandCeClear: (index: number) => void;
  onSupportSettingsOpen: () => void;
}

/**
 * Map a servant's rarity to the CSS class that drives its portrait
 * frame gradient. Rarity buckets follow FGO's metallic frame scheme:
 *
 *   1 ★ / 2 ★  → brass
 *   3 ★        → silver
 *   4 ★ / 5 ★  → gold
 *
 * Returns an empty string for unknown / out-of-range values so the
 * caller can opt out of the gradient (the default flat-grey frame
 * still renders).
 */
function rarityFrameClass(rarity: number): string {
  if (rarity >= 4) return "rarity-gold";
  if (rarity === 3) return "rarity-silver";
  if (rarity >= 1) return "rarity-brass";
  return "";
}

/**
 * One unified servant+CE card. The vertical stack is:
 *   1. `.slot-header` — empty reserved band (future: ascension/lv/etc).
 *   2. `.servant-portrait` — aspect-ratio-locked image area that owns:
 *        - the portrait `<img>` (or placeholder),
 *        - the top-right `SUPPORT` badge on the support slot,
 *        - a `.ce-overlay` strip pinned to the bottom edge that covers
 *          the lower slice of the portrait (replaces the standalone CE
 *          tile that used to sit below this card),
 *        - on the support slot, either the requirement-summary chips
 *          or the "技能/宝具设置" button as an overlay above the CE strip.
 *
 * Empty / empty-support states keep the same skeleton so the card
 * height matches a filled card; only the middle portrait region swaps
 * for a `+ 选择从者` / `助战` placeholder.
 */
function SortableSlot({
  slot,
  portraitSrc,
  ceCardSrc,
  supportGrandMode,
  supportGrandCraftEssences,
  supportGrandCeCardSrcs,
  supportGrandCeMlbRequired,
  supportGrandBondCeMode,
  mlbIconSrc,
  bondIconSrc,
  bondNpIconSrc,
  supportNpLevel,
  supportSkillLevels,
  supportAppendSkillLevels,
  onSelect,
  onCeSelect,
  onCeClear,
  onGrandModeToggle,
  onGrandCeSelect,
  onGrandCeClear,
  onSupportSettingsOpen,
}: SortableSlotProps) {
  const {
    attributes,
    listeners,
    setNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id: slot.id });

  const style = {
    transform: CSS.Transform.toString(transform),
    transition,
    opacity: isDragging ? 0.4 : 1,
    zIndex: isDragging ? 10 : undefined,
  };

  const { servant } = slot;
  const isSupport = slot.type === "support";
  const rarityClass = servant ? rarityFrameClass(servant.rarity) : "";
  const hasSupportRequirements =
    supportNpLevel != null ||
    hasConfiguredLevels(supportSkillLevels) ||
    hasConfiguredLevels(supportAppendSkillLevels);

  return (
    <div
      ref={setNodeRef}
      style={style}
      className="slot-drag-wrapper"
      {...attributes}
      {...listeners}
    >
      <Flex direction="column" className="slot-card">
        <div className="slot-header" />
        <div
          className={`servant-portrait${servant ? " filled" : " empty"}${isSupport ? " support" : ""}${isSupport && supportGrandMode ? " grand-support" : ""}${rarityClass ? ` ${rarityClass}` : ""}`}
          onClick={onSelect}
        >
          {servant ? (
            portraitSrc ? (
              <img
                className="servant-portrait-img"
                src={portraitSrc}
                alt={servant.name_cn}
                draggable={false}
              />
            ) : (
              <Flex
                direction="column"
                align="center"
                justify="center"
                className="servant-portrait-placeholder"
              >
                <Text size="2" weight="bold" align="center">
                  {servant.name_cn}
                </Text>
              </Flex>
            )
          ) : isSupport ? (
            <Flex
              direction="column"
              align="center"
              justify="center"
              gap="1"
              className="servant-portrait-placeholder"
            >
              <PersonIcon width={28} height={28} className="support-slot-icon" />
              <Text size="1" weight="medium" className="support-slot-label">
                助战
              </Text>
            </Flex>
          ) : (
            <Flex
              direction="column"
              align="center"
              justify="center"
              gap="1"
              className="servant-portrait-placeholder"
            >
              <PlusIcon width={28} height={28} className="servant-slot-icon" />
              <Text size="1" color="gray">
                选择从者
              </Text>
            </Flex>
          )}
          {isSupport && (
            <span className="support-corner-badge">SUPPORT</span>
          )}
          {isSupport && (
            <button
              type="button"
              className={`support-grand-toggle${supportGrandMode ? " active" : ""}`}
              aria-pressed={supportGrandMode}
              aria-label={supportGrandMode ? "关闭冠位模式" : "开启冠位模式"}
              onClick={(event) => {
                event.stopPropagation();
                onGrandModeToggle();
              }}
            >
              冠位
            </button>
          )}
          {isSupport && hasSupportRequirements && (
            <SupportRequirementSummary
              npLevel={supportNpLevel}
              skillLevels={supportSkillLevels}
              appendSkillLevels={supportAppendSkillLevels}
              onOpen={onSupportSettingsOpen}
            />
          )}
          {isSupport && !hasSupportRequirements && (
            <Button
              type="button"
              size="1"
              variant="surface"
              color="gray"
              className="support-settings-button support-settings-overlay-button"
              onClick={(event) => {
                event.stopPropagation();
                onSupportSettingsOpen();
              }}
            >
              技能/宝具设置
            </Button>
          )}
          {isSupport && supportGrandMode ? (
            <GrandCraftEssenceOverlay
              craftEssences={supportGrandCraftEssences}
              cardSrcs={supportGrandCeCardSrcs}
              mlbRequired={supportGrandCeMlbRequired}
              mlbIconSrc={mlbIconSrc}
              grandBondCeMode={supportGrandBondCeMode}
              bondIconSrc={bondIconSrc}
              bondNpIconSrc={bondNpIconSrc}
              onSelect={onGrandCeSelect}
              onClear={onGrandCeClear}
            />
          ) : (
            <CraftEssenceOverlay
              craftEssence={slot.craftEssence}
              cardSrc={ceCardSrc}
              mlbRequired={isSupport ? slot.craftEssenceMlbRequired ?? true : false}
              mlbIconSrc={mlbIconSrc}
              onSelect={onCeSelect}
              onClear={onCeClear}
            />
          )}
        </div>
      </Flex>
    </div>
  );
}

export function ContentGrid({
  servants,
  craftEssences,
  slots,
  onSlotsChange,
  activeProject,
  onUpdateActiveProject,
}: ContentGridProps) {
  const [dialogOpen, setDialogOpen] = useState(false);
  const [activeSlotId, setActiveSlotId] = useState<string | null>(null);
  const [ceDialogOpen, setCeDialogOpen] = useState(false);
  const [activeCeTarget, setActiveCeTarget] = useState<
    { type: "slot"; slotId: string } | { type: "grand"; index: number } | null
  >(null);
  const [supportSettingsOpen, setSupportSettingsOpen] = useState(false);

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 5 } })
  );

  // The support slot is project-owned: ignore whatever happens to be in
  // `slots[?].servant` for the support row and resolve from the active
  // project's pinned id instead. Falls back to `null` (empty slot) when no
  // project is active or the pin doesn't resolve to a known servant.
  const supportPinned: Servant | null = (() => {
    const id = activeProject?.supportServantId;
    if (id == null) return null;
    return (
      servants.find((s) => s.variantKey === activeProject?.supportServantVariantKey) ??
      servants.find((s) => s.id === id) ??
      null
    );
  })();

  const displaySlots: SlotItem[] = slots.map((s) =>
    s.type === "support" ? { ...s, servant: supportPinned } : s
  );

  const portraitMap = usePortraits(
    displaySlots
      .map((s) => s.servant)
      .filter((servant): servant is Servant => servant != null)
  );

  const ceCardMap = useCeCards(
    [
      ...displaySlots.map((s) => s.craftEssence?.id),
      ...(activeProject?.supportGrandCraftEssenceIds ?? []),
    ]
      .filter((id): id is number => id != null)
  );

  const handleDragEnd = (event: DragEndEvent) => {
    const { active, over } = event;
    if (!over || active.id === over.id) return;

    const oldIndex = slots.findIndex((s) => s.id === active.id);
    const newIndex = slots.findIndex((s) => s.id === over.id);
    onSlotsChange(arrayMove(slots, oldIndex, newIndex));
  };

  const handleSlotClick = (slot: SlotItem) => {
    setActiveSlotId(slot.id);
    setDialogOpen(true);
  };

  const handleCeSlotClick = (slotId: string) => {
    setActiveCeTarget({ type: "slot", slotId });
    setCeDialogOpen(true);
  };

  const supportGrandCeIds = normalizeSupportGrandCraftEssenceIds(
    activeProject?.supportGrandCraftEssenceIds,
  );
  const supportGrandCraftEssences = supportGrandCeIds.map((id) =>
    id == null ? null : craftEssences.find((ce) => ce.id === id) ?? null
  );
  const supportGrandCeCardSrcs = supportGrandCeIds.map((id) =>
    id == null ? null : ceCardMap[id]
  );
  const supportGrandCeMlbRequired = normalizeSupportGrandCraftEssenceMlbRequired(
    activeProject?.supportGrandCraftEssenceMlbRequired,
  );
  const supportGrandBondCeMode = activeProject?.supportGrandBondCeMode ?? "any";
  const defaultServantClassFilter =
    activeProject?.advancedMode === true
      ? grandClassToServantClass(activeProject.grandClass)
      : undefined;

  const handleGrandModeToggle = () => {
    if (!activeProject) return;
    void onUpdateActiveProject({
      ...activeProject,
      supportGrandMode: !(activeProject.supportGrandMode ?? false),
      supportGrandCraftEssenceIds: supportGrandCeIds,
      supportGrandCraftEssenceMlbRequired: supportGrandCeMlbRequired,
      supportGrandBondCeMode,
    });
  };

  const handleGrandCeSlotClick = (index: number) => {
    setActiveCeTarget({ type: "grand", index });
    setCeDialogOpen(true);
  };

  const handleCeSelect = (ce: CraftEssence) => {
    if (!activeCeTarget) return;
    if (activeCeTarget.type === "slot") {
      onSlotsChange(
        slots.map((s) =>
          s.id === activeCeTarget.slotId
            ? {
                ...s,
                craftEssence: ce,
                craftEssenceMlbRequired: s.craftEssenceMlbRequired ?? true,
              }
            : s
        )
      );
      return;
    }
    if (!activeProject) return;
    const nextIds = [...supportGrandCeIds] as SupportGrandCraftEssenceIds;
    nextIds[activeCeTarget.index] = ce.id;
    void onUpdateActiveProject({
      ...activeProject,
      supportGrandCraftEssenceIds: nextIds,
      supportGrandCraftEssenceMlbRequired: supportGrandCeMlbRequired,
      supportGrandBondCeMode,
    });
  };

  const handleCeClear = (slotId: string) => {
    onSlotsChange(
      slots.map((s) =>
        s.id === slotId
          ? { ...s, craftEssence: null, craftEssenceMlbRequired: true }
          : s
      )
    );
  };

  const handleGrandCeClear = (index: number) => {
    if (!activeProject) return;
    const nextIds = [...supportGrandCeIds] as SupportGrandCraftEssenceIds;
    const nextMlb = [...supportGrandCeMlbRequired] as SupportGrandCraftEssenceMlbRequired;
    nextIds[index] = null;
    nextMlb[index] = true;
    void onUpdateActiveProject({
      ...activeProject,
      supportGrandCraftEssenceIds: nextIds,
      supportGrandCraftEssenceMlbRequired: nextMlb,
      supportGrandBondCeMode: index === 1 ? "any" : supportGrandBondCeMode,
    });
  };

  const handleCeMlbRequiredChange = (required: boolean) => {
    if (!activeCeTarget) return;
    if (activeCeTarget.type === "slot") {
      onSlotsChange(
        slots.map((s) =>
          s.id === activeCeTarget.slotId
            ? { ...s, craftEssenceMlbRequired: required }
            : s
        )
      );
      return;
    }
    if (!activeProject) return;
    const nextMlb = [...supportGrandCeMlbRequired] as SupportGrandCraftEssenceMlbRequired;
    nextMlb[activeCeTarget.index] = required;
    void onUpdateActiveProject({
      ...activeProject,
      supportGrandCraftEssenceIds: supportGrandCeIds,
      supportGrandCraftEssenceMlbRequired: nextMlb,
      supportGrandBondCeMode,
    });
  };

  const handleGrandBondCeModeChange = (mode: SupportGrandBondCeMode) => {
    if (!activeProject) return;
    void onUpdateActiveProject({
      ...activeProject,
      supportGrandCraftEssenceIds: supportGrandCeIds,
      supportGrandCraftEssenceMlbRequired: supportGrandCeMlbRequired,
      supportGrandBondCeMode: mode,
    });
  };

  const handleSelect = (servant: Servant) => {
    const target = slots.find((s) => s.id === activeSlotId);
    if (!target) return;
    if (target.type === "support") {
      // Persist the pin on the project so the runner can read it via
      // `RunConfig::support_servant_id`. Leave `slots` untouched for the
      // support row — the support visual derives from the project.
      if (activeProject) {
        void onUpdateActiveProject({
          ...activeProject,
          supportServantId: servant.id,
          supportServantVariantKey: servant.variantKey,
        });
      }
      return;
    }
    onSlotsChange(
      slots.map((s) => (s.id === activeSlotId ? { ...s, servant } : s))
    );
  };

  // Disallow picking the same servant into two non-support slots. The
  // support slot is intentionally exempt — the player can stack their own
  // copy of a friend's servant — so we only build the block-list for
  // party slots and exclude the slot currently being edited (so the user
  // can re-open the dialog on a filled slot without that slot's own
  // servant disappearing from the list).
  const activeSlot = slots.find((s) => s.id === activeSlotId);
  const activeCeSlot =
    activeCeTarget?.type === "slot"
      ? slots.find((s) => s.id === activeCeTarget.slotId)
      : null;
  const activeCeTargetMlbRequired =
    activeCeTarget?.type === "grand"
      ? supportGrandCeMlbRequired[activeCeTarget.index]
      : activeCeSlot?.craftEssenceMlbRequired ?? true;
  const showCeMlbOption =
    activeCeTarget?.type === "grand" || activeCeSlot?.type === "support";
  const showGrandBondOption =
    activeCeTarget?.type === "grand" && activeCeTarget.index === 1;
  const disabledIds: number[] | undefined =
    activeSlot && activeSlot.type !== "support"
      ? slots
        .filter(
          (s) =>
            s.type !== "support" &&
            s.id !== activeSlotId &&
            s.servant != null
        )
        .map((s) => s.servant!.id)
      : undefined;

  const leftSlots = displaySlots.slice(0, 3);
  const rightSlots = displaySlots.slice(3, 6);
  const slotIds = displaySlots.map((s) => s.id);
  const supportSkillLevels = normalizeSupportSkillLevels(
    activeProject?.supportSkillLevelMins,
  );
  const supportAppendSkillLevels = normalizeSupportAppendSkillLevels(
    activeProject?.supportAppendSkillLevelMins,
  );

  const handleSupportSettingsConfirm = (next: {
    npLevel: number | null;
    skillLevels: SupportSkillLevelMins;
    appendSkillLevels: SupportAppendSkillLevelMins;
  }) => {
    if (!activeProject) return;
    void onUpdateActiveProject({
      ...activeProject,
      supportNoblePhantasmLevelMin: next.npLevel,
      supportSkillLevelMins: next.skillLevels,
      supportAppendSkillLevelMins: next.appendSkillLevels,
    });
  };

  const renderSlot = (slot: SlotItem) => (
    <SortableSlot
      key={slot.id}
      slot={slot}
      portraitSrc={slot.servant ? portraitMap[slot.servant.variantKey] : null}
      ceCardSrc={slot.craftEssence ? ceCardMap[slot.craftEssence.id] : null}
      supportGrandMode={activeProject?.supportGrandMode ?? false}
      supportGrandCraftEssences={supportGrandCraftEssences}
      supportGrandCeCardSrcs={supportGrandCeCardSrcs}
      supportGrandCeMlbRequired={supportGrandCeMlbRequired}
      supportGrandBondCeMode={supportGrandBondCeMode}
      mlbIconSrc={mlbIconSrc}
      bondIconSrc={grandBondIconSrc}
      bondNpIconSrc={grandBondNpIconSrc}
      supportNpLevel={activeProject?.supportNoblePhantasmLevelMin ?? null}
      supportSkillLevels={supportSkillLevels}
      supportAppendSkillLevels={supportAppendSkillLevels}
      onSelect={() => handleSlotClick(slot)}
      onCeSelect={() => handleCeSlotClick(slot.id)}
      onCeClear={() => handleCeClear(slot.id)}
      onGrandModeToggle={handleGrandModeToggle}
      onGrandCeSelect={handleGrandCeSlotClick}
      onGrandCeClear={handleGrandCeClear}
      onSupportSettingsOpen={() => setSupportSettingsOpen(true)}
    />
  );

  return (
    <>
      <DndContext
        sensors={sensors}
        collisionDetection={closestCenter}
        onDragEnd={handleDragEnd}
      >
        <SortableContext items={slotIds} strategy={rectSortingStrategy}>
          <Box className="content-unified-grid">
            {leftSlots.map(renderSlot)}
            {rightSlots.map(renderSlot)}
          </Box>
        </SortableContext>
      </DndContext>

      <ServantSelectDialog
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        onSelect={handleSelect}
        servants={servants}
        disabledIds={disabledIds}
        defaultClassFilter={defaultServantClassFilter}
      />

      <CraftEssenceSelectDialog
        open={ceDialogOpen}
        onOpenChange={(open) => {
          setCeDialogOpen(open);
          if (!open) setActiveCeTarget(null);
        }}
        onSelect={handleCeSelect}
        craftEssences={craftEssences}
        mlbRequired={activeCeTargetMlbRequired}
        onMlbRequiredChange={showCeMlbOption ? handleCeMlbRequiredChange : undefined}
        grandBondCeMode={supportGrandBondCeMode}
        onGrandBondCeModeChange={
          showGrandBondOption ? handleGrandBondCeModeChange : undefined
        }
      />

      {supportSettingsOpen && (
        <SupportSettingsDialog
          open={supportSettingsOpen}
          project={activeProject}
          onOpenChange={setSupportSettingsOpen}
          onConfirm={handleSupportSettingsConfirm}
        />
      )}
    </>
  );
}
