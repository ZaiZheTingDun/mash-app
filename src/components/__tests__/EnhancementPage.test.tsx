import { describe, expect, it, vi } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithTheme } from "../../test/renderWithTheme";
import { EnhancementPage } from "../EnhancementPage";
import type { Servant } from "../../types/servant";

const SERVANTS: Servant[] = [
  {
    id: 100100,
    variantKey: "100100:1",
    faceId: 1,
    name_cn: "阿尔托莉雅·Caster",
    name_jp: "アルトリア・キャスター",
    name_en: "Altria Caster",
    class: "caster",
    rarity: 5,
    noblePhantasmName: "きみをいだく希望の星",
  },
];

describe("EnhancementPage", () => {
  it("starts enhancement automation with the selected servant", async () => {
    const user = userEvent.setup();
    renderWithTheme(<EnhancementPage servants={SERVANTS} onBack={() => {}} />);

    await user.click(screen.getByRole("button", { name: "开始" }));

    expect(invoke).toHaveBeenCalledWith("start_enhancement_automation", {
      config: {
        targetServantId: 100100,
        targetServantVariantKey: "100100:1",
      },
    });
  });

  it("uses the shared servant dialog for target selection", async () => {
    const user = userEvent.setup();
    const otherServant: Servant = {
      id: 200200,
      variantKey: "200200:1",
      faceId: 2,
      name_cn: "梅林",
      name_jp: "マーリン",
      name_en: "Merlin",
      class: "Caster",
      rarity: 5,
      noblePhantasmName: "永久关闭的理想乡",
    };
    renderWithTheme(
      <EnhancementPage servants={[SERVANTS[0], otherServant]} onBack={() => {}} />
    );

    await user.click(screen.getByRole("button", { name: /阿尔托莉雅·Caster/ }));
    await user.click(await screen.findByText("梅林"));
    await user.click(screen.getByRole("button", { name: "开始" }));

    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("start_enhancement_automation", {
        config: {
          targetServantId: 200200,
          targetServantVariantKey: "200200:1",
        },
      });
    });
  });

  it("stops enhancement automation when stop is clicked", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "start_enhancement_automation") return null;
      return null;
    });
    const user = userEvent.setup();
    renderWithTheme(<EnhancementPage servants={SERVANTS} onBack={() => {}} />);

    await user.click(screen.getByRole("button", { name: "开始" }));
    await user.click(screen.getByRole("button", { name: "停止" }));

    expect(invoke).toHaveBeenCalledWith("stop_enhancement_automation");
  });
});
