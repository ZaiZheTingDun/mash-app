import { screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { MashSkillTwoSummary } from "../MashSkillTwoChoice";
import type { Servant } from "../../../types/servant";
import type { ServantAction } from "../../../types/command";

// Render the requested image immediately; jsdom does not load Radix Avatar images.
vi.mock("@radix-ui/themes", async importOriginal => ({
  ...await importOriginal<typeof import("@radix-ui/themes")>(),
  Avatar: ({ src }: { src?: string }) => <img src={src} alt="技能图标" />,
}));
const mash: Servant = { id: 1, variantKey: "1:3", name_cn: "玛修", name_jp: "", name_en: "", class: "shielder", rarity: 5 };
const action: ServantAction = { type: "servant", id: "saved", servant: "servant_1", skill: "skill_2", target: null };
const src = "http://asset.localhost/assets%2Ficons%2Fskill_00419.png";

describe("saved Mash skill form summary", () => {
  it.each([
    [2550, "时为朦胧的白垩之壁 A", "skill_00419.png"],
    [2477450, "测定时间的紫弹之薪 B", "skill_00302.png"],
  ] as const)("uses the explicit form %s even when both actions have no target", (skillFormId, name, filename) => {
    renderWithTheme(<MashSkillTwoSummary action={{ ...action, skillFormId }} servant={mash} src={src} name="默认技能" fallback="2" />);
    expect(screen.getByText(name)).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "技能图标" })).toHaveAttribute("src", `http://asset.localhost/assets%2Ficons%2F${filename}`);
  });

  it.each([undefined, 999999])("preserves the original display for legacy or unknown form IDs (%s)", skillFormId => {
    renderWithTheme(<MashSkillTwoSummary action={{ ...action, skillFormId }} servant={mash} src={src} name="原技能" fallback="2" />);
    expect(screen.getByText("原技能")).toBeInTheDocument();
    expect(screen.getByRole("img")).toHaveAttribute("src", src);
  });

  it("does not override another servant or another skill", () => {
    const { rerender } = renderWithTheme(<MashSkillTwoSummary action={{ ...action, skillFormId: 2477450 }} servant={{ ...mash, id: 2 }} src={src} name="其他从者技能" fallback="2" />);
    expect(screen.getByText("其他从者技能")).toBeInTheDocument();
    rerender(<MashSkillTwoSummary action={{ ...action, skill: "skill_1", skillFormId: 2477450 }} servant={mash} src={src} name="技能一" fallback="1" />);
    expect(screen.getByText("技能一")).toBeInTheDocument();
    expect(screen.getByRole("img")).toHaveAttribute("src", src);
  });
});
