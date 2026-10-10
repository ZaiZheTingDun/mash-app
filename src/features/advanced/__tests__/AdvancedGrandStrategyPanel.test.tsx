import { useState } from "react";
import { describe, it, expect, vi } from "vitest";
import { screen, within, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { GrandCardStrategyPanel } from "../AdvancedGrandStrategyPanel";
import { defaultRuleSlot } from "../advancedCommandModel";
import type { GrandCardStrategy } from "../../../types/project";
import type { PartyMember } from "../../team/partyServants";

const members: PartyMember[] = Array.from({ length: 6 }, (_, index) => ({
  memberId: `member-${index}`,
  servant: { id: index === 5 ? 1 : index + 1, variantKey: String(index + 1), name_cn: `从者 ${index + 1}`, name_jp: "", name_en: "", class: "saber", rarity: 5, noblePhantasmCard: "buster" },
  isSupport: index === 5,
}));
const initial: GrandCardStrategy = { customRules: [{ id: "rule", name: "自定义规则", slots: [defaultRuleSlot(), defaultRuleSlot(), defaultRuleSlot()] }] };

function Harness({ onChange, empty = false, embedded = true }: { onChange: (strategy: GrandCardStrategy) => void; empty?: boolean; embedded?: boolean }) {
  const [strategy, setStrategy] = useState<GrandCardStrategy>(empty ? { customRules: [] } : initial);
  return <GrandCardStrategyPanel strategy={strategy} partyMembers={members} faces={{}} embedded={embedded} onChange={next => { onChange(next); setStrategy(next); }} />;
}

describe("GrandCardStrategyPanel", () => {
  it("highlights only the edited slot until switching, cancelling, or saving", async () => {
    const user = userEvent.setup();
    renderWithTheme(<Harness onChange={vi.fn()} />);
    const firstSlot = screen.getByRole("button", { name: /第 1 张，任意从者/ });
    const secondSlot = screen.getByRole("button", { name: /第 2 张，任意从者/ });
    await user.click(firstSlot);
    expect(firstSlot).toHaveClass("is-editing");
    expect(firstSlot).toHaveAttribute("aria-expanded", "true");
    await user.click(screen.getByRole("button", { name: "从者 4" }));
    await user.click(screen.getByRole("button", { name: "指令卡" }));
    expect(firstSlot).toHaveClass("is-editing");
    await user.click(secondSlot);
    expect(firstSlot).not.toHaveClass("is-editing");
    expect(secondSlot).toHaveClass("is-editing");
    await user.click(screen.getByRole("button", { name: "撤销添加行动" }));
    expect(secondSlot).not.toHaveClass("is-editing");
    expect(secondSlot).toHaveAttribute("aria-expanded", "false");
    await user.click(firstSlot);
    await user.click(screen.getByRole("button", { name: "从者 4" }));
    await user.click(screen.getByRole("button", { name: "宝具" }));
    expect(firstSlot).not.toHaveClass("is-editing");
    expect(firstSlot).toHaveAttribute("aria-expanded", "false");
  });

  it("adds a persisted rule with closed settings, and permits cancelling an explicit edit", async () => {
    const onChange = vi.fn(); const user = userEvent.setup();
    renderWithTheme(<Harness onChange={onChange} empty />);
    await user.click(screen.getByRole("button", { name: "添加规则" }));
    expect(onChange).toHaveBeenCalledOnce();
    expect(onChange.mock.calls[0][0].customRules).toHaveLength(1);
    expect(screen.queryByRole("region", { name: "设置策略" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /第 1 张，任意从者/ }));
    expect(screen.queryByRole("group", { name: "指令卡类型" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "从者 6" }));
    await user.click(screen.getByRole("button", { name: "指令卡" }));
    expect(onChange).toHaveBeenCalledOnce();
    await user.click(screen.getByRole("button", { name: "撤销添加行动" }));
    expect(onChange).toHaveBeenCalledOnce();
    expect(screen.getByRole("button", { name: /第 1 张，任意从者/ })).toBeInTheDocument();
  });

  it("offers all six servants and preserves backline support identity when choosing NP", async () => {
    const onChange = vi.fn(); const user = userEvent.setup();
    renderWithTheme(<Harness onChange={onChange} />);
    await user.click(screen.getByRole("button", { name: /第 2 张，任意从者/ }));
    members.forEach(member => expect(screen.getByRole("button", { name: member.servant!.name_cn })).toBeEnabled());
    const sourceChoices = screen.getByRole("region", { name: "设置策略" }).querySelectorAll(".command-rule-sources .command-actor-choice");
    expect(Array.from(sourceChoices, button => button.getAttribute("aria-label"))).toEqual([
      "从者 1", "从者 2", "从者 3", "从者 4", "从者 5", "从者 6", "冠位从者", "任意从者",
    ]);
    await user.click(screen.getByRole("button", { name: "从者 6" }));
    expect(screen.queryByRole("button", { name: "从者 1" })).not.toBeInTheDocument();
    expect(within(screen.getByRole("group", { name: "指令卡类型" })).getAllByRole("button").map(button => button.getAttribute("aria-label"))).toEqual(["指令卡/宝具", "指令卡", "宝具"]);
    await user.click(screen.getByRole("button", { name: "宝具" }));
    expect(onChange.mock.calls[0][0].customRules[0].slots).toEqual([
      expect.objectContaining({ slotIndex: null, kind: "any" }),
      expect.objectContaining({ memberId: "member-5", slotIndex: 5, servantId: 1, isSupport: true, kind: "np", color: "buster" }),
      expect.objectContaining({ slotIndex: null, kind: "any" }),
    ]);
    expect(screen.queryByRole("region", { name: "设置策略" })).not.toBeInTheDocument();
  });

  it.each(["指令卡/宝具", "指令卡"])("selects a color after %s, and allows reselecting the servant and type", async kind => {
    const onChange = vi.fn(); const user = userEvent.setup();
    renderWithTheme(<Harness onChange={onChange} />);
    await user.click(screen.getByRole("button", { name: /第 1 张，任意从者/ }));
    await user.click(screen.getByRole("button", { name: "从者 4" }));
    expect(screen.getByRole("button", { name: "重选" })).toHaveClass("command-draft-reselect");
    await user.click(screen.getByRole("button", { name: kind }));
    expect(screen.getByRole("group", { name: "指令卡颜色" })).toBeInTheDocument();
    const reselectType = screen.getByRole("button", { name: "重选类型" });
    expect(reselectType).toHaveClass("command-draft-reselect");
    expect(screen.queryByRole("button", { name: "重选" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: / · 重选类型/ })).not.toBeInTheDocument();
    await user.click(reselectType);
    expect(screen.getByRole("group", { name: "指令卡类型" })).toBeInTheDocument();
    expect(screen.queryByRole("group", { name: "指令卡颜色" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "从者 4" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "从者 1" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "重选" }));
    expect(screen.queryByRole("group", { name: "指令卡类型" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "任意从者" }));
    await user.click(screen.getByRole("button", { name: kind }));
    expect(onChange).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "蓝卡" }));
    expect(onChange.mock.calls[0][0].customRules[0].slots[0]).toMatchObject({ memberId: null, slotIndex: null, servantId: null, kind: kind === "指令卡/宝具" ? "any" : "command", color: "arts" });
  });

  it("hides only the combined kind icon and keeps the other resource icons", async () => {
    const imageSpy = vi.spyOn(window, "Image").mockImplementation(function () {
      const image = document.createElement("img");
      Object.defineProperties(image, { complete: { value: true }, naturalWidth: { value: 100 } });
      return image;
    });
    try {
      const filenames = ["skill_00601", "skill_00306", "skill_00305", "skill_00304", "skill_00317"];
      vi.mocked(invoke).mockImplementation(async command => command === "get_attack_card_icon_paths" ? filenames.map(filename => ({ path: `/icons/${filename}.png`, name: filename })) : null);
      const user = userEvent.setup();
      renderWithTheme(<Harness onChange={vi.fn()} />);
      await user.click(screen.getByRole("button", { name: /第 1 张，任意从者/ }));
      await user.click(screen.getByRole("button", { name: "任意从者" }));
      const kindGroup = screen.getByRole("group", { name: "指令卡类型" });
      expect(within(kindGroup).getByRole("button", { name: "指令卡/宝具" }).querySelector(".rt-AvatarRoot")).toBeNull();
      expect(kindGroup.querySelectorAll(".rt-AvatarRoot")).toHaveLength(2);
      await waitFor(() => {
        expect(within(kindGroup).getByRole("img", { name: "指令卡" })).toHaveAttribute("src", "asset:///icons/skill_00317.png");
        expect(within(kindGroup).getByRole("img", { name: "宝具" })).toHaveAttribute("src", "asset:///icons/skill_00601.png");
      });
      await user.click(screen.getByRole("button", { name: "指令卡" }));
      await waitFor(() => {
        ["红卡", "蓝卡", "绿卡", "任意"].forEach((name, index) => expect(screen.getByRole("img", { name })).toHaveAttribute("src", `asset:///icons/${filenames[index + 1]}.png`));
      });
    } finally { imageSpy.mockRestore(); }
  });

  it("keeps the same servant-first flow in the non-embedded dialog", async () => {
    const user = userEvent.setup(); const onChange = vi.fn();
    renderWithTheme(<Harness onChange={onChange} embedded={false} />);
    await user.click(screen.getByRole("button", { name: "指令卡策略" }));
    await user.click(screen.getByRole("button", { name: /第 1 张，任意从者/ }));
    const dialog = screen.getByRole("dialog", { name: "设置策略" });
    expect(within(dialog).queryByRole("group", { name: "指令卡类型" })).not.toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "冠位从者" }));
    await user.click(within(dialog).getByRole("button", { name: "宝具" }));
    expect(onChange.mock.calls[0][0].customRules[0].slots[0]).toMatchObject({ grandServant: true, kind: "np" });
  });
});
