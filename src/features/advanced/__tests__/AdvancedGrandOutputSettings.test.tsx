import { describe, expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { GrandOutputSettings } from "../AdvancedGrandOutputSettings";
import type { Servant } from "../../../types/servant";

describe("compact GrandOutputSettings", () => {
  it("updates the inline priority while preserving member identity and the other role", async () => {
    const servant: Servant = { id: 1, variantKey: "1", name_cn: "甲", name_jp: "甲", name_en: "A", class: "saber", rarity: 5 };
    const main = { memberId: "owned", slotIndex: 0, servantId: 1, isSupport: false, role: "main", npCard: "auto" as const, priority: "damage" as const };
    const deputy = { ...main, memberId: "support", slotIndex: 1, isSupport: true, role: "deputy" };
    const onChange = vi.fn();
    renderWithTheme(<GrandOutputSettings compact
      partyMembers={[{ memberId: "owned", servant, isSupport: false }, { memberId: "support", servant, isSupport: true }]}
      grandServants={[main, deputy]} faces={{}} onChange={onChange} />);

    const user = userEvent.setup();
    await user.click(screen.getByRole("combobox", { name: "主冠位出卡策略" }));
    await user.click(screen.getByRole("option", { name: "NP 优先" }));
    expect(onChange).toHaveBeenCalledWith([{ ...main, priority: "np" }, deputy]);
  });
});
