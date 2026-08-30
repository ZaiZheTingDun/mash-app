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
import { CraftEssenceManageDialog } from "./CraftEssenceManageDialog";
import { useCeCards, usePortraits } from "./contentGridAssets";
import { SortableSlot } from "./ContentGridSlot";
import { SupportSettingsDialog } from "./SupportSettingsDialog";
import { PortraitSelectDialog } from "./PortraitSelectDialog";
import { invoke } from "../../tauri";
import {
  grandClassToServantClass,
  normalizeSupportAppendSkillLevels,
  normalizeSupportGrandCraftEssenceIdLists,
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
  SupportGrandCraftEssenceIdLists,
  SupportGrandCraftEssenceIds,
  SupportGrandCraftEssenceMlbRequired,
  SupportAppendSkillLevelMins,
  SupportSkillLevelMins,
  GrandClassDefinition,
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
  grandClassDefinitions?: GrandClassDefinition[];
  onUpdateActiveProject: (next: Project) => Promise<void> | void;
}

export function ContentGrid({
  servants,
  craftEssences,
  slots,
  onSlotsChange,
  activeProject,
  grandClassDefinitions = [],
  onUpdateActiveProject,
}: ContentGridProps) {
  const [dialogOpen, setDialogOpen] = useState(false);
  const [activeSlotId, setActiveSlotId] = useState<string | null>(null);
  const [ceDialogOpen, setCeDialogOpen] = useState(false);
  const [activeCeTarget, setActiveCeTarget] = useState<
    { type: "slot"; slotId: string } | { type: "grand"; index: number } | null
  >(null);
  const [cePickerMode, setCePickerMode] = useState<"single" | "add" | "replace">(
    "single"
  );
  const [ceReplaceId, setCeReplaceId] = useState<number | null>(null);
  const [ceManageTarget, setCeManageTarget] = useState<
    { type: "slot"; slotId: string } | { type: "grand"; index: number } | null
  >(null);
  const [ceManageOpen, setCeManageOpen] = useState(false);
  const [supportSettingsOpen, setSupportSettingsOpen] = useState(false);

  const [deleteTargetSlotId, setDeleteTargetSlotId] = useState<string | null>(null);
  const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false);
  const [clearAllCeTarget, setClearAllCeTarget] = useState<
    { type: "slot"; slotId: string } | { type: "grand"; index: number } | null
  >(null);
  const [clearAllCeConfirmOpen, setClearAllCeConfirmOpen] = useState(false);
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
      ...displaySlots.flatMap((s) =>
        s.craftEssences?.length
          ? s.craftEssences.map((ce) => ce.id)
          : s.craftEssence
            ? [s.craftEssence.id]
            : []
      ),
      ...(activeProject?.supportGrandCraftEssenceIds ?? []),
      ...(activeProject?.supportGrandCraftEssenceIdLists ?? []).flat(),
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

  const openCePicker = (
    target: NonNullable<typeof activeCeTarget>,
    mode: "single" | "add" | "replace",
    replaceId: number | null = null
  ) => {
    setActiveCeTarget(target);
    setCePickerMode(mode);
    setCeReplaceId(replaceId);
    setCeDialogOpen(true);
  };

  const supportGrandCeIds = normalizeSupportGrandCraftEssenceIds(
    activeProject?.supportGrandCraftEssenceIds,
  );
  const supportGrandCeIdLists = normalizeSupportGrandCraftEssenceIdLists(
    activeProject?.supportGrandCraftEssenceIdLists,
    supportGrandCeIds,
  );
  const supportGrandCraftEssenceGroups = supportGrandCeIdLists.map((ids) =>
    ids
      .map((id) => craftEssences.find((ce) => ce.id === id) ?? null)
      .filter((ce): ce is CraftEssence => ce != null)
  );
  const supportGrandCeCardSrcGroups = supportGrandCraftEssenceGroups.map((group) =>
    group.map((ce) => ceCardMap[ce.id])
  );
  const supportGrandCeMlbRequired = normalizeSupportGrandCraftEssenceMlbRequired(
    activeProject?.supportGrandCraftEssenceMlbRequired,
  );
  const supportGrandBondCeMode = activeProject?.supportGrandBondCeMode ?? "any";
  const defaultServantClassFilter =
    activeProject?.advancedMode === true
      ? grandClassToServantClass(activeProject.grandClass, grandClassDefinitions)
      : undefined;

  const handleGrandModeToggle = () => {
    if (!activeProject) return;
    void onUpdateActiveProject({
      ...activeProject,
      supportGrandMode: !(activeProject.supportGrandMode ?? false),
      supportGrandCraftEssenceIds: supportGrandCeIds,
      supportGrandCraftEssenceIdLists: supportGrandCeIdLists,
      supportGrandCraftEssenceMlbRequired: supportGrandCeMlbRequired,
      supportGrandBondCeMode,
    });
  };

  const slotCraftEssences = (slotId: string) => {
    const slot = slots.find((item) => item.id === slotId);
    return slot?.craftEssences?.length
      ? slot.craftEssences
      : slot?.craftEssence
        ? [slot.craftEssence]
        : [];
  };

  const updateSlotCraftEssences = (slotId: string, selected: CraftEssence[]) => {
    const unique = Array.from(
      new Map(selected.map((ce) => [ce.id, ce])).values()
    ).slice(0, 10);
    onSlotsChange(
      slots.map((slot) =>
        slot.id === slotId
          ? {
              ...slot,
              craftEssence: unique[0] ?? null,
              craftEssences: unique,
              craftEssenceMultiSelect: slot.type === "support" && unique.length > 1,
              craftEssenceMlbRequired: slot.craftEssenceMlbRequired ?? true,
            }
          : slot
      )
    );
  };

  const updateGrandCraftEssences = (index: number, selected: CraftEssence[]) => {
    if (!activeProject) return;
    const limit = index === 1 ? 1 : 10;
    const ids = Array.from(new Set(selected.map((ce) => ce.id))).slice(0, limit);
    const nextLists = supportGrandCeIdLists.map((list) => [...list]) as
      SupportGrandCraftEssenceIdLists;
    const nextIds = [...supportGrandCeIds] as SupportGrandCraftEssenceIds;
    nextLists[index] = ids;
    nextIds[index] = ids[0] ?? null;
    void onUpdateActiveProject({
      ...activeProject,
      supportGrandCraftEssenceIds: nextIds,
      supportGrandCraftEssenceIdLists: nextLists,
      supportGrandCraftEssenceMlbRequired: supportGrandCeMlbRequired,
      supportGrandBondCeMode:
        index === 1 && ids.length === 0 ? "any" : supportGrandBondCeMode,
    });
  };

  const selectedForTarget = (
    target: NonNullable<typeof activeCeTarget> | null
  ): CraftEssence[] => {
    if (!target) return [];
    return target.type === "slot"
      ? slotCraftEssences(target.slotId)
      : supportGrandCraftEssenceGroups[target.index];
  };

  const updateTargetCraftEssences = (
    target: NonNullable<typeof activeCeTarget>,
    selected: CraftEssence[]
  ) => {
    if (target.type === "slot") updateSlotCraftEssences(target.slotId, selected);
    else updateGrandCraftEssences(target.index, selected);
  };

  const handleCeSelect = (ce: CraftEssence) => {
    if (!activeCeTarget) return;
    const current = selectedForTarget(activeCeTarget);
    const next =
      cePickerMode === "add"
        ? [...current, ce]
        : cePickerMode === "replace" && ceReplaceId != null
          ? current.map((item) => (item.id === ceReplaceId ? ce : item))
          : [ce];
    updateTargetCraftEssences(activeCeTarget, next);
  };

  const openCeManager = (target: NonNullable<typeof ceManageTarget>) => {
    setCeManageTarget(target);
    setCeManageOpen(true);
  };

  const handleCeClear = (target: NonNullable<typeof clearAllCeTarget>) => {
    const selected = selectedForTarget(target);
    if (selected.length > 1) {
      setClearAllCeTarget(target);
      setClearAllCeConfirmOpen(true);
    } else {
      updateTargetCraftEssences(target, []);
    }
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
      supportGrandCraftEssenceIdLists: supportGrandCeIdLists,
      supportGrandCraftEssenceMlbRequired: nextMlb,
      supportGrandBondCeMode,
    });
  };

  const handleGrandBondCeModeChange = (mode: SupportGrandBondCeMode) => {
    if (!activeProject) return;
    void onUpdateActiveProject({
      ...activeProject,
      supportGrandCraftEssenceIds: supportGrandCeIds,
      supportGrandCraftEssenceIdLists: supportGrandCeIdLists,
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
  const cePickerDisabledIds = selectedForTarget(activeCeTarget)
    .map((ce) => ce.id)
    .filter((id) => id !== ceReplaceId);
  const managedCraftEssences = selectedForTarget(ceManageTarget);

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
    servantLevel: number | null;
    starMapScore: number | null;
    grandStarMapScore: number | null;
    npLevel: number | null;
    skillLevels: SupportSkillLevelMins;
    appendSkillLevels: SupportAppendSkillLevelMins;
  }) => {
    if (!activeProject) return;
    void onUpdateActiveProject({
      ...activeProject,
      supportServantLevelMin: next.servantLevel,
      supportStarMapScoreMin: next.starMapScore,
      supportGrandStarMapScoreMin: next.grandStarMapScore,
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
      craftEssences={
        slot.craftEssences?.length
          ? slot.craftEssences
          : slot.craftEssence
            ? [slot.craftEssence]
            : []
      }
      ceCardSrcs={(slot.craftEssences?.length
        ? slot.craftEssences
        : slot.craftEssence
          ? [slot.craftEssence]
          : []
      ).map((ce) => ceCardMap[ce.id])}
      supportGrandMode={activeProject?.supportGrandMode ?? false}
      supportGrandCraftEssenceGroups={supportGrandCraftEssenceGroups}
      supportGrandCeCardSrcGroups={supportGrandCeCardSrcGroups}
      supportGrandCeMlbRequired={supportGrandCeMlbRequired}
      supportGrandBondCeMode={supportGrandBondCeMode}
      mlbIconSrc={mlbIconSrc}
      bondIconSrc={grandBondIconSrc}
      bondNpIconSrc={grandBondNpIconSrc}
      supportServantLevel={activeProject?.supportServantLevelMin ?? null}
      supportNpLevel={activeProject?.supportNoblePhantasmLevelMin ?? null}
      supportStarMapScore={activeProject?.supportStarMapScoreMin ?? null}
      supportGrandStarMapScore={activeProject?.supportGrandStarMapScoreMin ?? null}
      supportSkillLevels={supportSkillLevels}
      supportAppendSkillLevels={supportAppendSkillLevels}
      onSelect={() => handleSlotClick(slot)}
      onCeSelect={() => openCePicker({ type: "slot", slotId: slot.id }, "single")}
      onCeAdd={() => openCePicker({ type: "slot", slotId: slot.id }, "add")}
      onCeManage={() => openCeManager({ type: "slot", slotId: slot.id })}
      onCeClear={() => handleCeClear({ type: "slot", slotId: slot.id })}
      onGrandModeToggle={handleGrandModeToggle}
      onGrandCeSelect={(index) =>
        openCePicker({ type: "grand", index }, "single")
      }
      onGrandCeAdd={(index) =>
        openCePicker({ type: "grand", index }, "add")
      }
      onGrandCeManage={(index) => openCeManager({ type: "grand", index })}
      onGrandCeClear={(index) => handleCeClear({ type: "grand", index })}
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
        portraitRefreshKey={portraitRefreshKey}
        onPortraitSaved={() => setPortraitRefreshKey((key) => key + 1)}
      />

      <CraftEssenceSelectDialog
        open={ceDialogOpen}
        onOpenChange={(open) => {
          setCeDialogOpen(open);
          if (!open) {
            setActiveCeTarget(null);
            setCePickerMode("single");
            setCeReplaceId(null);
          }
        }}
        onSelect={handleCeSelect}
        craftEssences={craftEssences}
        mlbRequired={activeCeTargetMlbRequired}
        onMlbRequiredChange={showCeMlbOption ? handleCeMlbRequiredChange : undefined}
        grandBondCeMode={supportGrandBondCeMode}
        onGrandBondCeModeChange={
          showGrandBondOption ? handleGrandBondCeModeChange : undefined
        }
        disabledIds={cePickerDisabledIds}
      />

      <CraftEssenceManageDialog
        open={ceManageOpen}
        onOpenChange={(open) => {
          setCeManageOpen(open);
          if (!open) setCeManageTarget(null);
        }}
        craftEssences={managedCraftEssences}
        onAdd={() => {
          if (ceManageTarget) openCePicker(ceManageTarget, "add");
        }}
        onRemove={(ce) => {
          if (!ceManageTarget) return;
          updateTargetCraftEssences(
            ceManageTarget,
            managedCraftEssences.filter((item) => item.id !== ce.id)
          );
        }}
        onReplace={(ce) => {
          if (ceManageTarget) openCePicker(ceManageTarget, "replace", ce.id);
        }}
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
            删除从者会同步清楚所有指令设置，请自行检查指令设置是否需要重新配置。删除后无法撤销，请谨慎操作。
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
                确认
              </Button>
            </AlertDialog.Action>
          </Flex>
        </AlertDialog.Content>
      </AlertDialog.Root>

      <AlertDialog.Root
        open={clearAllCeConfirmOpen}
        onOpenChange={(open) => {
          setClearAllCeConfirmOpen(open);
          if (!open) setClearAllCeTarget(null);
        }}
      >
        <AlertDialog.Content maxWidth="420px">
          <AlertDialog.Title>确认清空礼装</AlertDialog.Title>
          <AlertDialog.Description size="2">
            将删除这个槽位中已选择的全部礼装，是否继续？
          </AlertDialog.Description>
          <Flex gap="3" mt="4" justify="end">
            <AlertDialog.Cancel>
              <Button variant="soft" color="gray">取消</Button>
            </AlertDialog.Cancel>
            <AlertDialog.Action>
              <Button
                color="red"
                onClick={() => {
                  if (clearAllCeTarget) {
                    updateTargetCraftEssences(clearAllCeTarget, []);
                  }
                  setClearAllCeConfirmOpen(false);
                  setClearAllCeTarget(null);
                }}
              >
                删除全部
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
