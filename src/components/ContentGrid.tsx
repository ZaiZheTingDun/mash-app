import { useState } from "react";
import { Box, Flex, Text } from "@radix-ui/themes";
import { ImageIcon, PlusIcon, PersonIcon } from "@radix-ui/react-icons";
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
import type { Servant } from "../types/servant";

interface ContentGridProps {
  servants: Servant[];
}

interface SlotItem {
  id: string;
  type: "servant" | "support";
  servant: Servant | null;
}

function createInitialSlots(): SlotItem[] {
  return [
    { id: "slot-0", type: "servant", servant: null },
    { id: "slot-1", type: "servant", servant: null },
    { id: "slot-2", type: "support", servant: null },
    { id: "slot-3", type: "servant", servant: null },
    { id: "slot-4", type: "servant", servant: null },
    { id: "slot-5", type: "servant", servant: null },
  ];
}

function ImageCard() {
  return (
    <Box className="image-card">
      <Flex align="center" justify="center" className="image-card-inner">
        <ImageIcon width={36} height={36} className="image-card-icon" />
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

  if (slot.type === "support") {
    return (
      <div
        ref={setNodeRef}
        style={style}
        className="slot-drag-wrapper"
        {...attributes}
        {...listeners}
      >
        <Box className="image-card servant-slot support">
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
      </div>
    );
  }

  const { servant } = slot;

  return (
    <div
      ref={setNodeRef}
      style={style}
      className="slot-drag-wrapper"
      {...attributes}
      {...listeners}
    >
      {servant ? (
        <Box className="image-card servant-slot filled" onClick={onSelect}>
          <Flex
            direction="column"
            align="center"
            justify="center"
            gap="2"
            className="image-card-inner"
          >
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

export function ContentGrid({ servants }: ContentGridProps) {
  const [slots, setSlots] = useState<SlotItem[]>(createInitialSlots);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [activeSlotId, setActiveSlotId] = useState<string | null>(null);

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 5 } })
  );

  const handleDragEnd = (event: DragEndEvent) => {
    const { active, over } = event;
    if (!over || active.id === over.id) return;

    setSlots((prev) => {
      const oldIndex = prev.findIndex((s) => s.id === active.id);
      const newIndex = prev.findIndex((s) => s.id === over.id);
      return arrayMove(prev, oldIndex, newIndex);
    });
  };

  const handleSlotClick = (slot: SlotItem) => {
    if (slot.type === "support") return;
    setActiveSlotId(slot.id);
    setDialogOpen(true);
  };

  const handleSelect = (servant: Servant) => {
    setSlots((prev) =>
      prev.map((s) =>
        s.id === activeSlotId ? { ...s, servant } : s
      )
    );
  };

  const leftSlots = slots.slice(0, 3);
  const rightSlots = slots.slice(3, 6);
  const slotIds = slots.map((s) => s.id);

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
            {Array.from({ length: 3 }).map((_, i) => (
              <ImageCard key={`left-bottom-${i}`} />
            ))}
            {Array.from({ length: 3 }).map((_, i) => (
              <ImageCard key={`right-bottom-${i}`} />
            ))}
          </Box>
        </SortableContext>
      </DndContext>

      <ServantSelectDialog
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        onSelect={handleSelect}
        servants={servants}
      />
    </>
  );
}
