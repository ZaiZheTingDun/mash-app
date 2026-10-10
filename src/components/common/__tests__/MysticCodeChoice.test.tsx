import { screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { MysticCodeChoice } from "../MysticCodeChoice";
import { MysticCodeIcon } from "../MysticCodeIcon";
import { MysticCodeSelector } from "../../../features/team/MysticCodeSelector";
import { renderWithTheme } from "../../../test/renderWithTheme";
import type { MysticCode } from "../../../types/mysticCode";
import * as tauri from "../../../tauri";

const code: MysticCode = {
  id: 1, name: "测试御主礼装", itemMalePath: null, itemFemalePath: null,
  masterFigureMalePath: null, masterFigureFemalePath: null,
  masterFaceMalePath: null, masterFaceFemalePath: null, skills: [],
};

describe("MysticCodeChoice", () => {
  it.each([null, code])("shares the selector fallback when the item image is unavailable: %s", async selected => {
    renderWithTheme(<>
      <MysticCodeChoice code={selected} onClick={vi.fn()} />
      <MysticCodeSelector expanded codes={selected ? [selected] : []} selectedId={selected?.id ?? null} gender="female" onSelect={vi.fn()} />
      <MysticCodeIcon code={selected} label="技能行御主礼装" size="2" inline />
    </>);
    const choice = screen.getByRole("button", { name: "御主礼装" });
    const selector = screen.getByRole("button", { name: selected ? `御主礼装：${selected.name}` : "选择御主礼装" });
    expect(await within(choice).findByText("礼", { exact: true })).toBeInTheDocument();
    expect(await within(selector).findByText("礼", { exact: true })).toBeInTheDocument();
    const actionIcon = screen.getByLabelText("技能行御主礼装");
    expect(await within(actionIcon).findByText("礼", { exact: true })).toBeInTheDocument();
    for (const icon of [choice, selector, actionIcon]) {
      expect(icon.querySelector(".mystic-code-icon")).toHaveAttribute("data-accent-color", "blue");
    }
    expect(choice).toHaveAttribute("title", selected?.name ?? "御主礼装");
  });

  it.each(["male", "female"] as const)("uses the same %s item resource in choices, selectors and action rows", gender => {
    const convert = vi.spyOn(tauri, "convertFileSrc");
    const selected = { ...code, itemMalePath: "/resources/male.png", itemFemalePath: "/resources/female.png" };
    renderWithTheme(<>
      <MysticCodeChoice code={selected} gender={gender} onClick={vi.fn()} />
      <MysticCodeSelector expanded codes={[selected]} selectedId={selected.id} gender={gender} onSelect={vi.fn()} />
      <MysticCodeIcon code={selected} gender={gender} label="技能行御主礼装" size="2" inline />
    </>);
    expect(convert.mock.calls).toEqual([
      [`/resources/${gender}.png`],
      [`/resources/${gender}.png`],
      [`/resources/${gender}.png`],
    ]);
    convert.mockRestore();
  });
});
