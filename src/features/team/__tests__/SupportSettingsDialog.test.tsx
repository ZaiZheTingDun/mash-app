import { useState } from "react";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { SupportSettingsDialog } from "../SupportSettingsDialog";
import type { Project } from "../../../types/project";

const PROJECT: Project = {
  id: "support-drawer",
  name: "测试队伍",
  supportServantId: null,
  slots: [],
  repeatMission: false,
  supportServantLevelMin: 100,
  supportStarMapScoreMin: 40,
  supportNoblePhantasmLevelMin: 3,
  supportSkillLevelMins: [5, null, 10],
  supportAppendSkillLevelMins: [null, 10, null, null, null],
};

function mount(project = PROJECT) {
  const onConfirm = vi.fn();
  const onCraftEssenceOpen = vi.fn();
  function Harness() {
    const [open, setOpen] = useState(true);
    return <>
      <button onClick={() => setOpen(true)}>重新打开设置</button>
      {open && <SupportSettingsDialog open={open} project={project} servant={null} portraitSrc={null} slotNumber={3}
        craftEssenceGroups={[{ count: 1, images: [] }, { count: 1, images: [] }, { count: 0, images: [] }, { count: 2, images: [] }]}
        onCraftEssenceOpen={onCraftEssenceOpen} onOpenChange={setOpen} onConfirm={onConfirm} />}
    </>;
  }
  renderWithTheme(<Harness />);
  return { onConfirm, onCraftEssenceOpen, user: userEvent.setup() };
}

describe("SupportSettingsDialog", () => {
  it.each(["close", "escape"])("discards unapplied drafts on %s", async (method) => {
    const { user, onConfirm } = mount();
    await user.clear(screen.getByRole("spinbutton", { name: "从者等级" }));
    await user.type(screen.getByRole("spinbutton", { name: "从者等级" }), "120");
    await user.click(screen.getByRole("switch", { name: "冠位从者" }));
    if (method === "close") await user.click(screen.getByRole("button", { name: "关闭助战筛选设置" }));
    else await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(onConfirm).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "重新打开设置" }));
    expect(screen.getByRole("spinbutton", { name: "从者等级" })).toHaveValue(100);
    expect(screen.getByRole("switch", { name: "冠位从者" })).not.toBeChecked();
  });

  it("applies grand mode and its conditional threshold together", async () => {
    const { user, onConfirm } = mount();
    expect(screen.queryByRole("spinbutton", { name: "冠位星图分值" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("switch", { name: "冠位从者" }));
    await user.type(screen.getByRole("spinbutton", { name: "冠位星图分值" }), "16");
    expect(onConfirm).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "应用并关闭" }));
    expect(onConfirm).toHaveBeenCalledWith(expect.objectContaining({ grandMode: true, grandStarMapScore: 16, skillLevels: [5, null, 10] }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("steps skill drafts between unrestricted and level 1 without exceeding level 10", async () => {
    const { user, onConfirm } = mount();
    expect(screen.getByRole("button", { name: "降低持有技能 2等级" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "提高持有技能 3等级" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "提高持有技能 2等级" }));
    expect(screen.getByRole("button", { name: "持有技能 2" })).toHaveTextContent("1");
    await user.click(screen.getByRole("button", { name: "降低持有技能 2等级" }));
    expect(screen.getByRole("button", { name: "持有技能 2" })).toHaveTextContent("—");
    await user.click(screen.getByRole("button", { name: "提高追加技能 1等级" }));
    await user.click(screen.getByRole("button", { name: "应用并关闭" }));
    expect(onConfirm).toHaveBeenCalledWith(expect.objectContaining({ skillLevels: [5, null, 10], appendSkillLevels: [1, 10, null, null, null] }));
  });

  it("clears filter drafts only when applied", async () => {
    const { user, onConfirm } = mount({ ...PROJECT, supportGrandMode: true, supportGrandStarMapScoreMin: 16 });
    await user.click(screen.getByRole("button", { name: "清空配置" }));
    expect(onConfirm).not.toHaveBeenCalled();
    expect(screen.getByRole("spinbutton", { name: "从者等级" })).toHaveValue(null);
    await user.click(screen.getByRole("button", { name: "应用并关闭" }));
    expect(onConfirm).toHaveBeenCalledWith({ grandMode: true, servantLevel: null, starMapScore: null, grandStarMapScore: null, npLevel: null, skillLevels: [null, null, null], appendSkillLevels: [null, null, null, null, null] });
  });

  it("opens the selected grand CE group from the combined summary", async () => {
    const { user, onCraftEssenceOpen, onConfirm } = mount({ ...PROJECT, supportGrandMode: true });
    expect(screen.getByRole("button", { name: "配置冠位助战礼装" })).toHaveTextContent("2/3 已配置");
    await user.click(screen.getByRole("button", { name: "配置冠位助战礼装" }));
    await user.click(screen.getByRole("menuitem", { name: "羁绊礼装" }));
    expect(onCraftEssenceOpen).toHaveBeenCalledWith(1, true);
    expect(onConfirm).not.toHaveBeenCalled();
  });
});
