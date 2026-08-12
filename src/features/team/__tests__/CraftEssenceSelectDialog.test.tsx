import { describe, it, expect, vi } from "vitest";
import { screen, fireEvent, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { CraftEssenceSelectDialog } from "../CraftEssenceSelectDialog";
import type { CraftEssence } from "../../../types/craftEssence";

const FIXTURE: CraftEssence[] = [
  { id: 1, rarity: 5, category: "normal", name: "Kaleidoscope" },
  { id: 2, rarity: 5, category: "normal", name: "Black Grail" },
  { id: 3, rarity: 4, category: "event", name: "The Imaginary Element" },
  { id: 4, rarity: 3, category: "bond", name: "Heaven's Feel" },
];

function setup(overrides?: {
  open?: boolean;
  craftEssences?: CraftEssence[];
  onSelect?: (ce: CraftEssence) => void;
  onOpenChange?: (open: boolean) => void;
}) {
  const onSelect = overrides?.onSelect ?? vi.fn();
  const onOpenChange = overrides?.onOpenChange ?? vi.fn();
  const utils = renderWithTheme(
    <CraftEssenceSelectDialog
      open={overrides?.open ?? true}
      onOpenChange={onOpenChange}
      onSelect={onSelect}
      craftEssences={overrides?.craftEssences ?? FIXTURE}
    />
  );
  return { ...utils, onSelect, onOpenChange };
}

describe("CraftEssenceSelectDialog", () => {
  it("renders the dialog with the title and search field when open", () => {
    setup();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.getByText("选择礼装")).toBeInTheDocument();
    expect(
      screen.getByPlaceholderText("搜索礼装名称...")
    ).toBeInTheDocument();
  });

  it("filters CEs by case-insensitive substring match", async () => {
    const user = userEvent.setup();
    setup();

    // Initial render: every fixture row is visible (small list, no
    // virtualization elision).
    expect(screen.getByText("Kaleidoscope")).toBeInTheDocument();
    expect(screen.getByText("Black Grail")).toBeInTheDocument();

    const search = screen.getByPlaceholderText("搜索礼装名称...");
    await user.type(search, "kal");

    expect(screen.getByText("Kaleidoscope")).toBeInTheDocument();
    expect(screen.queryByText("Black Grail")).not.toBeInTheDocument();
    expect(
      screen.queryByText("The Imaginary Element")
    ).not.toBeInTheDocument();
  });

  it("shows the resolved CE card art at the right of its row", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "get_craft_essence_card_path") {
        const { craftEssenceId } = (args ?? {}) as { craftEssenceId?: number };
        return craftEssenceId === 1 ? "/tmp/ces/1/card_ce.png" : null;
      }
      return null;
    });
    setup();

    await waitFor(() => {
      const thumbnail = document.querySelector(".ce-card-thumbnail");
      expect(thumbnail).toHaveAttribute("src", "asset:///tmp/ces/1/card_ce.png");
    });
  });

  it("shows a full-size preview when hovering a CE card thumbnail", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "get_craft_essence_card_path") {
        const { craftEssenceId } = (args ?? {}) as { craftEssenceId?: number };
        return craftEssenceId === 1 ? "/tmp/ces/1/card_ce.png" : null;
      }
      return null;
    });
    setup();

    const thumbnail = await waitFor(() => {
      const image = document.querySelector(".ce-card-thumbnail");
      expect(image).toBeInstanceOf(HTMLImageElement);
      return image as HTMLImageElement;
    });
    await user.hover(thumbnail);

    expect(await screen.findByAltText("Kaleidoscope 卡面预览")).toHaveAttribute(
      "src",
      "asset:///tmp/ces/1/card_ce.png"
    );
  });

  it("filters by the supported category and rarity", async () => {
    const user = userEvent.setup();
    setup({
      craftEssences: [
        ...FIXTURE,
        {
          id: 5,
          rarity: 4,
          category: "manaExchange",
          name: "Mana Prism CE",
        },
        {
          id: 6,
          rarity: 4,
          category: "eventReward",
          name: "Event Reward CE",
        },
        { id: 7, rarity: 4, category: "other", name: "Chocolate CE" },
      ],
    });

    const categoryGroup = screen.getByRole("group", { name: "礼装类型筛选" });
    const rarityGroup = screen.getByRole("group", { name: "礼装星级筛选" });
    expect(within(categoryGroup).getAllByRole("button")).toHaveLength(6);
    expect(within(rarityGroup).getAllByRole("button")).toHaveLength(6);
    expect(
      within(categoryGroup).queryByRole("button", { name: "巧克力礼装" })
    ).not.toBeInTheDocument();

    await user.click(
      within(categoryGroup).getByRole("button", {
        name: "魔力棱镜/进阶关卡礼装",
      })
    );
    await user.click(within(rarityGroup).getByRole("button", { name: "★4" }));

    expect(screen.getByText("Mana Prism CE")).toBeInTheDocument();
    expect(screen.queryByText("Event Reward CE")).not.toBeInTheDocument();
    expect(screen.queryByText("Chocolate CE")).not.toBeInTheDocument();
  });

  it("matches translation aliases while displaying the fixed CE name", async () => {
    const user = userEvent.setup();
    setup({
      craftEssences: [
        {
          id: 2234,
          rarity: 5,
          category: "normal",
          name: "心愿之味",
          nameAliases: ["心意的滋味"],
        },
        {
          id: 2237,
          rarity: 5,
          category: "normal",
          name: "去往大海",
          nameAliases: ["向着大海"],
        },
      ],
    });

    const search = screen.getByPlaceholderText("搜索礼装名称...");
    await user.type(search, "心意的滋味");

    expect(screen.getByText("心愿之味")).toBeInTheDocument();
    expect(screen.queryByText("心意的滋味")).not.toBeInTheDocument();
    expect(screen.queryByText("去往大海")).not.toBeInTheDocument();
  });

  it("shows the empty-state message when no CE matches the query", async () => {
    const user = userEvent.setup();
    setup();
    const search = screen.getByPlaceholderText("搜索礼装名称...");
    await user.type(search, "zzzzzz-no-such-ce");

    expect(
      screen.getByText("未找到匹配的礼装")
    ).toBeInTheDocument();
  });

  it("selects a CE on click and closes the dialog", async () => {
    const user = userEvent.setup();
    const { onSelect, onOpenChange } = setup();

    await user.click(screen.getByText("Black Grail"));

    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onSelect).toHaveBeenCalledWith(
      expect.objectContaining({ id: 2, name: "Black Grail" })
    );
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("selects via keyboard: ArrowDown + Enter picks the second row", async () => {
    const user = userEvent.setup();
    const { onSelect } = setup();

    const search = screen.getByPlaceholderText("搜索礼装名称...");
    await user.click(search);
    await user.keyboard("{ArrowDown}{Enter}");

    expect(onSelect).toHaveBeenCalledTimes(1);
    // Initial activeIndex is 0 (Kaleidoscope); ArrowDown moves to index
    // 1 (Black Grail), Enter commits.
    expect(onSelect).toHaveBeenCalledWith(
      expect.objectContaining({ id: 2, name: "Black Grail" })
    );
  });

  it("ArrowUp at the top of the list stays on the first row", async () => {
    const user = userEvent.setup();
    const { onSelect } = setup();

    const search = screen.getByPlaceholderText("搜索礼装名称...");
    await user.click(search);
    await user.keyboard("{ArrowUp}{Enter}");

    expect(onSelect).toHaveBeenCalledWith(
      expect.objectContaining({ id: 1, name: "Kaleidoscope" })
    );
  });

  it("Enter is a no-op when the filtered list is empty", async () => {
    const user = userEvent.setup();
    const { onSelect } = setup();

    const search = screen.getByPlaceholderText("搜索礼装名称...");
    await user.type(search, "zzz-nothing-matches");
    await user.keyboard("{Enter}");

    expect(onSelect).not.toHaveBeenCalled();
  });

  it("virtualizes large lists: only mounts a window of rows, not all 500", () => {
    const big: CraftEssence[] = Array.from({ length: 500 }, (_, i) => ({
      id: i + 1,
      rarity: 5,
      category: "normal",
      name: `CE ${i + 1}`,
    }));
    setup({ craftEssences: big });

    const options = screen.queryAllByRole("option");
    // Viewport is 420px / 40px row + overscan; we expect well under 100
    // rows mounted and definitely fewer than the full 500.
    expect(options.length).toBeGreaterThan(0);
    expect(options.length).toBeLessThan(100);

  });

  it("clears the search when the dialog is closed via onOpenChange", async () => {
    const user = userEvent.setup();
    const onOpenChange = vi.fn();
    const { rerender } = renderWithTheme(
      <CraftEssenceSelectDialog
        open={true}
        onOpenChange={onOpenChange}
        onSelect={vi.fn()}
        craftEssences={FIXTURE}
      />
    );

    const search = screen.getByPlaceholderText(
      "搜索礼装名称..."
    ) as HTMLInputElement;
    await user.type(search, "kal");
    expect(search.value).toBe("kal");

    // Close by pressing Escape — Radix Dialog forwards this through the
    // controlled `onOpenChange`. Our handler clears the search input
    // for next open.
    fireEvent.keyDown(search, { key: "Escape", code: "Escape" });

    await waitFor(() => {
      expect(onOpenChange).toHaveBeenCalledWith(false);
    });

    // Reopen the dialog: the search input should now be empty again.
    rerender(
      <CraftEssenceSelectDialog
        open={true}
        onOpenChange={onOpenChange}
        onSelect={vi.fn()}
        craftEssences={FIXTURE}
      />
    );
    const reopened = screen.getByPlaceholderText(
      "搜索礼装名称..."
    ) as HTMLInputElement;
    expect(reopened.value).toBe("");
  });
});
