import { fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { renderWithTheme } from "../../../test/renderWithTheme";
import type { MysticCode } from "../../../types/mysticCode";
import { HomePage } from "../HomePage";

const codes: MysticCode[] = [470, 20].map((id) => ({
  id,
  name: `御主 ${id}`,
  itemMalePath: null,
  itemFemalePath: null,
  masterFigureMalePath: `/assets/mystic-codes/${id}/master-figure-male.png`,
  masterFigureFemalePath: `/assets/mystic-codes/${id}/master-figure-female.png`,
  masterFaceMalePath: `/assets/mystic-codes/${id}/master-face-male.png`,
  masterFaceFemalePath: `/assets/mystic-codes/${id}/master-face-female.png`,
  skills: [],
}));

function renderHome(overrides: Partial<React.ComponentProps<typeof HomePage>> = {}) {
  const onSelectFigure = vi.fn(async () => {});
  renderWithTheme(
    <HomePage
      mysticCodes={codes}
      figureId={470}
      gender="female"
      showSummon
      showEnhancement
      showRankUpQuest
      onSelectFigure={onSelectFigure}
      onOpenTeam={vi.fn()}
      onOpenSummon={vi.fn()}
      onOpenCraftEssenceEnhancement={vi.fn()}
      onOpenRankUpQuest={vi.fn()}
      {...overrides}
    />,
  );
  return onSelectFigure;
}

describe("HomePage", () => {
  it("returns from the battle menu with an icon-only heading button", async () => {
    renderHome();
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "战斗" }));
    expect(screen.getByRole("heading", { name: "战斗" })).toBeInTheDocument();
    const back = screen.getByRole("button", { name: "返回" });
    expect(back).not.toHaveTextContent("返回");
    await user.click(back);
    expect(screen.getByRole("heading", { name: "选择任务" })).toBeInTheDocument();
  });

  it("opens the resource figure picker on right click and saves a separate selection", async () => {
    const onSelectFigure = renderHome();
    const figure = screen.getByRole("img", { name: "御主 470" });
    await userEvent.setup().click(figure);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    fireEvent.contextMenu(figure);
    expect(screen.getByText("只更换主页立绘，不影响队伍的御主礼装。")).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole("button", { name: "御主 20" }));
    await waitFor(() => expect(onSelectFigure).toHaveBeenCalledWith(20));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("ignores right clicks on transparent parts of the figure image", () => {
    const getContext = vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({
      drawImage: vi.fn(),
      getImageData: () => ({
        width: 2,
        height: 2,
        data: new Uint8ClampedArray([0, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
      }),
    } as unknown as CanvasRenderingContext2D);
    try {
      renderHome();
      const figure = screen.getByRole("img", { name: "御主 470" });
      Object.defineProperties(figure, {
        naturalWidth: { value: 2 },
        naturalHeight: { value: 2 },
      });
      vi.spyOn(figure, "getBoundingClientRect").mockReturnValue({
        left: 0, top: 0, width: 20, height: 20,
      } as DOMRect);
      fireEvent.load(figure);
      expect(figure).toHaveStyle({ clipPath: "inset(0% 50% 50% 0%)" });

      fireEvent.contextMenu(figure, { clientX: 15, clientY: 5 });
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
      fireEvent.contextMenu(figure, { clientX: 5, clientY: 5 });
      expect(screen.getByRole("dialog", { name: "更换御主立绘" })).toBeInTheDocument();
    } finally {
      getContext.mockRestore();
    }
  });

  it("uses the current display gender and keeps navigation available without assets", () => {
    renderHome({ mysticCodes: [], gender: "male" });
    expect(screen.getByText("未找到御主立绘，请更新资源包")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "战斗" })).toBeEnabled();
  });

  it("shows the matching male figure when the display gender is male", () => {
    renderHome({ gender: "male" });
    expect(screen.getByRole("img", { name: "御主 470" })).toHaveAttribute(
      "src",
      expect.stringContaining("master-figure-male.png"),
    );
  });
});
