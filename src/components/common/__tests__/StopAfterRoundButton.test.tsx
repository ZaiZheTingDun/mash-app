import { describe, expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { StopAfterRoundButton } from "../StopAfterRoundButton";

describe("StopAfterRoundButton", () => {
  it("requests stopping after the round", async () => {
    const onClick = vi.fn();
    renderWithTheme(<StopAfterRoundButton onClick={onClick} />);

    const button = screen.getByRole("button", { name: "运行完当前轮次后停止" });
    expect(button).toHaveAttribute("aria-pressed", "false");
    await userEvent.setup().click(button);
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("communicates the selected state and prevents duplicate requests", async () => {
    const onClick = vi.fn();
    renderWithTheme(<StopAfterRoundButton selected onClick={onClick} />);

    const button = screen.getByRole("button", { name: "本轮结束后将停止" });
    expect(button).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByText("STOPPING AFTER ROUND")).toBeInTheDocument();
    await userEvent.setup().click(button);
    expect(onClick).not.toHaveBeenCalled();
  });

  it("does not request stopping when unavailable", async () => {
    const onClick = vi.fn();
    renderWithTheme(<StopAfterRoundButton disabled onClick={onClick} />);
    await userEvent.setup().click(screen.getByRole("button", { name: "运行完当前轮次后停止" }));
    expect(onClick).not.toHaveBeenCalled();
  });
});
