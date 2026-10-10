import { describe, expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { TaskActionButton } from "../TaskActionButton";

describe("TaskActionButton", () => {
  it("supports the formation action with its own accessible label", async () => {
    const onClick = vi.fn();
    renderWithTheme(<TaskActionButton label="指令设置" eyebrow="NEXT" onClick={onClick} />);

    expect(screen.getByText("NEXT")).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole("button", { name: "指令设置" }));
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("preserves start, stop, and disabled action behavior", async () => {
    const onClick = vi.fn();
    const { rerender } = renderWithTheme(<TaskActionButton disabled onClick={onClick} />);
    await userEvent.setup().click(screen.getByRole("button", { name: "开始任务" }));
    expect(onClick).not.toHaveBeenCalled();

    rerender(<TaskActionButton running onClick={onClick} />);
    expect(screen.getByText("STOP")).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole("button", { name: "停止任务" }));
    expect(onClick).toHaveBeenCalledOnce();
  });
});
