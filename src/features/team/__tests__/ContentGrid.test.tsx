import { describe, it, expect, vi } from "vitest";
import { fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { ContentGrid, type SlotItem } from "../ContentGrid";
import { createInitialProjectSlots } from "../projectSlots";
import type { Servant } from "../../../types/servant";
import type { CraftEssence } from "../../../types/craftEssence";
import type { Project } from "../../../types/project";

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
  variantKey: "1",
  name_cn: "玛修",
  name_jp: "マシュ・キリエライト",
  name_en: "Mash Kyrielight",
  class: "Shielder",
  rarity: 4,
};

const MASH_VARIANT: Servant = {
  ...MASH,
  variantKey: "1:1",
  faceId: 800170,
  noblePhantasmName: "已然遥远的理想之城",
};

const ALTRIA_CASTER: Servant = {
  id: 284,
  variantKey: "284",
  name_cn: "阿尔托莉雅·卡斯特",
  name_jp: "アルトリア・キャスター",
  name_en: "Altria Caster",
  class: "Caster",
  rarity: 5,
};

const ALTRIA_SABER: Servant = {
  id: 2,
  variantKey: "2",
  name_cn: "阿尔托莉雅",
  name_jp: "アルトリア",
  name_en: "Altria",
  class: "Saber",
  rarity: 5,
};

const HERACLES: Servant = {
  id: 3,
  variantKey: "3",
  name_cn: "赫拉克勒斯",
  name_jp: "ヘラクレス",
  name_en: "Heracles",
  class: "Berserker",
  rarity: 4,
};

