import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { SimplePreparationPanel } from "../SimplePreparationPanel";
import type { Servant } from "../../../types/servant";
import type { PartyMember } from "../../team/partyServants";

const mash: Servant = { id: 1, variantKey: "1:3", name_cn: "玛修", name_jp: "", name_en: "", class: "shielder", rarity: 5 };
const members: PartyMember[] = [
  { memberId: "owned-mash", servant: mash, isSupport: false },
  { memberId: "other", servant: { ...mash, id: 2, variantKey: "2:1", name_cn: "从者乙" }, isSupport: false },
  { memberId: "support-mash", servant: { ...mash, name_cn: "助战玛修" }, isSupport: true },
];
const props = {
  enabled: true, actionCount: 0, members, faces: {}, skillIcons: {}, mysticCode: null,
  gender: "female" as const, disableAutoSkillTargetRecognition: false,
  skillTargetStatus: () => "mixed" as const, skillSelection: () => null, isSkillLoading: () => false,
};
const skillButton = (skill = "技能 2") => within(screen.getByRole("group", { name: "前排 1 技能" })).getByRole("button", { name: skill });
const whiteWall = () => screen.getByRole("button", { name: /时为朦胧的白垩之壁 A/ });
const purpleBullet = () => screen.getByRole("button", { name: /测定时间的紫弹之薪 B/ });

describe("temporary Mash skill two choice", () => {
  it("offers both forms on hover and click without prematurely adding a skill", async () => {
    const user = userEvent.setup(); const onAction = vi.fn();
    renderWithTheme(<SimplePreparationPanel {...props} onAction={onAction}>配置</SimplePreparationPanel>);
    await user.hover(skillButton());
    const chooser = screen.getByRole("dialog", { name: "选择玛修二技能形态" });
    expect(within(chooser).getAllByRole("button")).toHaveLength(2);
    expect(chooser.textContent).toBe("");
    expect(whiteWall()).toBeInTheDocument();
    expect(purpleBullet()).toBeInTheDocument();
    expect(onAction).not.toHaveBeenCalled();
    await user.click(skillButton());
    expect(whiteWall()).toBeInTheDocument();
    expect(onAction).not.toHaveBeenCalled();
  });

  it.each([false, true])("applies required targeting to the white-wall form (preselected=%s)", async selected => {
    const user = userEvent.setup(); const onAction = vi.fn();
    renderWithTheme(<SimplePreparationPanel {...props} onAction={onAction}>配置</SimplePreparationPanel>);
    if (selected) await user.click(screen.getByRole("button", { name: "助战玛修" }));
    await user.hover(skillButton());
    await user.click(whiteWall());
    if (!selected) {
      expect(onAction).not.toHaveBeenCalled();
      const dialog = screen.getByRole("dialog", { name: "选择技能目标" });
      expect(within(dialog).getByText("该技能需要指定目标，请选择一名从者。")).toBeInTheDocument();
      await user.click(within(dialog).getByRole("button", { name: "助战玛修" }));
    }
    expect(onAction).toHaveBeenCalledOnce();
    expect(onAction).toHaveBeenCalledWith(expect.objectContaining({ type: "servant", servant: "servant_1", servantMemberId: "owned-mash", servantId: 1, servantIsSupport: false,
      skill: "skill_2", skillFormId: 2550, skillSelection: null, target: "servant_3", targetMemberId: "support-mash", targetServantId: 1, targetIsSupport: true }));
    expect(screen.queryByRole("dialog", { name: "选择玛修二技能形态" })).not.toBeInTheDocument();
  });

  it.each([false, true])("keeps the purple-bullet form untargeted even with a selected servant (recognition disabled=%s)", async disabled => {
    const user = userEvent.setup(); const onAction = vi.fn();
    renderWithTheme(<SimplePreparationPanel {...props} disableAutoSkillTargetRecognition={disabled} onAction={onAction}>配置</SimplePreparationPanel>);
    await user.click(screen.getByRole("button", { name: "助战玛修" }));
    await user.hover(skillButton());
    await user.click(purpleBullet());
    expect(onAction).toHaveBeenCalledOnce();
    expect(onAction).toHaveBeenCalledWith(expect.objectContaining({ skill: "skill_2", skillFormId: 2477450, target: null, targetMemberId: null, targetServantId: null, targetIsSupport: false }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("does not block an unset white-wall target when automatic recognition is disabled", async () => {
    const user = userEvent.setup(); const onAction = vi.fn();
    renderWithTheme(<SimplePreparationPanel {...props} disableAutoSkillTargetRecognition onAction={onAction}>配置</SimplePreparationPanel>);
    await user.hover(skillButton());
    await user.click(whiteWall());
    expect(onAction).toHaveBeenCalledWith(expect.objectContaining({ skill: "skill_2", skillFormId: 2550, target: null }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it.each(["1:1", "1:2", "2:3"])("leaves other battle variants unchanged (%s)", async variantKey => {
    const user = userEvent.setup(); const onAction = vi.fn();
    const first = { ...members[0], servant: { ...mash, id: variantKey === "2:3" ? 2 : 1, variantKey } };
    renderWithTheme(<SimplePreparationPanel {...props} members={[first, ...members.slice(1)]} onAction={onAction}>配置</SimplePreparationPanel>);
    await user.hover(skillButton());
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await user.click(skillButton());
    expect(onAction).toHaveBeenCalledWith(expect.objectContaining({ skill: "skill_2" }));
  });

  it("keeps other skills unchanged and prevents opening disabled form choices", async () => {
    const user = userEvent.setup(); const onAction = vi.fn();
    const { rerender } = renderWithTheme(<SimplePreparationPanel {...props} onAction={onAction}>配置</SimplePreparationPanel>);
    await user.hover(skillButton("技能 1"));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await user.click(skillButton("技能 1"));
    expect(onAction).toHaveBeenCalledWith(expect.objectContaining({ skill: "skill_1" }));
    rerender(<SimplePreparationPanel {...props} disabled onAction={onAction}>配置</SimplePreparationPanel>);
    await user.hover(skillButton());
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("supports keyboard opening and Escape cancellation without adding an action", async () => {
    const user = userEvent.setup(); const onAction = vi.fn();
    renderWithTheme(<SimplePreparationPanel {...props} onAction={onAction}>配置</SimplePreparationPanel>);
    skillButton().focus();
    await user.keyboard("{ArrowDown}");
    expect(whiteWall()).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(onAction).not.toHaveBeenCalled();
  });

  it("lets the pointer enter the choices and closes when it leaves", async () => {
    const user = userEvent.setup(); const onAction = vi.fn();
    renderWithTheme(<SimplePreparationPanel {...props} onAction={onAction}>配置</SimplePreparationPanel>);
    await user.hover(skillButton());
    await user.hover(whiteWall());
    expect(purpleBullet()).toBeInTheDocument();
    await user.unhover(whiteWall());
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(onAction).not.toHaveBeenCalled();
  });
});
