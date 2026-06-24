import { describe, it, expect, vi } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { SkillOptionButtons } from "../SkillOptionButtons";
import type { SkillIcons } from "../../../features/team/useServantSkillIcons";
import type { Servant } from "../../../types/servant";

function makeServant(id: number): Servant {
  return {
    id,
    variantKey: `${id}:1`,
    name_cn: "测试从者",
    name_jp: "テスト",
    name_en: "Test",
    class: "saber",
    rarity: 5,
    noblePhantasmName: null,
    noblePhantasmCard: null,
    overWriteServantNames: [],
    faceId: null,
  };
}

const NO_ICONS: Record<string, SkillIcons> = {};

const WITH_ICONS: Record<string, SkillIcons> = {
  "1:1": [
    { src: "asset://icons/skill_001.png", name: "技能甲" },
    { src: "asset://icons/skill_002.png", name: "技能乙" },
    { src: null, name: "" },
  ],
};

describe("SkillOptionButtons", () => {
  it("renders three buttons with default accessible names when servant is null", () => {
    renderWithTheme(
      <SkillOptionButtons servant={null} skillIcons={NO_ICONS} onSelect={vi.fn()} />
    );
    expect(screen.getByRole("button", { name: "技能 1" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "技能 2" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "技能 3" })).toBeInTheDocument();
  });

  it("renders three buttons with default accessible names when no icons are loaded", () => {
    const servant = makeServant(1);
    renderWithTheme(
      <SkillOptionButtons servant={servant} skillIcons={NO_ICONS} onSelect={vi.fn()} />
    );
    expect(screen.getByRole("button", { name: "技能 1" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "技能 2" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "技能 3" })).toBeInTheDocument();
  });

  it("uses CN skill name as accessible name and tooltip when icons are loaded", () => {
    const servant = makeServant(1);
    renderWithTheme(
      <SkillOptionButtons servant={servant} skillIcons={WITH_ICONS} onSelect={vi.fn()} />
    );
    expect(screen.getByRole("button", { name: "技能甲" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "技能乙" })).toBeInTheDocument();
    // third slot has no icon, falls back to default label
    expect(screen.getByRole("button", { name: "技能 3" })).toBeInTheDocument();
  });

  it("calls onSelect with the correct skill key when a button is clicked", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    const servant = makeServant(1);
    renderWithTheme(
      <SkillOptionButtons servant={servant} skillIcons={NO_ICONS} onSelect={onSelect} />
    );
    await user.click(screen.getByRole("button", { name: "技能 2" }));
    expect(onSelect).toHaveBeenCalledWith("skill_2");
  });
});
