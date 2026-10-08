import { describe, expect, it, vi } from "vitest";
import { fireEvent, screen } from "@testing-library/react";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { FormationLevels } from "../FormationLevels";
import type { SupportAppendSkillLevelMins, SupportSkillLevelMins } from "../../../types/project";

describe("FormationLevels", () => {
  it("shows the settings entry instead of a summary when nothing is configured", () => {
    const onOpen = vi.fn();
    const { container } = renderWithTheme(<FormationLevels support onOpen={onOpen} />);
    expect(container.querySelector(".support-requirement-summary")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "助战筛选设置" }));
    expect(onOpen).toHaveBeenCalledOnce();
  });

  it.each([
    { props: { level: 100 }, row: ".formation-levels-stats", label: "从者至少 100 级" },
    { props: { np: 2 }, row: ".formation-levels-stats", label: "宝具至少 2 级" },
    { props: { starMapScore: 0 }, row: ".formation-levels-scores", label: "星图至少 0" },
    { props: { grandStarMapScore: 16 }, row: ".formation-levels-scores", label: "冠位至少 16" },
    { props: { skills: [null, 10, null] as SupportSkillLevelMins }, row: ".formation-levels-skills", label: "持有技能 2 至少 10 级" },
    { props: { append: [null, null, null, null, 10] as SupportAppendSkillLevelMins }, row: ".formation-levels-append", label: "追加技能 5 至少 10 级" },
  ])("shows only the configured row for $label", ({ props, row, label }) => {
    const { container } = renderWithTheme(<FormationLevels support onOpen={vi.fn()} {...props} />);
    expect(screen.getByLabelText(label)).toBeInTheDocument();
    for (const selector of [".formation-levels-stats", ".formation-levels-scores", ".formation-levels-skills", ".formation-levels-append"]) {
      if (selector === row) expect(container.querySelector(selector)).not.toBeNull();
      else expect(container.querySelector(selector)).toBeNull();
    }
    expect(screen.queryByLabelText("从者任意等级")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("宝具任意等级")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "助战筛选设置" })).not.toBeInTheDocument();
  });

  it("shows all four rows for a full configuration and restores the entry when cleared", () => {
    const onOpen = vi.fn();
    const { container, rerender } = renderWithTheme(<FormationLevels support onOpen={onOpen}
      level={120} np={5} starMapScore={62} grandStarMapScore={16}
      skills={[10, 10, 10]} append={[10, 10, 10, 10, 10]} />);
    expect(container.querySelector(".formation-levels")?.children).toHaveLength(4);
    expect(container.querySelector(".formation-levels-stats")).toHaveTextContent("Lv.120宝具 5");
    expect(container.querySelector(".formation-levels-scores")).toHaveTextContent("星图 62冠位 16");
    expect(container.querySelectorAll(".formation-levels-skills .skill")).toHaveLength(3);
    expect(container.querySelectorAll(".formation-levels-append .append")).toHaveLength(5);
    expect(screen.queryByRole("button", { name: "助战筛选设置" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "编辑助战筛选设置" }));
    expect(onOpen).toHaveBeenCalledOnce();
    rerender(<FormationLevels support onOpen={onOpen} />);
    expect(container.querySelector(".support-requirement-summary")).toBeNull();
    expect(screen.getByRole("button", { name: "助战筛选设置" })).toBeInTheDocument();
  });
});
