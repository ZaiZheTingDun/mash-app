import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../../test/renderWithTheme";
import type { FormationSharePreview } from "../../../types/formationShare";
import { FormationShareActions } from "../FormationShareActions";
import { FormationShareImportDialog } from "../FormationShareImportDialog";

const preview: FormationSharePreview = {
  token: "preview-token", sourceName: "三面编队", targetName: "三面编队（导入）", advancedMode: false,
  members: [{ position: 1, name: "从者甲", isSupport: false, craftEssences: ["礼装甲"], mlbRequired: true }, { position: 3, name: "从者甲", isSupport: true, craftEssences: ["礼装乙", "礼装丙"], mlbRequired: false }],
  supportRequirements: ["技能：10 / 任意 / 10"],
  sections: [{ title: "普通指令 · 第1面 · 第1回合", lines: ["1. 第3位（助战） · 技能1 → 第1位"] }],
  warnings: ["第六位尚未选择从者。"],
};

const project = { id: "new-project", name: "新编队", slots: [] };

function setupInvoke() {
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "create_formation_share") return { code: "TUFTSA==", characters: 8, copied: true };
    if (command === "preview_formation_share") return preview;
    if (command === "import_formation_share") return project;
    return null;
  });
}

describe("formation share", () => {
  it("waits for pending saves before generating a code and reports copying", async () => {
    const user = userEvent.setup();
    setupInvoke();
    let finish!: () => void;
    const saves = new Promise<void>((resolve) => { finish = resolve; });
    const beforeShare = vi.fn(() => saves);
    renderWithTheme(<FormationShareActions projectId="p1" groups={[]} beforeShare={beforeShare} />);
    await user.click(screen.getByRole("button", { name: "分享" }));
    expect(beforeShare).toHaveBeenCalledWith("p1");
    expect(invoke).not.toHaveBeenCalledWith("create_formation_share", expect.anything());
    expect(screen.getByRole("button", { name: "分享" })).toBeDisabled();
    await act(async () => { finish(); });
    expect(await screen.findByRole("status")).toHaveTextContent("已复制（8 字符）");
    expect(invoke).toHaveBeenCalledWith("create_formation_share", { projectId: "p1" });
  });

  it("does not share stale saved state after a failed write", async () => {
    const user = userEvent.setup();
    setupInvoke();
    renderWithTheme(<FormationShareActions projectId="p1" groups={[]} beforeShare={async () => { throw new Error("保存失败"); }} />);
    await user.click(screen.getByRole("button", { name: "分享" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("保存失败");
    expect(invoke).not.toHaveBeenCalledWith("create_formation_share", expect.anything());
  });

  it("provides a selectable full code when the clipboard write fails", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockResolvedValue({ code: "FULL+CODE/==", characters: 12, copied: false });
    renderWithTheme(<FormationShareActions projectId="p1" groups={[]} />);
    await user.click(screen.getByRole("button", { name: "分享" }));
    const text = await screen.findByRole("textbox", { name: "可手动复制的分享码" });
    expect(text).toHaveValue("FULL+CODE/==");
    expect(text).toHaveAttribute("readonly");
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("keeps import available without an existing team", async () => {
    const user = userEvent.setup();
    setupInvoke();
    renderWithTheme(<FormationShareActions projectId={null} groups={[]} onFormationImported={vi.fn()} />);
    expect(screen.getByRole("button", { name: "分享" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "粘贴编队" }));
    expect(await screen.findByRole("textbox", { name: "编队分享码" })).toBeInTheDocument();
  });

  it("previews support identity and actions before importing a new team", async () => {
    const user = userEvent.setup();
    setupInvoke();
    const onImported = vi.fn();
    const onClose = vi.fn();
    renderWithTheme(<FormationShareImportDialog onImported={onImported} onClose={onClose} />);
    expect(screen.getByRole("button", { name: "导入为新队伍" })).toBeDisabled();
    fireEvent.change(screen.getByRole("textbox", { name: "编队分享码" }), { target: { value: "CODE+TEXT/==" } });
    await user.click(screen.getByRole("button", { name: "预览" }));
    expect(await screen.findByText("助战")).toBeInTheDocument();
    expect(screen.getByText("礼装乙 / 礼装丙")).toBeInTheDocument();
    expect(screen.getByText("第六位尚未选择从者。")).toBeInTheDocument();
    await user.click(screen.getByText("普通指令 · 第1面 · 第1回合"));
    expect(screen.getByText(/第3位（助战） · 技能1/)).toBeVisible();
    fireEvent.change(screen.getByRole("textbox", { name: "保存名称" }), { target: { value: "新编队" } });
    await user.click(screen.getByRole("button", { name: "导入为新队伍" }));
    await waitFor(() => expect(onImported).toHaveBeenCalledWith(project));
    expect(invoke).toHaveBeenCalledWith("import_formation_share", { previewToken: "preview-token", name: "新编队", groupId: null });
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("invalidates the token when edited text no longer matches its preview", async () => {
    const user = userEvent.setup();
    setupInvoke();
    renderWithTheme(<FormationShareImportDialog onImported={vi.fn()} onClose={vi.fn()} />);
    const input = screen.getByRole("textbox", { name: "编队分享码" });
    fireEvent.change(input, { target: { value: "FIRST" } });
    await user.click(screen.getByRole("button", { name: "预览" }));
    await screen.findByText("三面编队");
    fireEvent.change(input, { target: { value: "DIFFERENT" } });
    expect(screen.getByRole("button", { name: "导入为新队伍" })).toBeDisabled();
    expect(invoke).toHaveBeenCalledWith("discard_formation_share_preview", { previewToken: "preview-token" });
    expect(invoke).not.toHaveBeenCalledWith("import_formation_share", expect.anything());
  });

  it("discards late preview responses after the dialog is closed", async () => {
    const user = userEvent.setup();
    let complete!: (value: FormationSharePreview) => void;
    vi.mocked(invoke).mockImplementation(async (command) => command === "preview_formation_share" ? new Promise<FormationSharePreview>((resolve) => { complete = resolve; }) : null);
    const { unmount } = renderWithTheme(<FormationShareImportDialog onImported={vi.fn()} onClose={vi.fn()} />);
    fireEvent.change(screen.getByRole("textbox", { name: "编队分享码" }), { target: { value: "CODE" } });
    await user.click(screen.getByRole("button", { name: "预览" }));
    unmount();
    await act(async () => { complete(preview); });
    expect(invoke).toHaveBeenCalledWith("discard_formation_share_preview", { previewToken: "preview-token" });
  });

  it("leaves the dialog open and reports expired or failed imports", async () => {
    const user = userEvent.setup();
    setupInvoke();
    const onImported = vi.fn();
    const onClose = vi.fn();
    renderWithTheme(<FormationShareImportDialog onImported={onImported} onClose={onClose} />);
    fireEvent.change(screen.getByRole("textbox", { name: "编队分享码" }), { target: { value: "CODE" } });
    await user.click(screen.getByRole("button", { name: "预览" }));
    await screen.findByText("三面编队");
    vi.mocked(invoke).mockRejectedValueOnce("编队预览已过期，请重新预览");
    await user.click(screen.getByRole("button", { name: "导入为新队伍" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("已过期");
    expect(onImported).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  });
});
