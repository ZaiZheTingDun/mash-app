import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../../test/renderWithTheme";
import type { FormationSharePreview } from "../../../types/formationShare";
import type { ComponentProps } from "react";
import { ProjectBar } from "../ProjectBar";
import { createInitialProjectSlots } from "../../team/projectSlots";

function renderProjectBar(overrides: Partial<ComponentProps<typeof ProjectBar>> = {}) {
  return renderWithTheme(<ProjectBar
    projects={[{ id: "p1", name: "队伍甲", advancedMode: false, supportServantId: null, slots: createInitialProjectSlots(), repeatMission: false }]}
    projectCatalog={{ schemaVersion: 1, groups: [{ id: "weekly", name: "周回", projectIds: ["p1"] }], ungroupedProjectIds: [] }}
    activeProjectId="p1"
    onProjectSelect={vi.fn()}
    onCreateProject={vi.fn()}
    onRenameProject={vi.fn()}
    onDuplicateProject={vi.fn()}
    onDeleteProject={vi.fn()}
    onCreateProjectGroup={vi.fn()}
    onRenameProjectGroup={vi.fn()}
    onDeleteProjectGroup={vi.fn()}
    onMoveProjectToGroup={vi.fn()}
    onReorderProjectGroups={vi.fn()}
    onReorderProjectsInGroup={vi.fn()}
    onOpenProjectSettings={vi.fn()}
    onFormationImported={vi.fn()}
    {...overrides}
  />);
}

async function selectAction(user: ReturnType<typeof userEvent.setup>, name: string) {
  await user.click(screen.getByRole("button", { name: "队伍操作" }));
  await user.click(await screen.findByRole("menuitem", { name }));
}

