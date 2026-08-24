import { describe, expect, it } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { TURN_HELP_TEXT, TurnHelpTooltip } from "../TurnHelpTooltip";

describe("TurnHelpTooltip", () => {
  it("shows the Turn explanation on hover", async () => {
    const user = userEvent.setup();

    renderWithTheme(<TurnHelpTooltip />);

    await user.hover(screen.getByRole("button", { name: "Turn 帮助" }));

    expect(await screen.findByRole("tooltip")).toHaveTextContent(TURN_HELP_TEXT);
  });
});
