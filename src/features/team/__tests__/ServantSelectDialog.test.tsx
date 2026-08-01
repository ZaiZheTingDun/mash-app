import { describe, it, expect, vi } from "vitest";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
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

  it("opens portrait settings on right click and refreshes the picker avatar", async () => {
    const user = userEvent.setup();
    const onPortraitSaved = vi.fn();
    let selectedPortraitId = 4_000_130;
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
        return null;
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

    const option = screen.getByText("Ｕ－奥尔加玛丽").closest('[role="option"]');
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
