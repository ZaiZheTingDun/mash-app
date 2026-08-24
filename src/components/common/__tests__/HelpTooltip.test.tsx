import { describe, expect, it } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { HelpTooltip } from "../HelpTooltip";

describe("HelpTooltip", () => {
  it("shows reusable help content on hover", async () => {
    const user = userEvent.setup();

    renderWithTheme(<HelpTooltip ariaLabel="测试说明" content="通用说明内容" />);

    await user.hover(screen.getByRole("button", { name: "测试说明" }));

    expect(await screen.findByRole("tooltip")).toHaveTextContent("通用说明内容");
  });
});
