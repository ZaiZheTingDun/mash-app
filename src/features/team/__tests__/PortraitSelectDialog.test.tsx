import { expect, it, vi } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { PortraitSelectDialog } from "../PortraitSelectDialog";

it("lists portraits horizontally and saves the selected global portrait", async () => {
  const user = userEvent.setup();
  const onOpenChange = vi.fn();
  const onSaved = vi.fn();
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === "list_servant_portraits") {
      return {
        options: [
          { id: 1, path: "/tmp/narrow_servant_1.png" },
          { id: 4, path: "/tmp/narrow_servant_4.png" },
        ],
        selectedId: 1,
      };
    }
    return null;
  });

  renderWithTheme(
    <PortraitSelectDialog
      open
      onOpenChange={onOpenChange}
      servantId={1}
      variantKey="1:1"
      servantName="玛修"
      onSaved={onSaved}
    />
  );

  await user.click(await screen.findByRole("button", { name: "立绘 4" }));
  await user.click(screen.getByRole("button", { name: "确认" }));

  await waitFor(() => {
    expect(invoke).toHaveBeenCalledWith("save_servant_portrait_selection", {
      variantKey: "1:1",
      portraitId: 4,
    });
    expect(onSaved).toHaveBeenCalledTimes(1);
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });
});
