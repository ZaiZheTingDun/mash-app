import { useState } from "react";
import { AlertDialog, Box, Button, Flex } from "@radix-ui/themes";
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
  arrayMove,
  rectSortingStrategy,
} from "@dnd-kit/sortable";
import { ServantSelectDialog } from "./ServantSelectDialog";
import { CraftEssenceSelectDialog } from "./CraftEssenceSelectDialog";
import { useCeCards, usePortraits } from "./contentGridAssets";
import { SortableSlot } from "./ContentGridSlot";
import { SupportSettingsDialog } from "./SupportSettingsDialog";
import { PortraitSelectDialog } from "./PortraitSelectDialog";
import { invoke } from "../../tauri";
import {
  grandClassToServantClass,
  normalizeSupportAppendSkillLevels,
  normalizeSupportGrandCraftEssenceIds,
  normalizeSupportGrandCraftEssenceMlbRequired,
  normalizeSupportSkillLevels,
} from "./supportSettingsModel";
import type { SlotItem } from "./contentGridTypes";
import type { Servant } from "../../types/servant";
import type { CraftEssence } from "../../types/craftEssence";
import type {
  Project,
  SupportGrandBondCeMode,
  SupportGrandCraftEssenceIds,
  SupportGrandCraftEssenceMlbRequired,
  SupportAppendSkillLevelMins,
  SupportSkillLevelMins,
} from "../../types/project";
import mlbIconSrc from "../../../src-tauri/resources/images/icon_mlb_mark.png";
import grandBondIconSrc from "../../../src-tauri/resources/images/icon_grand_bond_ce.png";
import grandBondNpIconSrc from "../../../src-tauri/resources/images/icon_grand_bond_ce_np.png";

export type { SlotItem } from "./contentGridTypes";

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

  const [deleteTargetSlotId, setDeleteTargetSlotId] = useState<string | null>(null);
  const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false);
  const [portraitSlotId, setPortraitSlotId] = useState<string | null>(null);
  const [portraitRefreshKey, setPortraitRefreshKey] = useState(0);

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
      .filter((servant): servant is Servant => servant != null),
    portraitRefreshKey,
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

  const handleDeleteRequest = (slot: SlotItem) => {
    if (!slot.servant && slot.type !== "support") return;
    if (slot.type === "support" && !activeProject?.supportServantId) return;
    setDeleteTargetSlotId(slot.id);
    setDeleteConfirmOpen(true);
  };

  const handleDeleteConfirm = (slotId: string) => {
    if (!activeProject) return Promise.resolve();
    return invoke<{ project: Project }>("delete_slot_servant", {
      projectId: activeProject.id,
      slotId,
    })
      .then(({ project }) => {
        void onUpdateActiveProject(project);
      })
      .catch(() => {});
  };

  const handlePortraitSettingsOpen = (slot: SlotItem) => {
    if (!slot.servant) return;
    setPortraitSlotId(slot.id);
  };

  const portraitSlot = portraitSlotId
    ? displaySlots.find((s) => s.id === portraitSlotId) ?? null
    : null;

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
      onDeleteRequest={() => handleDeleteRequest(slot)}
      onPortraitSettingsOpen={
        slot.servant ? () => handlePortraitSettingsOpen(slot) : undefined
      }
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

      <AlertDialog.Root
        open={deleteConfirmOpen}
        onOpenChange={(open) => {
          setDeleteConfirmOpen(open);
          if (!open) setDeleteTargetSlotId(null);
        }}
      >
        <AlertDialog.Content maxWidth="420px">
          <AlertDialog.Title>确认删除从者</AlertDialog.Title>
          <AlertDialog.Description size="2">
            从者及槽位设置会被清除，相关指令可能失效。
          </AlertDialog.Description>
          <Flex gap="3" mt="4" justify="end">
            <AlertDialog.Cancel>
              <Button variant="soft" color="gray">
                取消
              </Button>
            </AlertDialog.Cancel>
            <AlertDialog.Action>
              <Button
                color="red"
                onClick={() => {
                  if (deleteTargetSlotId) {
                    void handleDeleteConfirm(deleteTargetSlotId);
                  }
                  setDeleteConfirmOpen(false);
                  setDeleteTargetSlotId(null);
                }}
              >
                确认删除
              </Button>
            </AlertDialog.Action>
          </Flex>
        </AlertDialog.Content>
      </AlertDialog.Root>

      {portraitSlot?.servant && (
        <PortraitSelectDialog
          open={portraitSlotId != null}
          onOpenChange={(open) => {
            if (!open) setPortraitSlotId(null);
          }}
          servantId={portraitSlot.servant.id}
          variantKey={portraitSlot.servant.variantKey}
          servantName={portraitSlot.servant.name_cn}
          onSaved={() => setPortraitRefreshKey((k) => k + 1)}
        />
      )}
    </>
  );
}
