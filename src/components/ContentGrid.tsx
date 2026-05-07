import { useEffect, useState } from "react";
import { Box, Flex, Text } from "@radix-ui/themes";
import { PlusIcon, PersonIcon, Cross2Icon } from "@radix-ui/react-icons";
import { invoke, convertFileSrc } from "../tauri";
import type React from "react";
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
import type { Project } from "../types/project";

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

interface CraftEssenceOverlayProps {
  craftEssence: CraftEssence | null;
  cardSrc: string | null | undefined;
  onSelect: () => void;
  onClear: () => void;
}

/**
 * Plate pinned to the bottom of the servant portrait, sized to the
 * natural CE card aspect (`--ce-card-aspect` in CSS).
 *
 * Empty state:  translucent gray scrim + four red L-shaped corner
 *               brackets (the FGO "this is where the CE goes"
 *               affordance).
 * Filled state: just the CE card artwork (or a fallback scrim with
 *               the CE name when the asset isn't on disk) plus a
 *               hover-revealed `×` clear button. The brackets are
 *               intentionally hidden once a CE is equipped so they
 *               don't compete visually with the artwork.
 *
 * Click target spans the full plate; `stopPropagation` keeps clicks
 * from bubbling up to the portrait's own click handler (which opens
 * the servant picker instead).
 */
function CraftEssenceOverlay({
  craftEssence,
  cardSrc,
  onSelect,
  onClear,
}: CraftEssenceOverlayProps) {
  const handleClick = (e: React.MouseEvent) => {
    e.stopPropagation();
    onSelect();
  };

  return (
    <div
      className={`ce-overlay${craftEssence ? " filled" : " empty"}`}
      onClick={handleClick}
      role="button"
      aria-label={craftEssence ? `礼装：${craftEssence.name}` : "选择礼装"}
    >
      {craftEssence && cardSrc ? (
        <img
          className="ce-overlay-img"
          src={cardSrc}
          alt={craftEssence.name}
          draggable={false}
        />
      ) : (
        <div className="ce-overlay-scrim">
          {craftEssence ? (
            <Text
              size="1"
              weight="bold"
              align="center"
              className="ce-overlay-fallback-label"
              truncate
            >
              {craftEssence.name}
            </Text>
          ) : (
            <PlusIcon width={20} height={20} className="ce-overlay-empty-icon" />
          )}
        </div>
      )}
      {!craftEssence && (
        <>
          <span className="ce-overlay-corner tl" aria-hidden />
          <span className="ce-overlay-corner tr" aria-hidden />
          <span className="ce-overlay-corner bl" aria-hidden />
          <span className="ce-overlay-corner br" aria-hidden />
        </>
      )}
      {craftEssence && (
        <button
          type="button"
          className="ce-overlay-clear"
          aria-label="清除礼装"
          onClick={(e) => {
            e.stopPropagation();
            onClear();
          }}
        >
          <Cross2Icon width={11} height={11} />
        </button>
      )}
    </div>
  );
}

/**
 * Generic asset-path resolver hook. Walks a Rust command that takes a
 * single integer id and returns either an absolute file path or
 * `null`, then wraps the path with `convertFileSrc` so the result is
 * ready to drop into `<img src>`.
 *
 * Returns a map keyed by id; values are either a `convertFileSrc` URL,
 * `null` when the resolver returned `None`, or `undefined` while the
 * fetch is still in flight. Refires only when the id set actually
 * changes (slot reorders that don't add/remove ids are no-ops).
 *
 * The CE card resolver uses this because it only needs one numeric id.
 * Servant portraits are variant-aware and use a dedicated hook below.
 */
