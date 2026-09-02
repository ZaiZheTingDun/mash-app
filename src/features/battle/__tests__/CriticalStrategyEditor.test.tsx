import { fireEvent, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { renderWithTheme } from "../../../test/renderWithTheme";
import type { Servant } from "../../../types/servant";
import type { PartyMember } from "../../team/partyServants";
import { CriticalStrategyEditor } from "../CriticalStrategyEditor";

const servant = (id: number, name: string): Servant => ({
  id,
  variantKey: String(id),
  name_cn: name,
  name_jp: name,
  name_en: name,
  class: "saber",
  rarity: 5,
});

const members: PartyMember[] = [
  { memberId: "one", servant: servant(1, "从者一"), isSupport: false },
  { memberId: "two", servant: servant(2, "从者二"), isSupport: true },
];

function mockHorizontalRects(buttons: HTMLElement[]) {
  buttons.forEach((button, index) => {
    vi.spyOn(button, "getBoundingClientRect").mockReturnValue(
      new DOMRect(index * 80, 0, 64, 48)
    );
  });
}

describe("CriticalStrategyEditor", () => {
  it("reorders servant priority with the horizontal keyboard drag interaction", async () => {
    const onChange = vi.fn();
    renderWithTheme(
      <CriticalStrategyEditor
        partyMembers={members}
        faces={{}}
        onChange={onChange}
      />
    );

    const servantButtons = screen.getAllByRole("button", { name: /拖动调整优先级/ }).slice(0, 2);
    mockHorizontalRects(servantButtons);
    servantButtons[0].focus();
    fireEvent.keyDown(servantButtons[0], { key: " ", code: "Space" });
    await waitFor(() =>
      expect(screen.getAllByRole("status").some((status) => status.textContent)).toBe(true)
    );
    fireEvent.keyDown(document, { key: "ArrowRight", code: "ArrowRight" });
    fireEvent.keyDown(document, { key: " ", code: "Space" });

    expect(onChange).toHaveBeenCalledWith(expect.objectContaining({
      memberPriority: [
        expect.objectContaining({ memberId: "two", slotIndex: 1 }),
        expect.objectContaining({ memberId: "one", slotIndex: 0 }),
      ],
    }));
  });

  it("reorders chain priority with the horizontal keyboard drag interaction", async () => {
    const onChange = vi.fn();
    renderWithTheme(
      <CriticalStrategyEditor partyMembers={members} faces={{}} onChange={onChange} />
    );

    const chainButtons = ["精湛连携", "力击连携", "技击连携", "迅击连携"].map((label) =>
      screen.getByRole("button", { name: `${label}，拖动调整优先级` })
    );
    mockHorizontalRects(chainButtons);
    chainButtons[0].focus();
    fireEvent.keyDown(chainButtons[0], { key: " ", code: "Space" });
    await waitFor(() =>
      expect(screen.getAllByRole("status").some((status) => status.textContent)).toBe(true)
    );
    fireEvent.keyDown(document, { key: "ArrowRight", code: "ArrowRight" });
    fireEvent.keyDown(document, { key: " ", code: "Space" });

    expect(onChange).toHaveBeenCalledWith(expect.objectContaining({
      chainPriority: ["buster", "mighty", "arts", "quick"],
    }));
  });

  it("shows only the first three frontline members", () => {
    renderWithTheme(
      <CriticalStrategyEditor
        partyMembers={[
          ...members,
          { memberId: "three", servant: servant(3, "从者三"), isSupport: false },
          { memberId: "four", servant: servant(4, "后排从者"), isSupport: false },
        ]}
        faces={{}}
        onChange={vi.fn()}
      />
    );

    expect(screen.getByRole("button", { name: /从者三，拖动调整优先级/ })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /后排从者，拖动调整优先级/ })).not.toBeInTheDocument();
  });
});
