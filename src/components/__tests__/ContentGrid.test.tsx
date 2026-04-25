import { describe, it, expect, vi } from "vitest";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
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

const MASH: Servant = {
  id: 1,
  name_cn: "玛修",
  name_jp: "マシュ・キリエライト",
  name_en: "Mash Kyrielight",
  class: "Shielder",
  rarity: 4,
};

const ALTRIA_CASTER: Servant = {
  id: 284,
  name_cn: "阿尔托莉雅·卡斯特",
  name_jp: "アルトリア・キャスター",
  name_en: "Altria Caster",
  class: "Caster",
  rarity: 5,
};

const SERVANTS: Servant[] = [MASH, ALTRIA_CASTER];

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
  it("renders six CE picker plates, all showing the empty placeholder", () => {
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

    // Six empty CE plates, one per slot. The empty plate carries
    // `aria-label="选择礼装"` on its outer button (the visible glyph
    // is just a `+` icon, no text).
    const placeholders = screen.getAllByLabelText("选择礼装");
    expect(placeholders).toHaveLength(6);

    // The 5 party servant slots show "选择从者"; the support slot shows
    // its dedicated "助战" affordance instead, so we expect 5 picks of
    // the party label.
    const servantPlaceholders = screen.getAllByText("选择从者");
    expect(servantPlaceholders).toHaveLength(5);
  });

  it("opens the CE picker dialog when an empty CE plate is clicked", async () => {
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

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    // Click the first empty CE plate (located via its aria-label).
    const placeholder = screen.getAllByLabelText("选择礼装")[0];
    await user.click(placeholder);

    const dialog = await screen.findByRole("dialog");
    expect(dialog).toBeInTheDocument();
    expect(within(dialog).getByText("选择礼装")).toBeInTheDocument();
  });

  it("falls back to the CE name in the scrim when the card art is missing", () => {
    // The setup mock returns null for `get_craft_essence_card_path`,
    // so a pinned CE renders the fallback scrim instead of an
    // `<img>` — the scrim shows the CE name as text.
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

    expect(screen.getByText("Kaleidoscope")).toBeInTheDocument();
    // Filled plate is labelled with the CE name (not "选择礼装").
    expect(screen.getByLabelText("礼装：Kaleidoscope")).toBeInTheDocument();
    // Five remaining CE slots still show the empty placeholder.
    expect(screen.getAllByLabelText("选择礼装")).toHaveLength(5);
    // Filled plates expose a clear button (Cross2Icon
    // `aria-label="清除礼装"`).
    expect(screen.getByLabelText("清除礼装")).toBeInTheDocument();
  });

  it("renders the CE card <img> when the resolver returns a path", async () => {
    // Override the default "no card on disk" stub so CE #1 resolves
    // to a real on-disk path. `convertFileSrc` (mocked to return
    // `asset://...`) wraps it for use as `<img src>`.
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "get_craft_essence_card_path") {
        const { craftEssenceId } = (args ?? {}) as { craftEssenceId?: number };
        return craftEssenceId === 1
          ? "/abs/src-tauri/assets/ces/1/card_ce.png"
          : null;
      }
      return null;
    });

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

    const img = await screen.findByAltText("Kaleidoscope");
    expect(img.tagName).toBe("IMG");
    expect(img.getAttribute("src")).toBe(
      "asset:///abs/src-tauri/assets/ces/1/card_ce.png"
    );
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

  // --- Portrait rendering -------------------------------------------

  it("renders an <img> with the resolved portrait when the resolver returns a path", async () => {
    // Override the default "no portrait" stub for `get_servant_portrait_path`
    // so the Mash slot resolves to a real on-disk path. `convertFileSrc`
    // (mocked to return `asset://...`) wraps it for use as `<img src>`.
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "get_servant_portrait_path") {
        const { servantId } = (args ?? {}) as { servantId?: number };
        return servantId === 1
          ? "/abs/src-tauri/assets/servants/1/narrow_servant_4.png"
          : null;
      }
      return null;
    });

    const slots = buildSlots();
    slots[0] = { ...slots[0], servant: MASH };

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

    // The portrait <img> uses the servant's Chinese name as alt text.
    const img = await screen.findByAltText("玛修");
    expect(img).toBeInTheDocument();
    expect(img.tagName).toBe("IMG");
    expect(img.getAttribute("src")).toBe(
      "asset:///abs/src-tauri/assets/servants/1/narrow_servant_4.png"
    );
  });

  it("renders a placeholder card with the servant name when the resolver returns null", async () => {
    // Default mock from setup.ts already returns null for
    // `get_servant_portrait_path`, so a servant without an on-disk
    // portrait should fall back to the placeholder card.
    const slots = buildSlots();
    slots[0] = { ...slots[0], servant: ALTRIA_CASTER };

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

    // No <img> for this servant, but the placeholder shows the name.
    expect(screen.queryByAltText("阿尔托莉雅·卡斯特")).not.toBeInTheDocument();
    expect(screen.getByText("阿尔托莉雅·卡斯特")).toBeInTheDocument();
  });

  // --- Support badge -------------------------------------------------

  it("shows a SUPPORT corner badge on an empty support slot", () => {
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

    // The corner badge is text-only, fixed copy "SUPPORT".
    const badges = screen.getAllByText("SUPPORT");
    expect(badges).toHaveLength(1);
  });

  it("keeps the SUPPORT badge when the support slot has a pinned servant", () => {
    const projectWithSupport: Project = {
      ...PROJECT,
      supportServantId: MASH.id,
    };

    renderWithTheme(
      <ContentGrid
        servants={SERVANTS}
        craftEssences={CES}
        slots={buildSlots()}
        onSlotsChange={vi.fn()}
        activeProject={projectWithSupport}
        onUpdateActiveProject={vi.fn()}
      />
    );

    expect(screen.getAllByText("SUPPORT")).toHaveLength(1);
  });

  // --- Rarity frame --------------------------------------------------

  it("tags the portrait frame class by servant rarity", () => {
    // 1-2 ★ → brass, 3 ★ → silver, 4-5 ★ → gold. Slot 0 holds Mash
    // (4 ★ → gold), slot 1 holds Altria Caster (5 ★ → also gold);
    // the support row is empty so it stays default-framed.
    const slots = buildSlots();
    slots[0] = { ...slots[0], servant: MASH };
    slots[1] = { ...slots[1], servant: ALTRIA_CASTER };

    const { container } = renderWithTheme(
      <ContentGrid
        servants={SERVANTS}
        craftEssences={CES}
        slots={slots}
        onSlotsChange={vi.fn()}
        activeProject={PROJECT}
        onUpdateActiveProject={vi.fn()}
      />
    );

    const portraits = container.querySelectorAll(".servant-portrait");
    expect(portraits).toHaveLength(6);
    // Filled slots get a rarity class (gold for both fixtures).
    expect(portraits[0].className).toMatch(/\brarity-gold\b/);
    expect(portraits[1].className).toMatch(/\brarity-gold\b/);
    // Empty slots get no rarity class — only the default frame applies.
    for (let i = 2; i < portraits.length; i++) {
      expect(portraits[i].className).not.toMatch(/\brarity-/);
    }
  });
});
