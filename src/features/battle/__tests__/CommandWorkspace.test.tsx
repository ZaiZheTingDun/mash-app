import { describe, expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { CommandWorkspace } from "../CommandWorkspace";

describe("CommandWorkspace step numbering", () => {
  it.each([false, true])("numbers the visible steps consecutively when advanced is %s", advanced => {
    const { container } = renderWithTheme(
      <CommandWorkspace wave={0} waveCount={1} turn={0} turns={[{ id: "turn-1" }]} step="attack" advanced={advanced}
        onStep={vi.fn()} onWave={vi.fn()} onTurn={vi.fn()} onAddTurn={vi.fn()} onDeleteTurn={vi.fn()}>
        <span>内容</span>
      </CommandWorkspace>
    );
    expect(Array.from(container.querySelectorAll(".command-step-node"), node => node.textContent)).toEqual(advanced ? ["01", "02", "03", "04"] : ["01", "02", "03"]);
    expect(container.querySelector(".command-phase-header strong")).toHaveTextContent(advanced ? "04" : "03");
    expect(screen.queryByRole("button", { name: /控制行动/ }) != null).toBe(advanced);
  });
});
