import { describe, expect, it, vi } from "vitest";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
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
    expect(Array.from(container.querySelectorAll(".command-step-node"), node => node.textContent)).toEqual(advanced ? ["01", "02", "03", "04", "05"] : ["01", "02", "03"]);
    expect(container.querySelector(".command-phase-header strong")).toHaveTextContent(advanced ? "05" : "03");
    expect(screen.queryByRole("button", { name: /基础配置/ }) != null).toBe(advanced);
    expect(screen.queryByRole("button", { name: /控制行动/ }) != null).toBe(advanced);
  });

  it("shows only preparation, target, and attack in later Grand Battle turns", () => {
    const { container } = renderWithTheme(
      <CommandWorkspace wave={0} waveCount={1} turn={1} turns={[{ id: "turn-1" }, { id: "turn-2" }]} step="prep" advanced
        onStep={vi.fn()} onWave={vi.fn()} onTurn={vi.fn()} onAddTurn={vi.fn()} onDeleteTurn={vi.fn()}>
        <span>内容</span>
      </CommandWorkspace>
    );
    const steps = within(container.querySelector(".command-step-list")!);
    expect(steps.getAllByRole("button").map(button => button.textContent)).toEqual([
      "01准备阶段PREPARATION", "02敌方目标TARGET", "03攻击阶段ATTACK",
    ]);
    expect(screen.queryByRole("button", { name: /基础配置|控制行动/ })).not.toBeInTheDocument();
  });

  it("prevents changing steps while a command mutation is pending", async () => {
    const onStep = vi.fn();
    renderWithTheme(
      <CommandWorkspace wave={0} waveCount={1} turn={0} turns={[{ id: "turn-1" }]} step="prep" busy
        onStep={onStep} onWave={vi.fn()} onTurn={vi.fn()} onAddTurn={vi.fn()} onDeleteTurn={vi.fn()}>
        <span>内容</span>
      </CommandWorkspace>
    );
    const attack = screen.getByRole("button", { name: /攻击阶段/ });
    expect(attack).toBeDisabled();
    await userEvent.setup().click(attack);
    expect(onStep).not.toHaveBeenCalled();
  });
});

describe("CommandWorkspace turn pagination", () => {
  const turns = [{ id: "first" }, { id: "second" }, { id: "third" }];

  it.each([0, 1, 2])("navigates from turn %s and disables the boundary arrows", async turn => {
    const onTurn = vi.fn();
    const onAddTurn = vi.fn();
    const user = userEvent.setup();
    renderWithTheme(
      <CommandWorkspace wave={0} waveCount={1} turn={turn} turns={turns} step="prep"
        onStep={vi.fn()} onWave={vi.fn()} onTurn={onTurn} onAddTurn={onAddTurn} onDeleteTurn={vi.fn()}>
        <span>内容</span>
      </CommandWorkspace>
    );
    const pager = within(screen.getByRole("group", { name: `第 ${turn + 1}/3 回合` }));
    expect(pager.getByText(String(turn + 1).padStart(2, "0"))).toBeInTheDocument();
    expect(pager.getByText("/ 03")).toBeInTheDocument();
    const previous = pager.getByRole("button", { name: "上一回合" });
    const next = pager.getByRole("button", { name: "下一回合" });
    expect(previous).toHaveProperty("disabled", turn === 0);
    expect(next).toHaveProperty("disabled", turn === 2);
    await user.click(previous);
    await user.click(next);
    expect(onTurn.mock.calls).toEqual([
      ...(turn > 0 ? [[turn - 1]] : []),
      ...(turn < 2 ? [[turn + 1]] : []),
    ]);
    await user.click(pager.getByRole("button", { name: "添加 Turn" }));
    expect(onAddTurn).toHaveBeenCalledOnce();
  });

  it("disables turn navigation and addition while saving", async () => {
    const onTurn = vi.fn();
    const onAddTurn = vi.fn();
    renderWithTheme(
      <CommandWorkspace wave={0} waveCount={1} turn={1} turns={turns} step="prep" busy
        onStep={vi.fn()} onWave={vi.fn()} onTurn={onTurn} onAddTurn={onAddTurn} onDeleteTurn={vi.fn()}>
        <span>内容</span>
      </CommandWorkspace>
    );
    const pager = within(screen.getByRole("group", { name: "第 2/3 回合" }));
    for (const button of pager.getAllByRole("button")) {
      expect(button).toBeDisabled();
      await userEvent.setup().click(button);
    }
    expect(onTurn).not.toHaveBeenCalled();
    expect(onAddTurn).not.toHaveBeenCalled();
  });
});
