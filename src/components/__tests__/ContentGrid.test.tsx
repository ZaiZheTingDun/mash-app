import { describe, it, expect, vi } from "vitest";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithTheme } from "../../test/renderWithTheme";
import {
  ContentGrid,
  createInitialProjectSlots,
  type SlotItem,
} from "../ContentGrid";
import type { Servant } from "../../types/servant";
import type { CraftEssence } from "../../types/craftEssence";
import type { Project } from "../../types/project";

function buildSlots(): SlotItem[] {
  return createInitialProjectSlots().map((s) => ({
    id: s.id,
    type: s.type,
    servant: null,
    craftEssence: null,
  }));
}

const SERVANTS: Servant[] = [
  {
    id: 284,
    name_cn: "阿尔托莉雅·卡斯特",
    name_jp: "アルトリア・キャスター",
    name_en: "Altria Caster",
    class: "Caster",
    rarity: 5,
  },
];

const CES: CraftEssence[] = [
  { id: 1, name: "Kaleidoscope" },
  { id: 2, name: "Black Grail" },
];

const PROJECT: Project = {
  id: "p1",
  name: "Test Project",
  supportServantId: null,
  slots: createInitialProjectSlots(),
  repeatMission: false,
};

describe("createInitialProjectSlots", () => {
  it("returns six slots with the support pinned at index 2", () => {
    const slots = createInitialProjectSlots();
    expect(slots).toHaveLength(6);
    expect(slots.map((s) => s.id)).toEqual([
      "slot-0",
      "slot-1",
      "slot-2",
      "slot-3",
      "slot-4",
      "slot-5",
    ]);
    expect(slots[2].type).toBe("support");
    for (const s of slots) {
      expect(s.servantId).toBeNull();
      expect(s.craftEssenceId).toBeNull();
    }
  });
});

describe("ContentGrid", () => {
  it("renders six CE picker tiles, all showing the empty placeholder", () => {
    renderWithTheme(
      <ContentGrid
        servants={SERVANTS}
        craftEssences={CES}
        slots={buildSlots()}
        onSlotsChange={vi.fn()}
        activeProject={PROJECT}
        onUpdateActiveProject={vi.fn()}
      />
    );

    // Six "选择礼装" placeholders, one per CE slot under each servant.
    const placeholders = screen.getAllByText("选择礼装");
    expect(placeholders).toHaveLength(6);

    // The 5 party servant slots show "选择从者"; the support slot shows
    // its dedicated "助战" affordance instead, so we expect 5 picks of
    // the party label.
    const servantPlaceholders = screen.getAllByText("选择从者");
    expect(servantPlaceholders).toHaveLength(5);
  });

  it("opens the CE picker dialog when a CE tile is clicked", async () => {
    const user = userEvent.setup();
    renderWithTheme(
      <ContentGrid
        servants={SERVANTS}
        craftEssences={CES}
        slots={buildSlots()}
        onSlotsChange={vi.fn()}
        activeProject={PROJECT}
        onUpdateActiveProject={vi.fn()}
      />
    );

    // Initially no dialog is open.
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    // Click one of the CE placeholder tiles. We grab the first
    // "选择礼装" placeholder and click its enclosing card.
    const placeholder = screen.getAllByText("选择礼装")[0];
    await user.click(placeholder);

    const dialog = await screen.findByRole("dialog");
    expect(dialog).toBeInTheDocument();
    expect(within(dialog).getByText("选择礼装")).toBeInTheDocument();
  });

  it("renders the pinned CE name in a slot that has one", () => {
    const slots = buildSlots();
    slots[0] = { ...slots[0], craftEssence: CES[0] };
    renderWithTheme(
      <ContentGrid
        servants={SERVANTS}
        craftEssences={CES}
        slots={slots}
        onSlotsChange={vi.fn()}
        activeProject={PROJECT}
        onUpdateActiveProject={vi.fn()}
      />
    );

    // Pinned CE: name + #id pair is visible.
    expect(screen.getByText("Kaleidoscope")).toBeInTheDocument();
    expect(screen.getByText("#1")).toBeInTheDocument();
    // Five remaining CE slots still show the placeholder.
    expect(screen.getAllByText("选择礼装")).toHaveLength(5);
    // And the slot exposes a clear button (the `Cross2Icon` carries
    // `aria-label="清除礼装"`).
    expect(screen.getByLabelText("清除礼装")).toBeInTheDocument();
  });

  it("clearing a pinned CE calls onSlotsChange with the slot's CE nulled", async () => {
    const user = userEvent.setup();
    const onSlotsChange = vi.fn();
    const slots = buildSlots();
    slots[0] = { ...slots[0], craftEssence: CES[0] };

    renderWithTheme(
      <ContentGrid
        servants={SERVANTS}
        craftEssences={CES}
        slots={slots}
        onSlotsChange={onSlotsChange}
        activeProject={PROJECT}
        onUpdateActiveProject={vi.fn()}
      />
    );

    await user.click(screen.getByLabelText("清除礼装"));

    expect(onSlotsChange).toHaveBeenCalledTimes(1);
    const next = onSlotsChange.mock.calls[0][0] as SlotItem[];
    expect(next[0].craftEssence).toBeNull();
    // Other slots untouched.
    for (let i = 1; i < next.length; i++) {
      expect(next[i].craftEssence).toBeNull();
    }
  });
});