function useAssetPaths(
  command: string,
  argKey: string,
  ids: number[],
): Record<number, string | null | undefined> {
  const [cache, setCache] = useState<Record<number, string | null>>({});

  const key = ids
    .filter((id, i, arr) => arr.indexOf(id) === i)
    .sort((a, b) => a - b)
    .join(",");

  useEffect(() => {
    const parsedIds = key
      ? key.split(",").map((s) => Number(s)).filter((n) => Number.isFinite(n))
      : [];
    const missing = parsedIds.filter((id) => !(id in cache));
    if (missing.length === 0) return;
    let cancelled = false;
    Promise.all(
      missing.map((id) =>
        invoke<string | null>(command, { [argKey]: id })
          .then((path) => [id, path ? convertFileSrc(path) : null] as const)
          .catch(() => [id, null] as const)
      )
    ).then((results) => {
      if (cancelled) return;
      setCache((prev) => {
        const next = { ...prev };
        for (const [id, src] of results) {
          next[id] = src;
        }
        return next;
      });
    });
    return () => {
      cancelled = true;
    };
    // `cache` intentionally excluded — re-running on cache writes would
    // create an infinite loop. The effect re-fires only when the id set
    // changes, which is exactly when we need to fetch new ones.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, command, argKey]);

  return cache;
}

/**
 * Resolve full-art portrait paths for servants that don't have a cached
 * entry yet. Variants share the same base servant id, so the cache is
 * keyed by `variantKey` and the backend receives the variant asset id
 * (`faceId`) when one exists.
 */
function usePortraits(servants: Servant[]): Record<string, string | null | undefined> {
  const [cache, setCache] = useState<Record<string, string | null>>({});
  const byVariant = new Map<string, Servant>();
  for (const servant of servants) {
    byVariant.set(servant.variantKey, servant);
  }
  const requests = Array.from(byVariant.values())
    .map((servant) => ({
      variantKey: servant.variantKey,
      servantId: servant.id,
      faceId: servant.faceId ?? null,
    }))
    .sort((a, b) => a.variantKey.localeCompare(b.variantKey));
  const key = JSON.stringify(requests);

  useEffect(() => {
    const parsed = JSON.parse(key) as typeof requests;
    const missing = parsed.filter(({ variantKey }) => !(variantKey in cache));
    if (missing.length === 0) return;
    let cancelled = false;
    Promise.all(
      missing.map((request) =>
        invoke<string | null>("get_servant_portrait_path", {
          servantId: request.servantId,
          faceId: request.faceId,
        })
          .then(
            (path) =>
              [
                request.variantKey,
                path ? convertFileSrc(path) : null,
              ] as const
          )
          .catch(() => [request.variantKey, null] as const)
      )
    ).then((results) => {
      if (cancelled) return;
      setCache((prev) => {
        const next = { ...prev };
        for (const [variantKey, src] of results) {
          next[variantKey] = src;
        }
        return next;
      });
    });
    return () => {
      cancelled = true;
    };
    // `cache` intentionally excluded — re-running on cache writes would
    // create an infinite loop. The effect re-fires only when the servant
    // variant set changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  return cache;
}

/**
 * Resolve craft-essence card art paths. The resolver lives in Rust
 * (`get_craft_essence_card_path`) and looks up
 * `assets/ces/{id}/card_ce.png`. Cards are checked into the repo today
 * for the full Atlas Academy CE list, so this almost always resolves
 * to a real file — the `null` branch in `CraftEssenceOverlay` exists
 * only as a defensive fallback for missing assets.
 */
function useCeCards(ceIds: number[]): Record<number, string | null | undefined> {
  return useAssetPaths("get_craft_essence_card_path", "craftEssenceId", ceIds);
}

interface SortableSlotProps {
  slot: SlotItem;
  portraitSrc: string | null | undefined;
  ceCardSrc: string | null | undefined;
  onSelect: () => void;
  onCeSelect: () => void;
  onCeClear: () => void;
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
 *          tile that used to sit below this card).
 *   3. `.slot-footer` — empty reserved band (future: name/HP/etc).
 *
 * Empty / empty-support states keep the same skeleton so the card
 * height matches a filled card; only the middle portrait region swaps
 * for a `+ 选择从者` / `助战` placeholder.
 */
function SortableSlot({
  slot,
  portraitSrc,
  ceCardSrc,
  onSelect,
  onCeSelect,
  onCeClear,
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
          className={`servant-portrait${servant ? " filled" : " empty"}${isSupport ? " support" : ""}${rarityClass ? ` ${rarityClass}` : ""}`}
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
          <CraftEssenceOverlay
            craftEssence={slot.craftEssence}
            cardSrc={ceCardSrc}
            onSelect={onCeSelect}
            onClear={onCeClear}
          />
        </div>
        <div className="slot-footer" />
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
    displaySlots
      .map((s) => s.craftEssence?.id)
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

  const renderSlot = (slot: SlotItem) => (
    <SortableSlot
      key={slot.id}
      slot={slot}
      portraitSrc={slot.servant ? portraitMap[slot.servant.variantKey] : null}
      ceCardSrc={slot.craftEssence ? ceCardMap[slot.craftEssence.id] : null}
      onSelect={() => handleSlotClick(slot)}
      onCeSelect={() => handleCeSlotClick(slot.id)}
      onCeClear={() => handleCeClear(slot.id)}
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
