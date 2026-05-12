import { describe, expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithTheme } from "../../test/renderWithTheme";
import { DebugPage } from "../DebugPage";
import type { CraftEssence } from "../../types/craftEssence";
import type { Servant } from "../../types/servant";

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  WebviewWindow: class {
    static getByLabel = vi.fn(async () => null);
    once = vi.fn();
  },
}));

const SERVANTS: Servant[] = [
  {
    id: 150,
    variantKey: "150",
    name_cn: "梅林",
    name_jp: "マーリン",
    name_en: "Merlin",
    class: "Caster",
    rarity: 5,
    noblePhantasmName: "永久关闭的理想乡",
  },
  {
    id: 284,
    variantKey: "284",
    name_cn: "阿尔托莉雅·卡斯特",
    name_jp: "アルトリア・キャスター",
    name_en: "Altria Caster",
    class: "Caster",
    rarity: 5,
    noblePhantasmName: "真圆集",
  },
];

const CRAFT_ESSENCES: CraftEssence[] = [
  { id: 1, name: "万华镜" },
  { id: 2, name: "黑之圣杯" },
];

describe("DebugPage", () => {
  it("selects command-card candidates by searchable Chinese servant name", async () => {
    const user = userEvent.setup();
    renderWithTheme(
      <DebugPage
        onBack={() => {}}
        servants={SERVANTS}
        craftEssences={CRAFT_ESSENCES}
        defaultCardServantIds={[]}
      />
    );

    await user.click(screen.getByRole("button", { name: "添加从者" }));
    await user.type(screen.getByPlaceholderText("搜索从者名称..."), "梅");
    await user.click(screen.getByRole("option", { name: /梅林/ }));

    expect(screen.getByRole("button", { name: "梅林" })).toBeInTheDocument();
    expect(screen.queryByPlaceholderText(/候选从者 id/)).not.toBeInTheDocument();
  });

  it("selects the optional support craft essence by searchable name", async () => {
    const user = userEvent.setup();
    renderWithTheme(
      <DebugPage
        onBack={() => {}}
        servants={SERVANTS}
        craftEssences={CRAFT_ESSENCES}
        defaultCardServantIds={[]}
      />
    );

    await user.click(screen.getByRole("button", { name: "选择礼装 (可选)" }));
    await user.type(screen.getByPlaceholderText("搜索礼装名称..."), "黑");
    await user.click(screen.getByRole("option", { name: /黑之圣杯/ }));

    expect(screen.getByRole("button", { name: "黑之圣杯" })).toBeInTheDocument();
    expect(screen.queryByPlaceholderText(/礼装 id/)).not.toBeInTheDocument();
  });
});
