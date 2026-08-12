import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { AddRowTrigger } from "../AddRowTrigger";

describe("AddRowTrigger", () => {
  it("supports a transparent icon background and custom icon size", () => {
    renderWithTheme(
      <AddRowTrigger transparentIconBackground iconSize={36}>
        添加礼装
      </AddRowTrigger>
    );

    const trigger = screen.getByRole("button", { name: "添加礼装" });
    const icon = trigger.querySelector(".add-row-trigger-icon");

    expect(trigger).toHaveClass("is-icon-background-transparent");
    expect(icon).toHaveStyle({ width: "36px", height: "36px", flexBasis: "36px" });
  });
});
