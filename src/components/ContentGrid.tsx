import { useState } from "react";
import { Box, Flex, Text } from "@radix-ui/themes";
import { PlusIcon, PersonIcon, Cross2Icon } from "@radix-ui/react-icons";
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
import type { Servant } from "../types/servant";
import type { CraftEssence } from "../types/craftEssence";
import type { Project, ProjectSlot } from "../types/project";

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
}

/**
 * Default 6-slot layout used when no project is active. Matches the Rust
 * `default_project_slots()` so the empty-project view in the frontend
 * renders the same arrangement a freshly-created project would have.
 */
export function createInitialProjectSlots(): ProjectSlot[] {
  return [
    { id: "slot-0", type: "servant", servantId: null, craftEssenceId: null },
    { id: "slot-1", type: "servant", servantId: null, craftEssenceId: null },
    { id: "slot-2", type: "support", servantId: null, craftEssenceId: null },
    { id: "slot-3", type: "servant", servantId: null, craftEssenceId: null },
    { id: "slot-4", type: "servant", servantId: null, craftEssenceId: null },
    { id: "slot-5", type: "servant", servantId: null, craftEssenceId: null },
  ];
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

interface CraftEssenceSlotProps {
  craftEssence: CraftEssence | null;
  onSelect: () => void;
  onClear: () => void;
}

/**
 * One picker tile rendered directly below a servant slot. Clicking the
 * tile opens the CE picker dialog; when a CE is pinned the tile shows
 * the CE name and a small clear button so the user can unpin without
 * having to re-open the dialog.
 */
function CraftEssenceSlot({ craftEssence, onSelect, onClear }: CraftEssenceSlotProps) {
  if (!craftEssence) {
    return (
      <Box className="image-card servant-slot empty" onClick={onSelect}>
        <Flex
          direction="column"
          align="center"
          justify="center"
          gap="1"
          className="image-card-inner"
        >
          <PlusIcon width={24} height={24} className="servant-slot-icon" />
          <Text size="1" color="gray">
            选择礼装
          </Text>
        </Flex>
      </Box>
    );
  }

  return (
    <Box className="image-card servant-slot filled" onClick={onSelect}>
      <Flex
        direction="column"
        align="center"
        justify="center"
        gap="1"
        className="image-card-inner"
        style={{ position: "relative", padding: "4px" }}
      >
        <button
          type="button"
          className="ce-clear-btn"
          aria-label="清除礼装"
          onClick={(e) => {
            e.stopPropagation();
            onClear();
          }}
          style={{
            position: "absolute",
            top: 2,
            right: 2,
            background: "transparent",
            border: "none",
            color: "var(--gray-9)",
            cursor: "pointer",
            padding: 2,
            display: "flex",
            alignItems: "center",
          }}
        >
          <Cross2Icon width={12} height={12} />
        </button>
        <Text size="2" weight="bold" align="center">
          {craftEssence.name}
        </Text>
        <Text size="1" color="gray">
          #{craftEssence.id}
        </Text>
      </Flex>
    </Box>
  );
}

interface SortableSlotProps {
  slot: SlotItem;
  onSelect: () => void;
}

function SortableSlot({ slot, onSelect }: SortableSlotProps) {
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

  return (
    <div
      ref={setNodeRef}
      style={style}
      className="slot-drag-wrapper"
      {...attributes}
      {...listeners}
    >
      {servant ? (
        <Box
          className={`image-card servant-slot filled${isSupport ? " support-filled" : ""}`}
          onClick={onSelect}
        >
          <Flex
            direction="column"
            align="center"
            justify="center"
            gap="2"
            className="image-card-inner"
          >
            {isSupport && (
              <Text size="1" weight="medium" className="support-slot-badge">
                助战
              </Text>
            )}
            <Text size="2" weight="bold" align="center">
              {servant.name_cn}
            </Text>
            <Text size="1" style={{ color: "#d4a537", letterSpacing: "1px" }}>
              {"★".repeat(servant.rarity)}
            </Text>
            <Text size="1" color="gray">
              {servant.class}
            </Text>
          </Flex>
        </Box>
      ) : isSupport ? (
        <Box className="image-card servant-slot support" onClick={onSelect}>
          <Flex
            direction="column"
            align="center"
            justify="center"
            gap="1"
            className="image-card-inner"
          >
            <PersonIcon width={28} height={28} className="support-slot-icon" />
            <Text size="1" weight="medium" className="support-slot-label">
              助战
            </Text>
          </Flex>
        </Box>
      ) : (
        <Box className="image-card servant-slot empty" onClick={onSelect}>
          <Flex
            direction="column"
            align="center"
            justify="center"
            gap="1"
            className="image-card-inner"
          >
            <PlusIcon width={28} height={28} className="servant-slot-icon" />
            <Text size="1" color="gray">
              选择从者
            </Text>
          </Flex>
        </Box>
      )}
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
  const [activeCeSlotId, setActiveCeSlotId] = useState<string | null>(null);

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
    return servants.find((s) => s.id === id) ?? null;
  })();

  const displaySlots: SlotItem[] = slots.map((s) =>
    s.type === "support" ? { ...s, servant: supportPinned } : s
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
    setActiveCeSlotId(slotId);
    setCeDialogOpen(true);
  };

  const handleCeSelect = (ce: CraftEssence) => {
    if (!activeCeSlotId) return;
    onSlotsChange(
      slots.map((s) =>
        s.id === activeCeSlotId ? { ...s, craftEssence: ce } : s
      )
    );
  };

  const handleCeClear = (slotId: string) => {
    onSlotsChange(
      slots.map((s) => (s.id === slotId ? { ...s, craftEssence: null } : s))
    );
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

  return (
    <>
      <DndContext
        sensors={sensors}
        collisionDetection={closestCenter}
        onDragEnd={handleDragEnd}
      >
        <SortableContext items={slotIds} strategy={rectSortingStrategy}>
          <Box className="content-unified-grid">
            {leftSlots.map((slot) => (
              <SortableSlot
                key={slot.id}
                slot={slot}
                onSelect={() => handleSlotClick(slot)}
              />
            ))}
            {rightSlots.map((slot) => (
              <SortableSlot
                key={slot.id}
                slot={slot}
                onSelect={() => handleSlotClick(slot)}
              />
            ))}
            {leftSlots.map((slot) => (
              <CraftEssenceSlot
                key={`ce-${slot.id}`}
                craftEssence={slot.craftEssence}
                onSelect={() => handleCeSlotClick(slot.id)}
                onClear={() => handleCeClear(slot.id)}
              />
            ))}
            {rightSlots.map((slot) => (
              <CraftEssenceSlot
                key={`ce-${slot.id}`}
                craftEssence={slot.craftEssence}
                onSelect={() => handleCeSlotClick(slot.id)}
                onClear={() => handleCeClear(slot.id)}
              />
            ))}
          </Box>
        </SortableContext>
      </DndContext>

      <ServantSelectDialog
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        onSelect={handleSelect}
        servants={servants}
        disabledIds={disabledIds}
      />

      <CraftEssenceSelectDialog
        open={ceDialogOpen}
        onOpenChange={setCeDialogOpen}
        onSelect={handleCeSelect}
        craftEssences={craftEssences}
      />
    </>
  );
}
