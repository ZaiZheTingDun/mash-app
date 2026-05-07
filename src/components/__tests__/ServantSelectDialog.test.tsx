import { describe, it, expect, vi } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../test/renderWithTheme";
import { ServantSelectDialog } from "../ServantSelectDialog";
import type { Servant } from "../../types/servant";

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

  it("filters by class and rarity", async () => {
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

    await user.click(screen.getByRole("combobox", { name: "职介筛选" }));
    await user.click(await screen.findByRole("option", { name: "Archer" }));
    await user.click(screen.getByRole("combobox", { name: "稀有度筛选" }));
    await user.click(await screen.findByRole("option", { name: "★1" }));
    expect(screen.getByText("阿拉什")).toBeInTheDocument();
    expect(screen.queryByText("梅林")).not.toBeInTheDocument();
  });

  it("renders noble phantasm names in the second row", () => {
    setup();
    expect(screen.getByText("永久关闭的理想乡")).toBeInTheDocument();
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
      });
      expect(invoke).toHaveBeenCalledWith("get_servant_face_path", {
        servantId: 1,
        faceId: 800151,
      });
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
