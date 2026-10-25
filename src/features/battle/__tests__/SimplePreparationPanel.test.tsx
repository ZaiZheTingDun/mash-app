import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { SimplePreparationPanel } from "../SimplePreparationPanel";
import type { PartyMember } from "../../team/partyServants";
import type { MysticCode } from "../../../types/mysticCode";
import type { ServantSkillSelectionEntry } from "../useServantSkillSelections";
import { equipmentSkillUsageKey, servantSkillUsageKey } from "../preparationSkillUsage";

const members: PartyMember[] = Array.from({ length: 6 }, (_, i) => ({
  memberId: `member-${i}`, isSupport: i === 2,
  servant: { id: i === 2 ? 1 : i + 1, variantKey: `${i === 2 ? 1 : i + 1}:1`, name_cn: `从者${i + 1}`, name_jp: "", name_en: "", class: "saber", rarity: 5 },
}));
const code: MysticCode = { id: 9, name: "测试礼装", itemMalePath: null, itemFemalePath: null, masterFigureMalePath: null, masterFigureFemalePath: null, masterFaceMalePath: null, masterFaceFemalePath: null,
  skills: [1, 2, 3].map(slot => ({ id: slot, slot, name: `礼装技能${slot}`, iconPath: null, targetingMode: slot === 3 ? "orderChange" : slot === 2 ? "needsTarget" : "noTarget" })) };
const selection: ServantSkillSelectionEntry = { servantCollectionNo: 1, skillId: 1, skillNum: 1, selectionType: "SelectAddInfo", supplementaryTypes: [], options: [{ index: 1, label: "选项乙" }] };
const props = { enabled: true, actionCount: 0, members, faces: {}, skillIcons: {}, mysticCode: code, gender: "female" as const, disableAutoSkillTargetRecognition: false,
  skillTargetStatus: () => "unknown" as const, skillSelection: () => null, isSkillLoading: () => false };
const servantSkill = () => within(screen.getByRole("group", { name: "前排 1 技能" })).getByRole("button", { name: "技能 1" });

async function openEnemyTarget() {
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "令咒 / 敌方目标" }));
  await user.click(screen.getByRole("menuitem", { name: "敌方目标" }));
  return user;
}

