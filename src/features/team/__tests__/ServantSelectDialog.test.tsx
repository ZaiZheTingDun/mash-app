import { describe, it, expect, vi } from "vitest";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { ServantSelectDialog } from "../ServantSelectDialog";
import type { Servant } from "../../../types/servant";

const FIXTURE: Servant[] = [
  {
    id: 284,
    variantKey: "284",
    name_cn: "阿尔托莉雅·卡斯特",
    name_jp: "アルトリア・キャスター",
    name_en: "Altria Caster",
    class: "Caster",
    rarity: 5,
    noblePhantasmName: "真圆集",
  },
  {
    id: 150,
    variantKey: "150",
    name_cn: "梅林",
    name_jp: "マーリン",
    name_en: "Merlin",
    class: "Caster",
    rarity: 5,
    noblePhantasmName: "永久关闭的理想乡",
  },
  {
    id: 215,
    variantKey: "215",
    name_cn: "斯卡哈",
    name_jp: "スカサハ",
    name_en: "Scathach",
    class: "Lancer",
    rarity: 5,
    noblePhantasmName: "贯穿死翔之枪",
  },
];

function setup(overrides?: {
  servants?: Servant[];
  disabledIds?: number[];
  onSelect?: (s: Servant) => void;
  onOpenChange?: (open: boolean) => void;
  onPortraitSaved?: () => void;
  portraitRefreshKey?: number;
  defaultClassFilter?: string;
}) {
  const onSelect = overrides?.onSelect ?? vi.fn();
  const onOpenChange = overrides?.onOpenChange ?? vi.fn();
  const utils = renderWithTheme(
    <ServantSelectDialog
      open={true}
      onOpenChange={onOpenChange}
      onSelect={onSelect}
      servants={overrides?.servants ?? FIXTURE}
      disabledIds={overrides?.disabledIds}
      onPortraitSaved={overrides?.onPortraitSaved}
      portraitRefreshKey={overrides?.portraitRefreshKey}
      defaultClassFilter={overrides?.defaultClassFilter}
    />
  );
  return { ...utils, onSelect, onOpenChange };
}

function makeServants(count: number): Servant[] {
  return Array.from({ length: count }, (_, i) => ({
    ...FIXTURE[0],
    id: i + 1,
    variantKey: String(i + 1),
    name_cn: `从者 ${i + 1}`,
  }));
}