const SERVANTS: Servant[] = [MASH, ALTRIA_CASTER];
const GRAND_SERVANTS: Servant[] = [ALTRIA_CASTER, ALTRIA_SABER, HERACLES];

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
      expect(s.servantVariantKey).toBeNull();
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
        const { servantId, faceId } = (args ?? {}) as {
          servantId?: number;
          faceId?: number | null;
        };
        expect(faceId).toBeNull();
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

  it("requests variant portraits by the variant asset id", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "get_servant_portrait_path") {
        const { servantId, faceId } = (args ?? {}) as {
          servantId?: number;
          faceId?: number | null;
        };
        return servantId === 1 && faceId === 800170
          ? "/abs/src-tauri/assets/servants/1/narrow_servant_800170.png"
          : null;
      }
      return null;
    });

    const slots = buildSlots();
    slots[0] = { ...slots[0], servant: MASH_VARIANT };

    renderWithTheme(
      <ContentGrid
        servants={[MASH_VARIANT, ALTRIA_CASTER]}
        craftEssences={CES}
        slots={slots}
        onSlotsChange={vi.fn()}
        activeProject={PROJECT}
        onUpdateActiveProject={vi.fn()}
      />
    );

    const img = await screen.findByAltText("玛修");
    expect(img.getAttribute("src")).toBe(
      "asset:///abs/src-tauri/assets/servants/1/narrow_servant_800170.png"
    );
    expect(invoke).toHaveBeenCalledWith("get_servant_portrait_path", {
      servantId: 1,
      faceId: 800170,
    });
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

  it("defaults servant selection filter to Saber in default grand battle projects", async () => {
    const user = userEvent.setup();
    renderWithTheme(
      <ContentGrid
        servants={GRAND_SERVANTS}
        craftEssences={CES}
        slots={buildSlots()}
        onSlotsChange={vi.fn()}
        activeProject={{ ...PROJECT, advancedMode: true }}
        onUpdateActiveProject={vi.fn()}
      />
    );

    await user.click(screen.getAllByText("选择从者")[0]);

    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByLabelText("职介筛选")).toHaveTextContent("Saber");
    expect(within(dialog).getByText("阿尔托莉雅")).toBeInTheDocument();
    expect(within(dialog).queryByText("赫拉克勒斯")).not.toBeInTheDocument();
    expect(within(dialog).queryByText("阿尔托莉雅·卡斯特")).not.toBeInTheDocument();

    await user.click(within(dialog).getByRole("combobox", { name: "职介筛选" }));
    await user.click(await screen.findByRole("option", { name: "全部职介" }));

    expect(within(dialog).getByText("阿尔托莉雅")).toBeInTheDocument();
    expect(within(dialog).getByText("赫拉克勒斯")).toBeInTheDocument();
    expect(within(dialog).getByText("阿尔托莉雅·卡斯特")).toBeInTheDocument();
  });

  it("defaults support selection filter to Berserker in berserker grand battle projects", async () => {
    const user = userEvent.setup();
    renderWithTheme(
      <ContentGrid
        servants={GRAND_SERVANTS}
        craftEssences={CES}
        slots={buildSlots()}
        onSlotsChange={vi.fn()}
        activeProject={{ ...PROJECT, advancedMode: true, grandClass: "berserker" }}
        onUpdateActiveProject={vi.fn()}
      />
    );

    await user.click(screen.getByText("助战"));

    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByLabelText("职介筛选")).toHaveTextContent("Berserker");
    expect(within(dialog).getByText("赫拉克勒斯")).toBeInTheDocument();
    expect(within(dialog).queryByText("阿尔托莉雅")).not.toBeInTheDocument();
    expect(within(dialog).queryByText("阿尔托莉雅·卡斯特")).not.toBeInTheDocument();

    await user.click(within(dialog).getByRole("combobox", { name: "职介筛选" }));
    await user.click(await screen.findByRole("option", { name: "全部职介" }));

    expect(within(dialog).getByText("阿尔托莉雅")).toBeInTheDocument();
    expect(within(dialog).getByText("赫拉克勒斯")).toBeInTheDocument();
    expect(within(dialog).getByText("阿尔托莉雅·卡斯特")).toBeInTheDocument();
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

  it("opens support skill settings and persists confirmed requirements", async () => {
    const user = userEvent.setup();
    const onUpdateActiveProject = vi.fn();

    renderWithTheme(
      <ContentGrid
        servants={SERVANTS}
        craftEssences={CES}
        slots={buildSlots()}
        onSlotsChange={vi.fn()}
        activeProject={PROJECT}
        onUpdateActiveProject={onUpdateActiveProject}
      />
    );

    await user.click(screen.getByRole("button", { name: "技能/宝具设置" }));
    expect(await screen.findByRole("dialog")).toHaveTextContent("技能/宝具设置");

    await user.click(screen.getByRole("button", { name: "宝具等级" }));
    await user.click(await screen.findByRole("radio", { name: "2" }));
    await user.click(screen.getByRole("button", { name: "确认" }));
    await user.click(screen.getByRole("button", { name: "持有技能 1" }));
    await user.click(await screen.findByRole("radio", { name: "10" }));
    await user.click(screen.getByRole("button", { name: "确认" }));
    await user.click(screen.getByRole("button", { name: "确认" }));

    expect(onUpdateActiveProject).toHaveBeenCalledWith(
      expect.objectContaining({
        supportNoblePhantasmLevelMin: 2,
        supportSkillLevelMins: [10, null, null],
        supportAppendSkillLevelMins: [null, null, null, null, null],
      })
    );
  });

  it("renders configured support requirements and opens settings from the summary", async () => {
    const user = userEvent.setup();
    renderWithTheme(
      <ContentGrid
        servants={SERVANTS}
        craftEssences={CES}
        slots={buildSlots()}
        onSlotsChange={vi.fn()}
        activeProject={{
          ...PROJECT,
          supportNoblePhantasmLevelMin: 2,
          supportSkillLevelMins: [10, null, 9],
          supportAppendSkillLevelMins: [null, 10, null, null, null],
        }}
        onUpdateActiveProject={vi.fn()}
      />
    );

    expect(screen.getByText("宝具 2")).toBeInTheDocument();
    expect(screen.getByLabelText("持有技能 1 至少 10 级")).toBeInTheDocument();
    expect(screen.getByLabelText("追加技能 2 至少 10 级")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "技能/宝具设置" })
    ).not.toBeInTheDocument();

    await user.click(screen.getByLabelText("编辑技能宝具设置"));
    expect(await screen.findByRole("dialog")).toHaveTextContent("技能/宝具设置");
  });

  it("toggles grand support mode from the support slot", async () => {
    const user = userEvent.setup();
    const onUpdateActiveProject = vi.fn();
    renderWithTheme(
      <ContentGrid
        servants={SERVANTS}
        craftEssences={CES}
        slots={buildSlots()}
        onSlotsChange={vi.fn()}
        activeProject={PROJECT}
        onUpdateActiveProject={onUpdateActiveProject}
      />
    );

    await user.click(screen.getByRole("button", { name: "开启冠位模式" }));

    expect(onUpdateActiveProject).toHaveBeenCalledWith(
      expect.objectContaining({
        supportGrandMode: true,
        supportGrandCraftEssenceIds: [null, null, null],
      })
    );
  });

  it("renders and persists three grand support craft essence slots", async () => {
    const user = userEvent.setup();
    const onUpdateActiveProject = vi.fn();
    renderWithTheme(
      <ContentGrid
        servants={SERVANTS}
        craftEssences={CES}
        slots={buildSlots()}
        onSlotsChange={vi.fn()}
        activeProject={{
          ...PROJECT,
          supportGrandMode: true,
          supportGrandCraftEssenceIds: [1, null, 2],
        }}
        onUpdateActiveProject={onUpdateActiveProject}
      />
    );

    expect(screen.getByLabelText("冠位礼装 1：Kaleidoscope")).toBeInTheDocument();
    expect(screen.getByLabelText("选择冠位礼装 2")).toBeInTheDocument();
    expect(screen.getByLabelText("冠位礼装 3：Black Grail")).toBeInTheDocument();

    await user.click(screen.getByLabelText("选择冠位礼装 2"));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByText("Black Grail"));
    expect(onUpdateActiveProject).toHaveBeenCalledWith(
      expect.objectContaining({
        supportGrandCraftEssenceIds: [1, 2, 2],
      })
    );

    await user.click(screen.getByRole("button", { name: "清除冠位礼装 1" }));
    expect(onUpdateActiveProject).toHaveBeenCalledWith(
      expect.objectContaining({
        supportGrandCraftEssenceIds: [null, null, 2],
      })
    );
  });

  it("stacks grand support craft essences and falls back when card art fails", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd, args) => {
      if (cmd === "get_craft_essence_card_path") {
        const { craftEssenceId } = (args ?? {}) as { craftEssenceId?: number };
        return craftEssenceId ? `/abs/ces/${craftEssenceId}/card_ce.png` : null;
      }
      return null;
    });

    const { container } = renderWithTheme(
      <ContentGrid
        servants={SERVANTS}
        craftEssences={CES}
        slots={buildSlots()}
        onSlotsChange={vi.fn()}
        activeProject={{
          ...PROJECT,
          supportGrandMode: true,
          supportGrandCraftEssenceIds: [1, null, 2],
        }}
        onUpdateActiveProject={vi.fn()}
      />
    );

    const supportPortrait = container.querySelector(
      ".servant-portrait.support.grand-support",
    );
    expect(supportPortrait).toBeInTheDocument();
    expect(
      supportPortrait?.querySelectorAll(".grand-ce-overlay .grand-ce-slot"),
    ).toHaveLength(3);

    const image = await screen.findByAltText("Kaleidoscope");
    fireEvent.error(image);

    expect(screen.getByText("Kaleidoscope")).toBeInTheDocument();
  });

  it("renders placeholder slots for unconfigured skills so configured chips keep their position", () => {
    // Only the 3rd owned skill and 2nd append skill are configured;
    // the rendered chip rows must still show all 3 owned + 5 append
    // slots (with `-` placeholders) so the user can tell which slot
    // each value belongs to.
    renderWithTheme(
      <ContentGrid
        servants={SERVANTS}
        craftEssences={CES}
        slots={buildSlots()}
        onSlotsChange={vi.fn()}
        activeProject={{
          ...PROJECT,
          supportSkillLevelMins: [null, null, 5],
          supportAppendSkillLevelMins: [null, 7, null, null, null],
        }}
        onUpdateActiveProject={vi.fn()}
      />
    );

    expect(screen.getByLabelText("持有技能 1 任意等级")).toBeInTheDocument();
    expect(screen.getByLabelText("持有技能 2 任意等级")).toBeInTheDocument();
    expect(screen.getByLabelText("持有技能 3 至少 5 级")).toBeInTheDocument();
    // Row 1 is rendered (owned skills are set), so the NP slot in
    // cols 4-5 also emits a placeholder even though NP itself isn't
    // configured — keeps the grid stable.
    expect(screen.getByLabelText("宝具任意等级")).toBeInTheDocument();

    expect(screen.getByLabelText("追加技能 1 任意等级")).toBeInTheDocument();
    expect(screen.getByLabelText("追加技能 2 至少 7 级")).toBeInTheDocument();
    expect(screen.getByLabelText("追加技能 3 任意等级")).toBeInTheDocument();
    expect(screen.getByLabelText("追加技能 4 任意等级")).toBeInTheDocument();
    expect(screen.getByLabelText("追加技能 5 任意等级")).toBeInTheDocument();
  });

  it("renders owned-skill placeholders alongside NP when only NP is configured", () => {
    // With only NP set, row 1 still renders all 3 owned-skill slots as
    // dashes so the NP chip stays anchored at cols 4-5 instead of
    // sliding to the left of the grid.
    renderWithTheme(
      <ContentGrid
        servants={SERVANTS}
        craftEssences={CES}
        slots={buildSlots()}
        onSlotsChange={vi.fn()}
        activeProject={{
          ...PROJECT,
          supportNoblePhantasmLevelMin: 5,
        }}
        onUpdateActiveProject={vi.fn()}
      />
    );

    expect(screen.getByLabelText("宝具至少 5 级")).toBeInTheDocument();
    expect(screen.getByLabelText("持有技能 1 任意等级")).toBeInTheDocument();
    expect(screen.getByLabelText("持有技能 2 任意等级")).toBeInTheDocument();
    expect(screen.getByLabelText("持有技能 3 任意等级")).toBeInTheDocument();
    expect(
      screen.queryByLabelText("追加技能 1 任意等级"),
    ).not.toBeInTheDocument();
  });

  it("hides the entire append row when no append skill is configured", () => {
    // NP / owned skills set, append untouched: the append row should
    // not render any placeholder chips since there's nothing real to
    // align against in that row.
    renderWithTheme(
      <ContentGrid
        servants={SERVANTS}
        craftEssences={CES}
        slots={buildSlots()}
        onSlotsChange={vi.fn()}
        activeProject={{
          ...PROJECT,
          supportNoblePhantasmLevelMin: 4,
          supportSkillLevelMins: [10, null, null],
        }}
        onUpdateActiveProject={vi.fn()}
      />
    );

    expect(screen.getByLabelText("持有技能 1 至少 10 级")).toBeInTheDocument();
    expect(
      screen.queryByLabelText("追加技能 1 任意等级"),
    ).not.toBeInTheDocument();
  });

  it("styles skill level picker options by requirement threshold", async () => {
    const user = userEvent.setup();
    renderWithTheme(
      <ContentGrid
        servants={SERVANTS}
        craftEssences={CES}
        slots={buildSlots()}
        onSlotsChange={vi.fn()}
        activeProject={{
          ...PROJECT,
          supportSkillLevelMins: [5, null, null],
        }}
        onUpdateActiveProject={vi.fn()}
      />
    );

    await user.click(screen.getByLabelText("编辑技能宝具设置"));
    await user.click(screen.getByRole("button", { name: "持有技能 1" }));

    const picker = await screen.findByRole("radiogroup", { name: "技能等级选择" });
    const pickerScope = within(picker);
    expect(pickerScope.getByRole("radio", { name: "4" })).toHaveClass("hint");
    expect(pickerScope.getByRole("radio", { name: "5" })).toHaveClass("current");
    expect(pickerScope.getByRole("radio", { name: "6" })).toHaveClass("meets");
  });

  it("uses threshold picker styling for noble phantasm levels", async () => {
    const user = userEvent.setup();
    renderWithTheme(
      <ContentGrid
        servants={SERVANTS}
        craftEssences={CES}
        slots={buildSlots()}
        onSlotsChange={vi.fn()}
        activeProject={{
          ...PROJECT,
          supportNoblePhantasmLevelMin: 3,
        }}
        onUpdateActiveProject={vi.fn()}
      />
    );

    await user.click(screen.getByLabelText("编辑技能宝具设置"));
    await user.click(screen.getByRole("button", { name: "宝具等级" }));
    const npPicker = await screen.findByRole("radiogroup", {
      name: "宝具等级选择",
    });
    const pickerScope = within(npPicker);
    expect(pickerScope.queryByRole("radio", { name: "6" })).not.toBeInTheDocument();
    expect(pickerScope.getByRole("radio", { name: "2" })).toHaveClass("hint");
    expect(pickerScope.getByRole("radio", { name: "3" })).toHaveClass("current");
    expect(pickerScope.getByRole("radio", { name: "4" })).toHaveClass("meets");
  });

  it("does not apply level picker changes when cancelled", async () => {
    const user = userEvent.setup();
    renderWithTheme(
      <ContentGrid
        servants={SERVANTS}
        craftEssences={CES}
        slots={buildSlots()}
        onSlotsChange={vi.fn()}
        activeProject={{
          ...PROJECT,
          supportNoblePhantasmLevelMin: 3,
          supportSkillLevelMins: [5, null, null],
        }}
        onUpdateActiveProject={vi.fn()}
      />
    );

    await user.click(screen.getByLabelText("编辑技能宝具设置"));
    const npButton = screen.getByRole("button", { name: "宝具等级" });
    await user.click(npButton);
    await user.click(await screen.findByRole("radio", { name: "4" }));
    await user.click(screen.getByRole("button", { name: "取消等级选择" }));
    expect(npButton).toHaveTextContent("3");

    const skillButton = screen.getByRole("button", { name: "持有技能 1" });
    await user.click(skillButton);
    await user.click(await screen.findByRole("radio", { name: "7" }));
    await user.click(screen.getByRole("button", { name: "取消等级选择" }));
    expect(skillButton).toHaveTextContent("5");
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
