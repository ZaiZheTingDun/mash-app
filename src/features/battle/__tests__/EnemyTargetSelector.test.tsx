import { describe, expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { EnemyTargetSelector } from "../EnemyTargetSelector";

describe("EnemyTargetSelector", () => {
  it("clears a configured target and also supports clicking it again", async () => {
    const onChange = vi.fn();
    const user = userEvent.setup();
    renderWithTheme(<EnemyTargetSelector value="enemy_3" onChange={onChange} />);

    expect(screen.getByText(/当前目标：敌人 3/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "清除目标" }));
    expect(onChange).toHaveBeenLastCalledWith(null);
    await user.click(screen.getByRole("button", { name: "敌人 3" }));
    expect(onChange).toHaveBeenLastCalledWith(null);
  });

  it("disables clearing when there is no selected target", () => {
    renderWithTheme(<EnemyTargetSelector onChange={vi.fn()} />);
    expect(screen.getByRole("button", { name: "清除目标" })).toBeDisabled();
    expect(screen.getByText("当前未指定敌方目标")).toBeInTheDocument();
  });
});