describe("ServantSelectDialog", () => {
  it("renders the dialog title and search field", () => {
    setup();
    expect(screen.getByText("选择从者")).toBeInTheDocument();
    expect(
      screen.getByPlaceholderText("搜索从者名称...")
    ).toBeInTheDocument();
  });

  it("filters by Chinese name (case-insensitive substring)", async () => {
    const user = userEvent.setup();
    setup();

    expect(screen.getByText("梅林")).toBeInTheDocument();
    expect(screen.getByText("斯卡哈")).toBeInTheDocument();

    await user.type(
      screen.getByPlaceholderText("搜索从者名称..."),
      "梅"
    );
    expect(screen.getByText("梅林")).toBeInTheDocument();
    expect(screen.queryByText("斯卡哈")).not.toBeInTheDocument();
  });

  it("switches layouts beside the search field while keeping filters and search", async () => {
    const user = userEvent.setup();
    setup();
    const search = screen.getByPlaceholderText("搜索从者名称...");
    const toggle = screen.getByRole("button", { name: "切换为网格布局" });
    expect(search.closest(".servant-search-row")).toContainElement(toggle);
    expect(screen.queryByRole("button", { name: "切换为列表布局" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /调整头像大小/ })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "术阶" }));
    await user.click(screen.getByRole("button", { name: "★5" }));
    await user.type(search, "Mer");
    await user.click(screen.getByRole("button", { name: "切换为网格布局" }));

    expect(screen.getByRole("button", { name: "切换为列表布局" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "切换为网格布局" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /调整头像大小/ })).toBeInTheDocument();
    expect(search).toHaveValue("Mer");
    expect(screen.getByRole("button", { name: "术阶" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByRole("button", { name: "★5" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getAllByRole("option")).toHaveLength(1);
    expect(screen.getByRole("option", { name: "梅林" })).toBeInTheDocument();
    expect(screen.queryByText("梅林")).not.toBeInTheDocument();
    expect(screen.queryByText("永久关闭的理想乡")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "切换为列表布局" }));
    expect(screen.getAllByRole("option")).toHaveLength(1);
    expect(screen.getByText("梅林")).toBeInTheDocument();
  });

  it("restores the saved layout and size when a new picker mounts", async () => {
    const user = userEvent.setup();
    const first = setup();
    await user.click(screen.getByRole("button", { name: "切换为网格布局" }));
    await user.click(screen.getByRole("button", { name: "调整头像大小（当前：中）" }));
    expect(invoke).toHaveBeenCalledWith("update_servant_picker_settings", { action: "toggleLayout" });
    expect(invoke).toHaveBeenCalledWith("update_servant_picker_settings", { action: "cycleGridSize" });
    first.unmount();

    const second = setup();
    expect(await screen.findByRole("button", { name: "调整头像大小（当前：大）" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "切换为列表布局" }));
    second.unmount();

    setup();
    const toggle = screen.getByRole("button", { name: "切换为网格布局" });
    await waitFor(() => expect(toggle).toBeEnabled());
    expect(screen.queryByRole("button", { name: /调整头像大小/ })).not.toBeInTheDocument();
    await user.click(toggle);
    expect(screen.getByRole("button", { name: "调整头像大小（当前：大）" })).toBeInTheDocument();
  });

  it("keeps the saved layout when a pending settings save fails", async () => {
    const user = userEvent.setup();
    const defaultInvoke = vi.mocked(invoke).getMockImplementation()!;
    let rejectSave!: (error: Error) => void;
    vi.mocked(invoke).mockImplementation((command, args) => {
      if (command === "update_servant_picker_settings") {
        return new Promise((_, reject) => { rejectSave = reject; });
      }
      return defaultInvoke(command, args);
    });
    try {
      setup();
      const toggle = screen.getByRole("button", { name: "切换为网格布局" });
      await user.click(toggle);
      expect(toggle).toBeDisabled();
      expect(screen.queryByRole("button", { name: "切换为列表布局" })).not.toBeInTheDocument();
      await act(async () => { rejectSave(new Error("磁盘不可写")); });
      expect(screen.getByRole("alert")).toHaveTextContent("保存显示设置失败");
      expect(toggle).toBeEnabled();
      expect(screen.queryByRole("button", { name: /调整头像大小/ })).not.toBeInTheDocument();
    } finally {
      vi.mocked(invoke).mockImplementation(defaultInvoke);
    }
  });

  it("defaults to medium grid size and preserves size across layout changes", async () => {
    const user = userEvent.setup();
    setup({ servants: makeServants(120) });
    await user.click(screen.getByRole("button", { name: "切换为网格布局" }));
    const sizeButton = screen.getByRole("button", { name: "调整头像大小（当前：中）" });
    const toggle = screen.getByRole("button", { name: "切换为列表布局" });
    expect(sizeButton.compareDocumentPosition(toggle) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

    const expectGridSize = (faceSize: number, columns: number, rowHeight: number) => {
      const options = screen.getAllByRole("option");
      expect(options[0].querySelector(".servant-face-frame")).toHaveStyle({
        width: `${faceSize}px`, height: `${faceSize}px`,
      });
      expect(within(options[0]).getByLabelText("稀有度 5 星")).toHaveTextContent("★★★★★");
      expect(options[0].querySelector(".servant-face-frame")).toContainElement(
        within(options[0]).getByRole("img", { name: "术阶" }),
      );
      expect(options[columns - 1]).toHaveStyle({ top: "0px" });
      expect(options[columns]).toHaveStyle({ top: `${rowHeight}px` });
    };
    expectGridSize(64, 6, 80);
    const scroll = screen.getByRole("listbox").parentElement!;
    fireEvent.scroll(scroll, { target: { scrollTop: 640 } });
    await user.click(sizeButton);
    expect(scroll.scrollTop).toBe(0);
    expectGridSize(80, 5, 96);
    const search = screen.getByPlaceholderText("搜索从者名称...");
    fireEvent.keyDown(search, { key: "ArrowDown" });
    expect(screen.getByRole("option", { name: "从者 6" })).toHaveAttribute("aria-selected", "true");

    await user.click(screen.getByRole("button", { name: "调整头像大小（当前：大）" }));
    expectGridSize(48, 8, 64);
    expect(screen.getByRole("option", { name: "从者 1" })).toHaveAttribute("aria-selected", "true");
    await user.click(toggle);
    expect(screen.queryByRole("button", { name: /调整头像大小/ })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "切换为网格布局" }));
    expectGridSize(48, 8, 64);
    await user.click(screen.getByRole("button", { name: "调整头像大小（当前：小）" }));
    expectGridSize(64, 6, 80);
  });

  it("shows faces with class and rarity in the grid and reveals full details after a sustained hover", async () => {
    const user = userEvent.setup();
    const { onSelect } = setup();
    await user.click(screen.getByRole("button", { name: "切换为网格布局" }));
    const option = screen.getByRole("option", { name: "梅林" });
    expect(option.querySelector(".servant-face-frame")).toBeInTheDocument();
    expect(option.querySelector(".servant-option-text")).toBeNull();
    const rarity = within(option).getByLabelText("稀有度 5 星");
    expect(rarity).toHaveTextContent("★★★★★");
    expect(option.querySelector(".servant-face-frame")).toContainElement(rarity);
    expect(option.querySelector(".servant-face-frame")).toContainElement(
      within(option).getByRole("img", { name: "术阶" }),
    );
    expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();

    vi.useFakeTimers();
    try {
      fireEvent.pointerMove(option, { pointerType: "mouse" });
      await act(async () => { vi.advanceTimersByTime(900); });
      expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
      fireEvent.pointerLeave(option);
      await act(async () => { vi.advanceTimersByTime(200); });
      expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();

      fireEvent.pointerMove(option, { pointerType: "mouse" });
      await act(async () => { vi.advanceTimersByTime(1000); });
      const tooltip = screen.getByRole("tooltip");
      expect(tooltip).toHaveTextContent("梅林");
      expect(tooltip).toHaveTextContent("宝具：永久关闭的理想乡");
      expect(onSelect).not.toHaveBeenCalled();
      fireEvent.pointerLeave(option);
      expect(screen.queryByRole("tooltip")).not.toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  it("virtualizes grid rows, selects scrolled items, and resets scroll on layout changes", async () => {
    const user = userEvent.setup();
    const servants = makeServants(120);
    const { onSelect } = setup({ servants });
    await user.click(screen.getByRole("button", { name: "切换为网格布局" }));

    const options = screen.getAllByRole("option");
    expect(options.length).toBeLessThan(servants.length);
    expect(options[0].style.top).toBe(options[5].style.top);
    expect(options[6]).toHaveStyle({ top: "80px" });
    const search = screen.getByPlaceholderText("搜索从者名称...");
    fireEvent.keyDown(search, { key: "ArrowDown" });
    expect(options[6]).toHaveAttribute("aria-selected", "true");
    fireEvent.keyDown(search, { key: "ArrowUp" });
    expect(options[0]).toHaveAttribute("aria-selected", "true");
    expect(screen.queryByRole("option", { name: "从者 120" })).not.toBeInTheDocument();
    await waitFor(() => {
      const faceCalls = vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "get_servant_face_path");
      expect(faceCalls.length).toBeLessThan(servants.length);
    });

    const scroll = screen.getByRole("listbox").parentElement!;
    fireEvent.scroll(scroll, { target: { scrollTop: 80 * 19 } });
    expect(screen.queryByRole("option", { name: "从者 1" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("option", { name: "从者 120" }));
    expect(onSelect).toHaveBeenCalledWith(servants[119]);

    fireEvent.scroll(scroll, { target: { scrollTop: 80 * 19 } });
    await user.click(screen.getByRole("button", { name: "切换为列表布局" }));
    expect(scroll.scrollTop).toBe(0);
    expect(screen.getByText("从者 1")).toBeInTheDocument();
  });

  it("adapts grid columns to the viewport and navigates by row and column", async () => {
    const width = vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(350);
    try {
      const user = userEvent.setup();
      const servants = makeServants(12);
      const { onSelect } = setup({ servants, disabledIds: [12] });
      await user.click(screen.getByRole("button", { name: "切换为网格布局" }));
      const options = screen.getAllByRole("option");
      expect(options[0].style.top).toBe(options[3].style.top);
      expect(options[4]).toHaveStyle({ top: "80px" });
      expect(screen.queryByRole("option", { name: "从者 12" })).not.toBeInTheDocument();

      const search = screen.getByPlaceholderText("搜索从者名称...");
      fireEvent.keyDown(search, { key: "ArrowDown" });
      expect(options[4]).toHaveAttribute("aria-selected", "true");
      fireEvent.keyDown(search, { key: "ArrowUp" });
      fireEvent.keyDown(search, { key: "ArrowRight" });
      expect(options[0]).toHaveAttribute("aria-selected", "true");
      fireEvent.keyDown(options[0], { key: "ArrowRight" });
      fireEvent.keyDown(options[0], { key: "ArrowDown" });
      expect(options[5]).toHaveAttribute("aria-selected", "true");
      fireEvent.keyDown(options[0], { key: "ArrowLeft" });
      fireEvent.keyDown(options[0], { key: "Enter" });
      expect(onSelect).toHaveBeenCalledWith(servants[4]);
    } finally {
      width.mockRestore();
    }
  });

  it("filters and displays by server Chinese name when present", async () => {
    const user = userEvent.setup();
    setup({
      servants: [
        {
          id: 1,
          variantKey: "1",
          name_cn: "旧中文名",
          name_cn_server: "国服中文名",
          name_jp: "サーヴァント",
          name_en: "Servant",
          class: "Shielder",
          rarity: 4,
          noblePhantasmName: "宝具",
        },
      ],
    });

    expect(screen.getByText("国服中文名")).toBeInTheDocument();
    await user.type(screen.getByPlaceholderText("搜索从者名称..."), "国服");
    expect(screen.getByText("国服中文名")).toBeInTheDocument();
  });

  it("filters by overwrite servant aliases while displaying the formal name", async () => {
    const user = userEvent.setup();
    setup({
      servants: [
        {
          id: 244,
          variantKey: "244",
          name_cn: "吉娜可·加里吉利",
          name_jp: "ジナコ＝カリギリ",
          name_en: "Jinako Carigiri",
          overWriteServantNames: [
            {
              ids: [1],
              nameCn: "伟大的石像神",
              nameJp: "大いなる石像神",
            },
          ],
          class: "Moon Cancer",
          rarity: 5,
          noblePhantasmName: "肉弹啊，明天再开始努力吧",
        },
        ...FIXTURE,
      ],
    });

    await user.type(screen.getByPlaceholderText("搜索从者名称..."), "石像神");
    expect(screen.getByText("吉娜可·加里吉利")).toBeInTheDocument();
    expect(screen.queryByText("伟大的石像神")).not.toBeInTheDocument();

    await user.clear(screen.getByPlaceholderText("搜索从者名称..."));
    await user.type(screen.getByPlaceholderText("搜索从者名称..."), "大いなる");
    expect(screen.getByText("吉娜可·加里吉利")).toBeInTheDocument();
  });

  it("filters by English name (case-insensitive)", async () => {
    const user = userEvent.setup();
    setup();
    await user.type(
      screen.getByPlaceholderText("搜索从者名称..."),
      "scath"
    );
    expect(screen.getByText("斯卡哈")).toBeInTheDocument();
    expect(screen.queryByText("梅林")).not.toBeInTheDocument();
  });

  it("filters by class icon and rarity", async () => {
    const user = userEvent.setup();
    setup({
      servants: [
        ...FIXTURE,
        {
          id: 16,
          variantKey: "16",
          name_cn: "阿拉什",
          name_jp: "アーラシュ",
          name_en: "Arash",
          class: "Archer",
          rarity: 1,
          noblePhantasmName: "流星一条",
        },
      ],
    });

    await user.click(screen.getByRole("button", { name: "弓阶" }));
    await user.click(screen.getByRole("button", { name: "★1" }));
    expect(screen.getByText("阿拉什")).toBeInTheDocument();
    expect(screen.queryByText("梅林")).not.toBeInTheDocument();
  });

  it("renders all plus five rarity buttons on the right", async () => {
    const user = userEvent.setup();
    setup({
      servants: [
        ...FIXTURE,
        {
          id: 16,
          variantKey: "16",
          name_cn: "阿拉什",
          name_jp: "アーラシュ",
          name_en: "Arash",
          class: "Archer",
          rarity: 1,
        },
      ],
    });

    const rarityGroup = screen.getByRole("group", { name: "稀有度筛选" });
    expect(within(rarityGroup).getAllByRole("button")).toHaveLength(6);
    expect(
      within(rarityGroup).getByRole("button", { name: "全部稀有度" })
    ).toHaveAttribute("aria-pressed", "true");
    expect(
      within(rarityGroup).queryByRole("button", { name: "★0" })
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("combobox", { name: "稀有度筛选" })
    ).not.toBeInTheDocument();

    await user.click(within(rarityGroup).getByRole("button", { name: "★1" }));
    expect(screen.getByText("阿拉什")).toBeInTheDocument();
    expect(screen.queryByText("梅林")).not.toBeInTheDocument();
    await user.click(
      within(rarityGroup).getByRole("button", { name: "全部稀有度" })
    );
    expect(screen.getByText("梅林")).toBeInTheDocument();
  });

  it("renders available classes as a flat icon group", () => {
    setup();

    expect(screen.getByRole("group", { name: "职阶筛选" })).toBeInTheDocument();
    const allClasses = screen.getByRole("button", { name: "全部职阶" });
    expect(allClasses).toHaveAttribute(
      "aria-pressed",
      "true"
    );
    expect(
      allClasses.querySelector(".servant-class-filter-icon.gold")
    ).toHaveAttribute(
      "src",
      expect.stringContaining("gold_all.png")
    );
    expect(screen.getByRole("button", { name: "术阶" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "枪阶" })).toBeInTheDocument();
    expect(
      screen.queryByRole("combobox", { name: "职阶筛选" })
    ).not.toBeInTheDocument();
  });

  it("layers silver and gold class icons and updates the selected state", async () => {
    const user = userEvent.setup();
    setup();

    const allClasses = screen.getByRole("button", { name: "全部职阶" });
    const caster = screen.getByRole("button", { name: "术阶" });
    expect(
      caster.querySelector(".servant-class-filter-icon.silver")
    ).toHaveAttribute(
      "src",
      expect.stringContaining("silver_caster.png")
    );
    expect(
      caster.querySelector(".servant-class-filter-icon.gold")
    ).toHaveAttribute(
      "src",
      expect.stringContaining("gold_caster.png")
    );

    await user.click(caster);

    expect(allClasses).toHaveAttribute("aria-pressed", "false");
    expect(caster).toHaveAttribute("aria-pressed", "true");
  });

  it("groups playable Beast variants under the Beast icon", async () => {
    const user = userEvent.setup();
    setup({
      servants: [
        {
          id: 377,
          variantKey: "377",
          name_cn: "所多玛之兽／德拉科",
          name_jp: "ソドムズビースト／ドラコー",
          name_en: "Sodom's Beast/Draco",
          class: "Beast",
          rarity: 5,
        },
        {
          id: 417,
          variantKey: "417",
          name_cn: "埃列什基伽勒",
          name_jp: "エレシュキガル",
          name_en: "Ereshkigal",
          class: "BeastEresh",
          rarity: 5,
        },
        FIXTURE[0],
      ],
    });

    await user.click(screen.getByRole("button", { name: "兽阶" }));
    expect(screen.getByText("所多玛之兽／德拉科")).toBeInTheDocument();
    expect(screen.getByText("埃列什基伽勒")).toBeInTheDocument();
    expect(screen.queryByText("阿尔托莉雅·卡斯特")).not.toBeInTheDocument();
  });

  it("uses the Extra1 grouped filter for Grand projects", () => {
    setup({
      defaultClassFilter: "Extra1",
      servants: [
        {
          id: 1,
          variantKey: "1",
          name_cn: "玛修",
          name_jp: "マシュ",
          name_en: "Mash",
          class: "Shielder",
          rarity: 4,
        },
        {
          id: 2,
          variantKey: "2",
          name_cn: "贞德",
          name_jp: "ジャンヌ",
          name_en: "Jeanne",
          class: "Ruler",
          rarity: 5,
        },
        {
          id: 3,
          variantKey: "3",
          name_cn: "阿比盖尔",
          name_jp: "アビゲイル",
          name_en: "Abigail",
          class: "Foreigner",
          rarity: 5,
        },
      ],
    });

    expect(screen.getByRole("button", { name: "Extra1" })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByText("玛修")).toBeInTheDocument();
    expect(screen.getByText("贞德")).toBeInTheDocument();
    expect(screen.queryByText("阿比盖尔")).not.toBeInTheDocument();
  });

  it("renders noble phantasm names in the second row", () => {
    setup();
    expect(screen.getByText("永久关闭的理想乡")).toBeInTheDocument();
  });

  it("overlays the rarity on the servant face", () => {
    setup();

    const option = screen.getByText("梅林").closest('[role="option"]');
    const face = option?.querySelector(".servant-face-frame");
    const rarity = within(option as HTMLElement).getByLabelText("稀有度 5 星");

    expect(face).toContainElement(rarity);
    expect(rarity).toHaveTextContent("★★★★★");
    expect(
      screen.getByText("梅林").parentElement
    ).not.toContainElement(rarity);
  });

  it("renders the class icon before the servant name with rarity-based color", () => {
    setup({
      servants: [
        FIXTURE[0],
        {
          id: 16,
          variantKey: "16",
          name_cn: "阿拉什",
          name_jp: "アーラシュ",
          name_en: "Arash",
          class: "Archer",
          rarity: 1,
          noblePhantasmName: "流星一条",
        },
      ],
    });

    const goldOption = screen.getByText("阿尔托莉雅·卡斯特").closest('[role="option"]');
    const goldIcon = within(goldOption as HTMLElement).getByRole("img", {
      name: "术阶",
    });
    expect(goldIcon).toHaveAttribute(
      "src",
      expect.stringContaining("gold_caster.png")
    );
    expect(
      goldIcon.compareDocumentPosition(screen.getByText("阿尔托莉雅·卡斯特")) &
        Node.DOCUMENT_POSITION_FOLLOWING
    ).toBeTruthy();

    const silverOption = screen.getByText("阿拉什").closest('[role="option"]');
    expect(
      within(silverOption as HTMLElement).getByRole("img", { name: "弓阶" })
    ).toHaveAttribute("src", expect.stringContaining("silver_archer.png"));
    expect(goldOption?.querySelector(".servant-class-badge")).toBeNull();
  });

  it.each([
    { servantClass: "Caster", rarity: 5, label: "术阶", icon: "gold_caster.png" },
    { servantClass: "Archer", rarity: 1, label: "弓阶", icon: "silver_archer.png" },
    { servantClass: "BeastEresh", rarity: 5, label: "兽阶", icon: "gold_beast.png" },
    { servantClass: "Unknown", rarity: 3, label: "Unknown", icon: "silver_unknown.png" },
  ])("overlays the $servantClass class icon on grid faces", async ({ servantClass, rarity, label, icon }) => {
    const user = userEvent.setup();
    setup({ servants: [{ ...FIXTURE[0], class: servantClass, rarity }] });
    await user.click(screen.getByRole("button", { name: "切换为网格布局" }));
    const option = screen.getByRole("option", { name: "阿尔托莉雅·卡斯特" });
    const classIcon = within(option).getByRole("img", { name: label });
    expect(classIcon).toHaveAttribute("src", expect.stringContaining(icon));
    expect(classIcon).toHaveClass("servant-face-class-icon");
    expect(option.querySelector(".servant-face-frame")).toContainElement(classIcon);
  });

  it("renders multiple variants for the same servant id", () => {
    setup({
      servants: [
        {
          id: 1,
          variantKey: "1:1",
          faceId: 800170,
          name_cn: "玛修",
          name_jp: "マシュ",
          name_en: "Mash",
          class: "Shielder",
          rarity: 4,
          noblePhantasmName: "已然遥远的理想之城",
        },
        {
          id: 1,
          variantKey: "1:2",
          faceId: 800151,
          name_cn: "玛修",
          name_jp: "マシュ",
          name_en: "Mash",
          class: "Shielder",
          rarity: 4,
          noblePhantasmName: "依然存在的梦想之城",
        },
      ],
    });

    expect(screen.getAllByText("玛修")).toHaveLength(2);
    expect(screen.getByText("已然遥远的理想之城")).toBeInTheDocument();
    expect(screen.getByText("依然存在的梦想之城")).toBeInTheDocument();
  });

  it("requests each variant face by its max variant id", async () => {
    setup({
      servants: [
        {
          id: 1,
          variantKey: "1:1",
          faceId: 800170,
          name_cn: "玛修",
          name_jp: "マシュ",
          name_en: "Mash",
          class: "Shielder",
          rarity: 4,
          noblePhantasmName: "已然遥远的理想之城",
        },
        {
          id: 1,
          variantKey: "1:2",
          faceId: 800151,
          name_cn: "玛修",
          name_jp: "マシュ",
          name_en: "Mash",
          class: "Shielder",
          rarity: 4,
          noblePhantasmName: "依然存在的梦想之城",
        },
      ],
    });

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("get_servant_face_path", {
        servantId: 1,
        faceId: 800170,
        variantKey: "1:1",
      });
      expect(invoke).toHaveBeenCalledWith("get_servant_face_path", {
        servantId: 1,
        faceId: 800151,
        variantKey: "1:2",
      });
    });
  });

  it.each(["list", "grid"])("opens portrait settings and refreshes the avatar in %s layout", async (layout) => {
    const user = userEvent.setup();
    const onPortraitSaved = vi.fn();
    let selectedPortraitId = 4_000_130;
    const defaultInvoke = vi.mocked(invoke).getMockImplementation()!;
    vi.mocked(invoke).mockImplementation(
      async (cmd: string, args) => {
        if (cmd === "get_servant_face_path") {
          return `/tmp/face_servant_${selectedPortraitId}.png`;
        }
        if (cmd === "list_servant_portraits") {
          return {
            options: [
              { id: 1, path: "/tmp/narrow_servant_1.png" },
              { id: 2, path: "/tmp/narrow_servant_2.png" },
              { id: 4_000_130, path: "/tmp/narrow_servant_4000130.png" },
            ],
            selectedId: 4_000_130,
          };
        }
        if (cmd === "save_servant_portrait_selection") {
          selectedPortraitId = (args as Record<string, unknown> | undefined)
            ?.portraitId as number;
        }
        return defaultInvoke(cmd, args);
      }
    );
    const { onSelect } = setup({
      servants: [
        {
          id: 444,
          variantKey: "444:1",
          faceId: 4_000_130,
          name_cn: "Ｕ－奥尔加玛丽",
          name_jp: "Ｕ－オルガマリー",
          name_en: "U-Olga Marie",
          class: "Beast",
          rarity: 5,
        },
      ],
      onPortraitSaved,
    });

    if (layout === "grid") {
      await user.click(screen.getByRole("button", { name: "切换为网格布局" }));
    }

    const option = screen.getByRole("option", { name: /Ｕ－奥尔加玛丽/ });
    await waitFor(() => {
      expect(option?.querySelector(".servant-face-frame img")).toHaveAttribute(
        "src",
        "asset:///tmp/face_servant_4000130.png"
      );
    });

    fireEvent.contextMenu(option as HTMLElement);
    expect(
      await screen.findByText("立绘设置 — Ｕ－奥尔加玛丽")
    ).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("list_servant_portraits", {
      servantId: 444,
      variantKey: "444:1",
    });
    expect(onSelect).not.toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: "立绘 2" }));
    await user.click(screen.getByRole("button", { name: "确认" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("save_servant_portrait_selection", {
        variantKey: "444:1",
        portraitId: 2,
      });
      expect(onPortraitSaved).toHaveBeenCalledTimes(1);
      expect(option?.querySelector(".servant-face-frame img")).toHaveAttribute(
        "src",
        "asset:///tmp/face_servant_2.png"
      );
    });
  });

  it("virtualizes large lists and only resolves visible faces", async () => {
    const servants: Servant[] = Array.from({ length: 120 }, (_, i) => ({
      id: i + 1,
      variantKey: String(i + 1),
      name_cn: `从者 ${i + 1}`,
      name_jp: `サーヴァント ${i + 1}`,
      name_en: `Servant ${i + 1}`,
      class: "Caster",
      rarity: 5,
      noblePhantasmName: `宝具 ${i + 1}`,
    }));

    setup({ servants });

    const options = screen.queryAllByRole("option");
    expect(options.length).toBeGreaterThan(0);
    expect(options.length).toBeLessThan(40);

    await waitFor(() => {
      const faceCalls = vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "get_servant_face_path");
      expect(faceCalls.length).toBeGreaterThan(0);
      expect(faceCalls.length).toBeLessThan(40);
    });
  });

  it("hides servants whose ids appear in disabledIds", () => {
    setup({ disabledIds: [150] });
    expect(screen.queryByText("梅林")).not.toBeInTheDocument();
    // Other servants are still visible.
    expect(screen.getByText("阿尔托莉雅·卡斯特")).toBeInTheDocument();
    expect(screen.getByText("斯卡哈")).toBeInTheDocument();
  });

  it("invokes onSelect and closes the dialog on row click", async () => {
    const user = userEvent.setup();
    const { onSelect, onOpenChange } = setup();
    await user.click(screen.getByText("梅林"));
    expect(onSelect).toHaveBeenCalledWith(
      expect.objectContaining({ id: 150, name_cn: "梅林" })
    );
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("shows the empty-state when nothing matches", async () => {
    const user = userEvent.setup();
    setup();
    await user.type(
      screen.getByPlaceholderText("搜索从者名称..."),
      "no-such-servant-zzzz"
    );
    expect(
      screen.getByText("未找到匹配的从者")
    ).toBeInTheDocument();
  });

  it("supports keyboard selection (ArrowDown + Enter)", async () => {
    const user = userEvent.setup();
    const { onSelect } = setup();
    await user.click(
      screen.getByPlaceholderText("搜索从者名称...")
    );
    await user.keyboard("{ArrowDown}{Enter}");
    // activeIndex starts at 0; ArrowDown takes it to index 1 (梅林).
    expect(onSelect).toHaveBeenCalledWith(
      expect.objectContaining({ id: 150, name_cn: "梅林" })
    );
  });
});
