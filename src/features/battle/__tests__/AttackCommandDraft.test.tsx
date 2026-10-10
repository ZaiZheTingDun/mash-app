import { describe, it, expect, vi } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { AttackCommandDraft } from "../AttackCommandDraft";
import { ATTACK_OPTIONS } from "../battleSceneModel";
import type { PartyMember } from "../../team/partyServants";

const members: PartyMember[] = [1, 2, 3].map(id => ({
  servant: { id, variantKey: String(id), name_cn: `从者 ${id}`, name_jp: "", name_en: "", class: "saber", rarity: 5 },
  isSupport: id === 2,
}));

describe("AttackCommandDraft", () => {
  it("uses the inline draft heading and chooses a servant before showing attack types", async () => {
    const onDraftChange = vi.fn();
    const onSelect = vi.fn();
    const onCancel = vi.fn();
    const view = renderWithTheme(<AttackCommandDraft draft={{ step: "source", targetIndex: 1 }} members={members} faces={{}}
      onDraftChange={onDraftChange} onSelect={onSelect} onCancel={onCancel} />);
    expect(view.container.querySelector(".command-inline-draft .command-draft-heading")).toHaveTextContent("设置攻击");
    expect(screen.queryByRole("group", { name: "攻击类型" })).not.toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole("button", { name: "从者 2" }));
    expect(onDraftChange).toHaveBeenCalledWith({ step: "option", source: "servant_2", targetIndex: 1 });
    expect(onSelect).not.toHaveBeenCalled();
    await userEvent.setup().click(screen.getByRole("button", { name: "撤销添加行动" }));
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it.each(ATTACK_OPTIONS)("keeps the five options in order and selects $label for the chosen servant", async option => {
    const onSelect = vi.fn();
    const onDraftChange = vi.fn();
    renderWithTheme(<AttackCommandDraft draft={{ step: "option", source: "servant_2", targetIndex: 2 }} members={members} faces={{}}
      onDraftChange={onDraftChange} onSelect={onSelect} onCancel={vi.fn()} />);
    const group = screen.getByRole("group", { name: "攻击类型" });
    expect(within(group).getAllByRole("button").map(button => button.getAttribute("aria-label"))).toEqual(["宝具", "红卡", "蓝卡", "绿卡", "任意"]);
    await userEvent.setup().click(within(group).getByRole("button", { name: option.label }));
    expect(onSelect).toHaveBeenCalledWith("servant_2", option.value);
    await userEvent.setup().click(screen.getByRole("button", { name: "重选" }));
    expect(onDraftChange).toHaveBeenCalledWith({ step: "source", targetIndex: 2 });
  });

  it("loads each option's icon from the installed resources", async () => {
    const imageSpy = vi.spyOn(window, "Image").mockImplementation(function () {
      const image = document.createElement("img");
      Object.defineProperties(image, { complete: { value: true }, naturalWidth: { value: 100 } });
      return image;
    });
    try {
      const filenames = ["skill_00601", "skill_00306", "skill_00305", "skill_00304", "skill_00317"];
      vi.mocked(invoke).mockImplementation(async command => command === "get_attack_card_icon_paths"
        ? filenames.map((filename, index) => ({ path: `/icons/${filename}.png`, name: ATTACK_OPTIONS[index].label })) : null);
      renderWithTheme(<AttackCommandDraft draft={{ step: "option", source: "servant_1", targetIndex: null }} members={members} faces={{}}
        onDraftChange={vi.fn()} onSelect={vi.fn()} onCancel={vi.fn()} />);
      const group = screen.getByRole("group", { name: "攻击类型" });
      await waitFor(() => {
        ATTACK_OPTIONS.forEach((option, index) => {
          expect(within(group).getByRole("img", { name: option.label })).toHaveAttribute("src", `asset:///icons/${filenames[index]}.png`);
        });
      });
    } finally {
      imageSpy.mockRestore();
    }
  });
});