describe("SimplePreparationPanel", () => {
  it("marks used servant and equipment icons without disabling another click", async () => {
    const user = userEvent.setup(); const onAction = vi.fn();
    const usedSkills = new Set([servantSkillUsageKey(members[0], "skill_1"), equipmentSkillUsageKey("skill_1")]);
    renderWithTheme(<SimplePreparationPanel {...props} usedSkills={usedSkills} onAction={onAction}>配置</SimplePreparationPanel>);
    const equipment = screen.getByRole("button", { name: "礼装技能1" });
    expect(servantSkill()).toHaveClass("is-used");
    expect(equipment).toHaveClass("is-used");
    expect(servantSkill()).toBeEnabled();
    expect(equipment).toBeEnabled();
    expect(screen.getByRole("button", { name: "礼装技能2" })).not.toHaveClass("is-used");
    await user.click(servantSkill());
    await user.click(equipment);
    expect(onAction).toHaveBeenCalledTimes(2);
  });

  it("scrolls the lower list after the saved skill renders and preserves scroll position on edits", async () => {
    const user = userEvent.setup(); const onAction = vi.fn();
    const { rerender } = renderWithTheme(<SimplePreparationPanel {...props} actionCount={1} onAction={onAction}><div>原有配置</div></SimplePreparationPanel>);
    const list = screen.getByRole("region", { name: "已配置行动" });
    Object.defineProperty(list, "scrollHeight", { configurable: true, get: () => list.childElementCount * 100 });
    list.scrollTop = 10;
    await user.click(servantSkill());
    expect(onAction).toHaveBeenCalledOnce();
    expect(list.scrollTop).toBe(10);
    // Persistence completes later; scrolling must use the updated list height.
    rerender(<SimplePreparationPanel {...props} actionCount={2} onAction={onAction}><div>原有配置</div><div>新增技能</div></SimplePreparationPanel>);
    expect(list.scrollTop).toBe(200);
    list.scrollTop = 20;
    rerender(<SimplePreparationPanel {...props} actionCount={2} onAction={onAction}><div>原有配置</div><div>编辑技能</div></SimplePreparationPanel>);
    expect(list.scrollTop).toBe(20);
    rerender(<SimplePreparationPanel {...props} actionCount={1} onAction={onAction}><div>原有配置</div></SimplePreparationPanel>);
    expect(list.scrollTop).toBe(20);
  });

  it("uses the existing enemy choices at the end of the list without a new backend command", async () => {
    const onAction = vi.fn(); const scroll = vi.spyOn(Element.prototype, "scrollIntoView");
    renderWithTheme(<SimplePreparationPanel {...props} onAction={onAction}><div>原有配置</div></SimplePreparationPanel>);
    const user = await openEnemyTarget();
    const list = screen.getByRole("region", { name: "已配置行动" });
    const draft = within(list).getByRole("group", { name: "选择敌方目标" });
    expect(list.lastElementChild).toBe(draft);
    expect(within(list).getByText("原有配置")).toBeInTheDocument();
    expect(within(draft).getAllByRole("button", { name: /敌人/ })).toHaveLength(6);
    expect(scroll.mock.contexts).toContain(draft);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(onAction).not.toHaveBeenCalled();
    await user.click(within(draft).getByRole("button", { name: "敌人 6" }));
    expect(onAction).toHaveBeenCalledWith(expect.objectContaining({ type: "enemyTarget", target: "enemy_6" }));
    expect(screen.queryByRole("group", { name: "选择敌方目标" })).not.toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalled();
    scroll.mockRestore();
  });

  it.each(["button", "escape"])("cancels the inline enemy target draft using %s without adding an action", async cancel => {
    const onAction = vi.fn();
    renderWithTheme(<SimplePreparationPanel {...props} onAction={onAction}>配置</SimplePreparationPanel>);
    const user = await openEnemyTarget();
    if (cancel === "button") await user.click(within(screen.getByRole("group", { name: "选择敌方目标" })).getByRole("button", { name: "撤销添加行动" }));
    else await user.keyboard("{Escape}");
    expect(screen.queryByRole("group", { name: "选择敌方目标" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "令咒 / 敌方目标" })).not.toBeDisabled();
    expect(onAction).not.toHaveBeenCalled();
  });

  it.each(["servant", "equipment"])("allows an unset %s target when auto recognition is disabled", async source => {
    const user = userEvent.setup(); const onAction = vi.fn();
    renderWithTheme(<SimplePreparationPanel {...props} disableAutoSkillTargetRecognition skillTargetStatus={() => "needsTarget"} onAction={onAction}>配置</SimplePreparationPanel>);
    await user.click(source === "servant" ? servantSkill() : screen.getByRole("button", { name: "礼装技能2" }));
    expect(onAction).toHaveBeenCalledWith(expect.objectContaining({ type: source, target: null }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it.each(["servant", "equipment"])("prompts for the required %s target and preserves support identity", async source => {
    const user = userEvent.setup(); const onAction = vi.fn();
    renderWithTheme(<SimplePreparationPanel {...props} skillTargetStatus={() => "needsTarget"} onAction={onAction}>配置</SimplePreparationPanel>);
    const button = () => source === "servant" ? servantSkill() : screen.getByRole("button", { name: "礼装技能2" });
    await user.click(button());
    let dialog = screen.getByRole("dialog", { name: "选择技能目标" });
    expect(within(dialog).getByText("该技能需要指定目标，请选择一名从者。")).toBeInTheDocument();
    expect(onAction).not.toHaveBeenCalled();
    await user.click(within(dialog).getByRole("button", { name: "取消" }));
    expect(onAction).not.toHaveBeenCalled();
    await user.click(button());
    dialog = screen.getByRole("dialog", { name: "选择技能目标" });
    await user.click(within(dialog).getByRole("button", { name: "从者3" }));
    expect(onAction).toHaveBeenCalledOnce();
    expect(onAction).toHaveBeenCalledWith(expect.objectContaining({ type: source, target: "servant_3", targetMemberId: "member-2", targetServantId: 1, targetIsSupport: true }));
  });

  it("retains the skill choice when subsequently requesting a target", async () => {
    const user = userEvent.setup(); const onAction = vi.fn();
    renderWithTheme(<SimplePreparationPanel {...props} skillSelection={() => selection} skillTargetStatus={() => "needsTarget"} onAction={onAction}>配置</SimplePreparationPanel>);
    await user.click(servantSkill());
    await user.click(screen.getByRole("button", { name: "选项乙" }));
    expect(onAction).not.toHaveBeenCalled();
    await user.click(within(screen.getByRole("dialog", { name: "选择技能目标" })).getByRole("button", { name: "从者2" }));
    expect(onAction).toHaveBeenCalledWith(expect.objectContaining({ target: "servant_2", skillSelection: { type: "SelectAddInfo", index: 1, optionCount: 1, label: "选项乙" } }));
  });

  it("keeps four positions and twelve icon-only skills, and toggles a selected target", async () => {
    const user = userEvent.setup(); const onAction = vi.fn();
    renderWithTheme(<SimplePreparationPanel {...props} onAction={onAction}><div>已保存配置</div></SimplePreparationPanel>);
    expect(within(screen.getByRole("group", { name: "简易技能设置" })).getAllByRole("button")).toHaveLength(15);
    expect(screen.queryByRole("button", { name: "御主礼装" })).not.toBeInTheDocument();
    expect(screen.queryByText(/技能目标/)).not.toBeInTheDocument();
    expect(screen.queryByText(/指令序列/)).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "从者3" }));
    expect(screen.getByRole("button", { name: "从者3" })).toHaveAttribute("aria-pressed", "true");
    await user.click(servantSkill());
    expect(onAction).toHaveBeenLastCalledWith(expect.objectContaining({ servantMemberId: "member-0", servantId: 1, servantIsSupport: false, target: "servant_3", targetMemberId: "member-2", targetIsSupport: true }));
    await user.click(screen.getByRole("button", { name: "从者3" }));
    await user.click(servantSkill());
    expect(onAction).toHaveBeenLastCalledWith(expect.objectContaining({ target: null }));
    expect(invoke).not.toHaveBeenCalled();
  });

  it("ignores the selected target for no-target skills", async () => {
    const user = userEvent.setup(); const onAction = vi.fn();
    renderWithTheme(<SimplePreparationPanel {...props} skillTargetStatus={() => "noTarget"} onAction={onAction}>配置</SimplePreparationPanel>);
    await user.click(screen.getByRole("button", { name: "从者3" }));
    await user.click(servantSkill());
    await user.click(screen.getByRole("button", { name: "礼装技能1" }));
    expect(onAction).toHaveBeenCalledTimes(2);
    for (const [action] of onAction.mock.calls) expect(action).toEqual(expect.objectContaining({ target: null, targetMemberId: null, targetServantId: null, targetIsSupport: false }));
  });

  it("keeps the selected target through skill choices and uses member references for order changes", async () => {
    const user = userEvent.setup(); const onAction = vi.fn();
    renderWithTheme(<SimplePreparationPanel {...props} skillSelection={() => selection} skillTargetStatus={() => "needsTarget"} onAction={onAction}>配置</SimplePreparationPanel>);
    await user.click(screen.getByRole("button", { name: "从者3" }));
    await user.click(servantSkill());
    await user.click(screen.getByRole("button", { name: "选项乙" }));
    expect(onAction).toHaveBeenLastCalledWith(expect.objectContaining({ targetMemberId: "member-2", skillSelection: expect.objectContaining({ index: 1 }) }));
    await user.click(screen.getByRole("button", { name: "礼装技能3" }));
    const dialog = screen.getByRole("dialog", { name: "选择换人从者" });
    expect(within(dialog).getByRole("button", { name: "从者4" })).toBeDisabled();
    await user.click(within(dialog).getByRole("button", { name: "从者3" }));
    expect(within(dialog).getByRole("button", { name: "从者1" })).toBeDisabled();
    await user.click(within(dialog).getByRole("button", { name: "从者4" }));
    expect(onAction).toHaveBeenLastCalledWith(expect.objectContaining({ type: "equipment", target: null, orderChange: expect.objectContaining({ front: "servant_3", frontMemberId: "member-2", frontIsSupport: true, back: "servant_4", backMemberId: "member-3" }) }));
  });

  it("uses existing metadata loading state to prevent bypassing skill choices", async () => {
    const user = userEvent.setup(); const onAction = vi.fn();
    const { rerender } = renderWithTheme(<SimplePreparationPanel {...props} isSkillLoading={() => true} onAction={onAction}>配置</SimplePreparationPanel>);
    expect(servantSkill()).toBeDisabled();
    expect(screen.getByRole("button", { name: "礼装技能1" })).toBeEnabled();
    rerender(<SimplePreparationPanel {...props} skillSelection={() => selection} onAction={onAction}>配置</SimplePreparationPanel>);
    await user.click(servantSkill());
    expect(screen.getByRole("dialog", { name: "选择技能选项" })).toBeInTheDocument();
    expect(onAction).not.toHaveBeenCalled();
  });

  it("expands and restores the same saved list without changing actions", async () => {
    const user = userEvent.setup(); const onAction = vi.fn();
    renderWithTheme(<SimplePreparationPanel {...props} onAction={onAction}><input aria-label="配置草稿" defaultValue="保留" /></SimplePreparationPanel>);
    const list = screen.getByRole("region", { name: "已配置行动" });
    const palette = screen.getByRole("group", { name: "简易技能设置" });
    await user.click(screen.getByRole("button", { name: "展开配置预览" }));
    expect(palette).toHaveAttribute("inert");
    expect(screen.getByRole("region", { name: "已配置行动" })).toBe(list);
    expect(screen.getByLabelText("配置草稿")).toHaveValue("保留");
    await user.click(screen.getByRole("button", { name: "恢复技能设置" }));
    expect(palette).not.toHaveAttribute("inert");
    expect(onAction).not.toHaveBeenCalled();
  });

  it("groups command seals and enemy target, and requests a command seal target", async () => {
    const user = userEvent.setup(); const onAction = vi.fn();
    renderWithTheme(<SimplePreparationPanel {...props} onAction={onAction}>配置</SimplePreparationPanel>);
    await user.click(screen.getByRole("button", { name: "令咒 / 敌方目标" }));
    expect(screen.getAllByRole("menuitem")).toHaveLength(3);
    await user.click(screen.getByRole("menuitem", { name: "灵基修复" }));
    await user.click(within(screen.getByRole("dialog", { name: "选择令咒目标" })).getByRole("button", { name: "从者2" }));
    expect(onAction).toHaveBeenCalledWith(expect.objectContaining({ type: "commandSpell", spell: "restore", target: "servant_2", targetMemberId: "member-1" }));
  });

  it("discards a pending draft and selection when the lineup or layout changes", async () => {
    const user = userEvent.setup(); const onAction = vi.fn();
    const { rerender } = renderWithTheme(<SimplePreparationPanel {...props} onAction={onAction}>配置</SimplePreparationPanel>);
    await openEnemyTarget();
    rerender(<SimplePreparationPanel {...props} members={[...members].reverse()} onAction={onAction}>配置</SimplePreparationPanel>);
    expect(screen.queryByRole("group", { name: "选择敌方目标" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "从者6" }));
    rerender(<SimplePreparationPanel {...props} enabled={false} onAction={onAction}>配置</SimplePreparationPanel>);
    rerender(<SimplePreparationPanel {...props} onAction={onAction}>配置</SimplePreparationPanel>);
    expect(screen.getByRole("button", { name: "从者3" })).toHaveAttribute("aria-pressed", "false");
    expect(onAction).not.toHaveBeenCalled();
  });
});