const preview: FormationSharePreview = {
  token: "preview-token", sourceName: "三面编队", targetName: "三面编队（导入）", advancedMode: false,
  members: [{ position: 1, name: "从者甲", servantId: 309, servantVariantKey: "309:1", faceId: 1, isSupport: false, craftEssences: ["礼装甲"], mlbRequired: true }, { position: 3, name: "从者甲", servantId: 309, servantVariantKey: "309:2", faceId: 2, isSupport: true, craftEssences: ["礼装乙", "礼装丙"], mlbRequired: false }],
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
    renderProjectBar({ beforeShare });
    await selectAction(user, "分享编队");
    expect(beforeShare).toHaveBeenCalledWith("p1");
    expect(invoke).not.toHaveBeenCalledWith("create_formation_share", expect.anything());
    await user.click(screen.getByRole("button", { name: "队伍操作" }));
    expect(await screen.findByRole("menuitem", { name: "分享编队" })).toHaveAttribute("aria-disabled", "true");
    await user.keyboard("{Escape}");
    await act(async () => { finish(); });
    expect(await screen.findByRole("dialog", { name: "分享编队" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "可手动复制的分享码" })).toHaveValue("TUFTSA==");
    expect(screen.getByRole("status")).toHaveTextContent("已复制（8 字符）");
    expect(invoke).toHaveBeenCalledWith("create_formation_share", { projectId: "p1" });
  });

  it("does not share stale saved state after a failed write", async () => {
    const user = userEvent.setup();
    setupInvoke();
    renderProjectBar({ beforeShare: async () => { throw new Error("保存失败"); } });
    await selectAction(user, "分享编队");
    expect(await screen.findByRole("alert")).toHaveTextContent("保存失败");
    expect(invoke).not.toHaveBeenCalledWith("create_formation_share", expect.anything());
  });

  it("provides a selectable full code when the clipboard write fails", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockResolvedValue({ code: "FULL+CODE/==", characters: 12, copied: false });
    renderProjectBar();
    await selectAction(user, "分享编队");
    const text = await screen.findByRole("textbox", { name: "可手动复制的分享码" });
    expect(text).toHaveValue("FULL+CODE/==");
    expect(text).toHaveAttribute("readonly");
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("keeps import available without an existing team", async () => {
    const user = userEvent.setup();
    setupInvoke();
    renderProjectBar({ projects: [], activeProjectId: null });
    expect(screen.queryByRole("button", { name: "粘贴编队" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "队伍操作" }));
    expect(await screen.findByRole("menuitem", { name: "分享编队" })).toHaveAttribute("aria-disabled", "true");
    await user.click(screen.getByRole("menuitem", { name: "新建队伍" }));
    expect(await screen.findByRole("textbox", { name: "编队分享码" })).toBeInTheDocument();
  });

  it("shows only a row of member avatars before importing a new team", async () => {
    const user = userEvent.setup();
    setupInvoke();
    const onImported = vi.fn();
    renderProjectBar({ onFormationImported: onImported });
    await selectAction(user, "新建队伍");
    fireEvent.change(screen.getByRole("textbox", { name: "编队分享码" }), { target: { value: "CODE+TEXT/==" } });
    expect(await screen.findByRole("list", { name: "编队成员" })).toBeInTheDocument();
    expect(screen.getAllByRole("listitem")).toHaveLength(2);
    const support = screen.getByRole("listitem", { name: "第3位 · 从者甲（助战）" });
    const owned = screen.getByRole("listitem", { name: "第1位 · 从者甲" });
    expect(within(support).getByLabelText("助战")).toHaveClass("battle-support-badge");
    expect(within(owned).queryByLabelText("助战")).not.toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("get_servant_face_path", { servantId: 309, variantKey: "309:1", faceId: 1 });
    expect(invoke).toHaveBeenCalledWith("get_servant_face_path", { servantId: 309, variantKey: "309:2", faceId: 2 });
    expect(screen.queryByRole("table")).not.toBeInTheDocument();
    expect(screen.queryByText("礼装乙 / 礼装丙")).not.toBeInTheDocument();
    expect(screen.queryByText("第六位尚未选择从者。")).not.toBeInTheDocument();
    expect(screen.queryByText(/运行设为单次/)).not.toBeInTheDocument();
    expect(screen.queryByText("普通指令 · 第1面 · 第1回合")).not.toBeInTheDocument();
    fireEvent.change(screen.getByRole("textbox", { name: "队伍名称" }), { target: { value: "新编队" } });
    await user.click(screen.getByRole("button", { name: "新建" }));
    await waitFor(() => expect(onImported).toHaveBeenCalledWith(project));
    expect(invoke).toHaveBeenCalledWith("import_formation_share", { previewToken: "preview-token", name: "新编队", groupId: "weekly" });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("invalidates the token when edited text no longer matches its preview", async () => {
    const user = userEvent.setup();
    setupInvoke();
    renderProjectBar();
    await selectAction(user, "新建队伍");
    const input = screen.getByRole("textbox", { name: "编队分享码" });
    fireEvent.change(input, { target: { value: "FIRST" } });
    await screen.findByRole("list", { name: "编队成员" });
    fireEvent.change(input, { target: { value: "DIFFERENT" } });
    expect(invoke).toHaveBeenCalledWith("discard_formation_share_preview", { previewToken: "preview-token" });
    expect(invoke).not.toHaveBeenCalledWith("import_formation_share", expect.anything());
  });

  it("discards late preview responses after the dialog is closed", async () => {
    const user = userEvent.setup();
    let complete!: (value: FormationSharePreview) => void;
    vi.mocked(invoke).mockImplementation(async (command) => command === "preview_formation_share" ? new Promise<FormationSharePreview>((resolve) => { complete = resolve; }) : null);
    const { unmount } = renderProjectBar();
    await selectAction(user, "新建队伍");
    fireEvent.change(screen.getByRole("textbox", { name: "编队分享码" }), { target: { value: "CODE" } });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("preview_formation_share", { text: "CODE" }));
    unmount();
    await act(async () => { complete(preview); });
    expect(invoke).toHaveBeenCalledWith("discard_formation_share_preview", { previewToken: "preview-token" });
  });

  it("leaves the dialog open and reports expired or failed imports", async () => {
    const user = userEvent.setup();
    setupInvoke();
    const onImported = vi.fn();
    renderProjectBar({ onFormationImported: onImported });
    await selectAction(user, "新建队伍");
    fireEvent.change(screen.getByRole("textbox", { name: "编队分享码" }), { target: { value: "CODE" } });
    await screen.findByRole("list", { name: "编队成员" });
    vi.mocked(invoke).mockRejectedValueOnce("编队预览已过期，请重新预览");
    await user.click(screen.getByRole("button", { name: "新建" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("已过期");
    expect(onImported).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog", { name: "新建队伍" })).toBeInTheDocument();
  });

  it("places sharing immediately below settings without standalone share or paste buttons", async () => {
    const user = userEvent.setup();
    renderProjectBar();
    expect(screen.queryByRole("button", { name: /分享|粘贴编队/ })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "队伍操作" }));
    expect((await screen.findAllByRole("menuitem")).slice(0, 2).map((item) => item.textContent)).toEqual(["队伍设置", "分享编队"]);
  });

  it("blocks invalid codes and restores ordinary creation when the input is cleared", async () => {
    const user = userEvent.setup();
    const onCreateProject = vi.fn();
    const onImported = vi.fn();
    vi.mocked(invoke).mockRejectedValue("分享码无效");
    renderProjectBar({ onCreateProject, onFormationImported: onImported });
    await selectAction(user, "新建队伍");
    const input = screen.getByRole("textbox", { name: "编队分享码" });
    fireEvent.change(input, { target: { value: "INVALID" } });
    expect(screen.getByRole("button", { name: "新建" })).toBeDisabled();
    expect(screen.queryByRole("combobox", { name: "队伍模式" })).not.toBeInTheDocument();
    expect(await screen.findByRole("alert")).toHaveTextContent("分享码无效");
    fireEvent.submit(screen.getByRole("button", { name: "新建" }).closest("form")!);
    expect(onCreateProject).not.toHaveBeenCalled();
    expect(onImported).not.toHaveBeenCalled();
    fireEvent.change(input, { target: { value: "" } });
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "队伍模式" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "新建" }));
    expect(onCreateProject).toHaveBeenCalledWith("队伍 2", false, "saber", "weekly");
  });

  it("keeps a custom name and allows a failed import to be previewed again", async () => {
    const user = userEvent.setup();
    setupInvoke();
    const onImported = vi.fn();
    renderProjectBar({ onFormationImported: onImported });
    await selectAction(user, "新建队伍");
    fireEvent.change(screen.getByRole("textbox", { name: "队伍名称" }), { target: { value: "自定义名称" } });
    fireEvent.change(screen.getByRole("textbox", { name: "编队分享码" }), { target: { value: "CODE" } });
    await screen.findByRole("list", { name: "编队成员" });
    expect(screen.getByRole("textbox", { name: "队伍名称" })).toHaveValue("自定义名称");
    vi.mocked(invoke).mockRejectedValueOnce("编队预览已过期，请重新预览");
    await user.click(screen.getByRole("button", { name: "新建" }));
    await screen.findByRole("alert");
    await user.click(screen.getByRole("button", { name: "重新读取" }));
    expect(screen.getByRole("button", { name: "新建" })).toBeDisabled();
    await screen.findByRole("list", { name: "编队成员" });
    expect(invoke).toHaveBeenCalledWith("discard_formation_share_preview", { previewToken: "preview-token" });
    await user.click(screen.getByRole("button", { name: "新建" }));
    expect(invoke).toHaveBeenCalledWith("import_formation_share", { previewToken: "preview-token", name: "自定义名称", groupId: "weekly" });
    expect(onImported).toHaveBeenCalledWith(project);
  });

  it("discards an old response while a newer input is waiting for preview", async () => {
    const user = userEvent.setup();
    let complete!: (value: FormationSharePreview) => void;
    setupInvoke();
    vi.mocked(invoke).mockImplementationOnce(() => new Promise<FormationSharePreview>((resolve) => { complete = resolve; }));
    renderProjectBar();
    await selectAction(user, "新建队伍");
    const input = screen.getByRole("textbox", { name: "编队分享码" });
    fireEvent.change(input, { target: { value: "OLD" } });
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("preview_formation_share", { text: "OLD" }));
    fireEvent.change(input, { target: { value: "NEW" } });
    await act(async () => { complete({ ...preview, token: "old-token", sourceName: "旧编队" }); });
    expect(screen.queryByText("旧编队")).not.toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("discard_formation_share_preview", { previewToken: "old-token" });
    expect(screen.getByRole("button", { name: "新建" })).toBeDisabled();
    await screen.findByRole("list", { name: "编队成员" });
    expect(screen.getByRole("button", { name: "新建" })).toBeEnabled();
  });

  it("clears the code and token when the create dialog is closed and reopened", async () => {
    const user = userEvent.setup();
    setupInvoke();
    renderProjectBar();
    await selectAction(user, "新建队伍");
    fireEvent.change(screen.getByRole("textbox", { name: "编队分享码" }), { target: { value: "CODE" } });
    await screen.findByRole("list", { name: "编队成员" });
    await user.click(screen.getByRole("button", { name: "取消" }));
    expect(invoke).toHaveBeenCalledWith("discard_formation_share_preview", { previewToken: "preview-token" });
    await selectAction(user, "新建队伍");
    expect(screen.getByRole("textbox", { name: "编队分享码" })).toHaveValue("");
    expect(screen.getByRole("textbox", { name: "队伍名称" })).toHaveValue("队伍 2");
    expect(screen.queryByText("三面编队")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "新建" })).toBeEnabled();
  });

  it("prevents closing or submitting twice while an import is pending", async () => {
    const user = userEvent.setup();
    setupInvoke();
    const onImported = vi.fn();
    renderProjectBar({ onFormationImported: onImported });
    await selectAction(user, "新建队伍");
    fireEvent.change(screen.getByRole("textbox", { name: "编队分享码" }), { target: { value: "CODE" } });
    await screen.findByRole("list", { name: "编队成员" });
    let complete!: (value: typeof project) => void;
    vi.mocked(invoke).mockImplementationOnce(() => new Promise<typeof project>((resolve) => { complete = resolve; }));
    await user.click(screen.getByRole("button", { name: "新建" }));
    expect(screen.getByRole("button", { name: "取消" })).toBeDisabled();
    expect(screen.getByRole("textbox", { name: "编队分享码" })).toBeDisabled();
    expect(screen.getByRole("textbox", { name: "队伍名称" })).toBeDisabled();
    expect(screen.getByRole("combobox", { name: "所属分组" })).toBeDisabled();
    fireEvent.submit(screen.getByRole("button", { name: "新建" }).closest("form")!);
    await user.keyboard("{Escape}");
    expect(screen.getByRole("dialog", { name: "新建队伍" })).toBeInTheDocument();
    expect(vi.mocked(invoke).mock.calls.filter(([command]) => command === "import_formation_share")).toHaveLength(1);
    await act(async () => { complete(project); });
    expect(onImported).toHaveBeenCalledOnce();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});
