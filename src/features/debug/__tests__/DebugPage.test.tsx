import { describe, expect, it, vi } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { DebugPage } from "../DebugPage";
import type { CraftEssence } from "../../../types/craftEssence";
import type { Servant } from "../../../types/servant";
import { invoke } from "@tauri-apps/api/core";

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
  function renderDebugPage() {
    renderWithTheme(
      <DebugPage
        onBack={() => {}}
        servants={SERVANTS}
        craftEssences={CRAFT_ESSENCES}
        defaultCardServantIds={[]}
      />
    );
  }

  function mockDebugPageBootstrap(
    handler: (cmd: string) => Promise<unknown> | unknown
  ) {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case "debug_get_cv_config":
          return { screens: {} };
        case "debug_list_templates":
        case "debug_list_servant_assets":
          return [];
        case "debug_get_runner_coordinates":
          return { groups: [] };
        default:
          return handler(cmd);
      }
    });
  }

  it("renders live scrcpy frames as data URLs for dynamic preview", async () => {
    const user = userEvent.setup();
    mockDebugPageBootstrap((cmd) => {
      if (cmd === "debug_stream_frame") {
        return {
          jpegBase64: "ZmFrZS1qcGVn",
          width: 1920,
          height: 1080,
          screen: null,
          score: null,
          timestampMs: 1,
        };
      }
      return null;
    });

    renderDebugPage();

    await user.click(screen.getByRole("button", { name: "动态读取画面" }));
    const img = await screen.findByRole("img", { name: "screenshot" });
    expect(img).toHaveAttribute("src", "data:image/jpeg;base64,ZmFrZS1qcGVn");
  });

  it("stops live frame polling while dynamic preview is paused", async () => {
    const user = userEvent.setup();
    let frameIndex = 0;
    mockDebugPageBootstrap((cmd) => {
      if (cmd === "debug_stream_frame") {
        frameIndex += 1;
        return {
          jpegBase64: `ZnJhbWUt${frameIndex}`,
          width: 1920,
          height: 1080,
          screen: null,
          score: null,
          timestampMs: frameIndex,
        };
      }
      return null;
    });

    renderDebugPage();

    await user.click(screen.getByRole("button", { name: "动态读取画面" }));
    await screen.findByRole("img", { name: "screenshot" });

    await user.click(screen.getByRole("button", { name: "暂停" }));
    const callsAfterPause = vi
      .mocked(invoke)
      .mock.calls.filter(([cmd]) => cmd === "debug_stream_frame").length;

    await new Promise((resolve) => window.setTimeout(resolve, 250));

    expect(
      vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "debug_stream_frame")
    ).toHaveLength(callsAfterPause);

    await user.click(screen.getByRole("button", { name: "继续" }));
    await waitFor(() =>
      expect(
        vi.mocked(invoke).mock.calls.filter(
          ([cmd]) => cmd === "debug_stream_frame"
        ).length
      ).toBeGreaterThan(callsAfterPause)
    );
  });

  it("runs dynamic NP detection through the live command", async () => {
    const user = userEvent.setup();
    mockDebugPageBootstrap((cmd) => {
      if (cmd === "debug_read_noble_phantasm_gauges_live") {
        return [
          {
            slot: 1,
            ready: true,
            edgeFrac: 0,
            stdBgr: 0,
            readySource: "glow",
            gaugeDigitCount: 3,
            cardReady: false,
            gaugeRegion: { x: 0.429, y: 0.913, w: 0.0297, h: 0.0278 },
            npGlowRegion: { x: 0.482, y: 0.94, w: 0.00625, h: 0.01111 },
            npGlowScore: 0.62,
            npGlowReady: true,
          },
        ];
      }
      return null;
    });

    renderDebugPage();

    await user.click(screen.getByRole("button", { name: "动态检测宝具" }));

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith(
        "debug_read_noble_phantasm_gauges_live"
      )
    );
    expect(await screen.findByText(/宝具卡 not ready/)).toBeInTheDocument();
    expect(screen.getByText(/宝具条端帽 0\.620/)).toBeInTheDocument();
  });

  it("selects command-card candidates by searchable Chinese servant name", async () => {
    const user = userEvent.setup();
    renderDebugPage();

    await user.click(screen.getByRole("button", { name: "添加从者" }));
    await user.type(screen.getByPlaceholderText("搜索从者名称..."), "梅");
    await user.click(screen.getByRole("option", { name: /梅林/ }));

    expect(screen.getByRole("button", { name: "梅林" })).toBeInTheDocument();
    expect(screen.queryByPlaceholderText(/候选从者 id/)).not.toBeInTheDocument();
  });

  it("selects the optional support craft essence by searchable name", async () => {
    const user = userEvent.setup();
    renderDebugPage();

    await user.click(screen.getByRole("button", { name: "选择礼装 (可选)" }));
    await user.type(screen.getByPlaceholderText("搜索礼装名称..."), "黑");
    await user.click(screen.getByRole("option", { name: /黑之圣杯/ }));

    expect(screen.getByRole("button", { name: "黑之圣杯" })).toBeInTheDocument();
    expect(screen.queryByPlaceholderText(/礼装 id/)).not.toBeInTheDocument();
  });

  it("includes the support marker in command-card debug summaries", async () => {
    const user = userEvent.setup();
    mockDebugPageBootstrap((cmd) => {
      if (cmd === "debug_capture") {
        return {
          imagePath: "/tmp/debug.png",
          screen: "Attack",
          score: 0.95,
          screenSize: { w: 1080, h: 1920 },
        };
      }
      if (cmd === "debug_find_command_cards") {
        return [
          {
            slot: 1,
            x: 0.3,
            y: 0.6,
            cardRegion: { x: 0.2, y: 0.46, w: 0.2, h: 0.4 },
            faceRegion: { x: 0.24, y: 0.55, w: 0.1, h: 0.14 },
            suit: "a",
            servantId: 150,
            ascension: 1,
            faceScore: 0.88,
            isSupport: true,
          },
        ];
      }
      return null;
    });

    renderDebugPage();

    await user.click(screen.getByRole("button", { name: "截取画面" }));
    await user.click(screen.getByRole("button", { name: "识别指令卡" }));

    expect(await screen.findByText(/C2:a 150@1\(0.88\) \[S\]/)).toBeInTheDocument();
  });
});
