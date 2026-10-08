import { useState } from "react";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { DiamondSwitch } from "../DiamondSwitch";

describe("DiamondSwitch", () => {
  it("can be toggled with Space and retains its checked semantics", async () => {
    function Harness() {
      const [checked, setChecked] = useState(false);
      return <DiamondSwitch aria-label="技能使用确认" checked={checked} onCheckedChange={setChecked} />;
    }
    const user = userEvent.setup();
    renderWithTheme(<Harness />);
    const toggle = screen.getByRole("switch", { name: "技能使用确认" });
    expect(toggle).not.toBeChecked();
    await user.tab();
    expect(toggle).toHaveFocus();
    await user.keyboard(" ");
    expect(toggle).toBeChecked();
    await user.keyboard(" ");
    expect(toggle).not.toBeChecked();
  });

  it("cannot change a disabled setting", async () => {
    const onCheckedChange = vi.fn();
    renderWithTheme(<DiamondSwitch aria-label="牵绊升级自动停止" checked disabled onCheckedChange={onCheckedChange} />);
    const toggle = screen.getByRole("switch", { name: "牵绊升级自动停止" });
    await userEvent.setup().click(toggle);
    expect(toggle).toBeDisabled();
    expect(toggle).toBeChecked();
    expect(onCheckedChange).not.toHaveBeenCalled();
  });
});
