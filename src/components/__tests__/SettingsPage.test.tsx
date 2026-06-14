import { useState } from "react";
import { describe, expect, it, vi, beforeEach } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../test/renderWithTheme";
import { SettingsDialog, type SettingsSection } from "../SettingsPage";
import type { SelfCheckStatus } from "../../types/selfCheck";

const selfCheckStatus: SelfCheckStatus = {
  appVersion: "0.5.4",
  cvRuntimeVersion: "2026.05.08-runtime1",
  cvRuntimeInstalled: true,
  cvCodeVersion: "v0.3.5",
  cvCodeInstalled: true,
  assetVersion: 2,
  appAssetsVersion: 2,
  servants: { entries: 3, hasImage: true, hasJson: true },
  ces: { entries: 5, hasImage: true, hasJson: false },
};

function SettingsHarness({ initialSection = "selfCheck" }: { initialSection?: SettingsSection }) {
  const [section, setSection] = useState<SettingsSection>(initialSection);
  return (
    <SettingsDialog
      open
      section={section}
      onOpenChange={vi.fn()}
      onSectionChange={setSection}
    />
  );
}

describe("SettingsDialog", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_self_check_status") return selfCheckStatus;
      if (cmd === "get_runtime_status") return { installed: true };
      if (cmd === "get_asset_bundle_status") return { installed: true };
      return null;
    });
  });

  it("runs self-check on entry and can run it again", async () => {
    const user = userEvent.setup();
    renderWithTheme(<SettingsHarness />);

    expect(await screen.findByText("0.5.4")).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("get_self_check_status");

    await user.click(screen.getByRole("button", { name: "重新自检" }));

    await waitFor(() => {
      expect(vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "get_self_check_status"))
        .toHaveLength(2);
    });
  });

  it("switches to resource management from the left navigation", async () => {
    const user = userEvent.setup();
    renderWithTheme(<SettingsHarness />);

    await user.click(screen.getByRole("button", { name: "资源管理" }));

    expect(await screen.findByText("管理 CV 运行时和素材包下载")).toBeInTheDocument();
    expect(screen.getByText("素材包")).toBeInTheDocument();
  });
});
