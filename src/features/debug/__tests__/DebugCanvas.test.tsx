import { describe, it, expect } from "vitest";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { DebugCanvas, type DebugCanvasState } from "../DebugCanvas";

/**
 * Tiny helper — most fields are unused per test, so default everything
 * to "off" and the individual specs override only what they care about.
 */
function makeState(overrides: Partial<DebugCanvasState> = {}): DebugCanvasState {
  return {
    imageSrc: null,
    probes: [],
    commandCards: [],
    npGaugeSlots: [],
    battleScene: null,
    attackButton: null,
    enhancementServantResult: null,
    supportResult: null,
    coordinates: null,
    showDigitRecognitionRegions: false,
    showCoordOverlay: false,
    visibleCoordGroups: [],
    ...overrides,
  };
}

describe("DebugCanvas", () => {
  it("shows the placeholder text when no screenshot has been captured", () => {
    renderWithTheme(<DebugCanvas {...makeState()} />);
    expect(
      document.body.textContent?.includes("点击 截取画面 开始")
    ).toBe(true);
  });

  it("supports a custom placeholder for the popout view", () => {
    renderWithTheme(
      <DebugCanvas
        {...makeState()}
        placeholder="等待主窗口截取画面…"
      />
    );
    expect(
      document.body.textContent?.includes("等待主窗口截取画面…")
    ).toBe(true);
  });

  it("renders the screenshot image and probe overlays when a probe is found", () => {
    const { container } = renderWithTheme(
      <DebugCanvas
        {...makeState({
          imageSrc: "tauri://localhost/fake.png?t=1",
          probes: [
            {
              label: "battle_anchor",
              threshold: 0.8,
              timestamp: "12:00:00",
              match: {
                found: true,
                x: 0.5,
                y: 0.5,
                score: 0.93,
                region: { x: 0.1, y: 0.2, w: 0.3, h: 0.4 },
              },
            },
          ],
        })}
      />
    );
    const img = container.querySelector("img.debug-canvas-img");
    expect(img).not.toBeNull();
    expect(img?.getAttribute("src")).toBe("tauri://localhost/fake.png?t=1");
    expect(
      container.querySelectorAll(".debug-overlay-box").length
    ).toBeGreaterThanOrEqual(1);
    expect(
      container.textContent?.includes("battle_anchor")
    ).toBe(true);
  });

  it("renders the attack button overlay group when results are present", () => {
    const { container } = renderWithTheme(
      <DebugCanvas
        {...makeState({
          imageSrc: "tauri://localhost/fake.png?t=2",
          attackButton: {
            template: "button_attack",
            region: { x: 0.799, y: 0.746, w: 0.177, h: 0.195 },
            threshold: 0.8,
            tapPoint: { x: 0.887, y: 0.844 },
            found: true,
            score: 0.91,
            matchX: 0.88,
            matchY: 0.84,
            matchRegion: { x: 0.85, y: 0.82, w: 0.07, h: 0.05 },
          },
        })}
      />
    );
    expect(container.textContent?.includes("攻击按钮")).toBe(true);
    expect(
      container.querySelectorAll(".debug-coord-dot").length
    ).toBeGreaterThanOrEqual(1);
  });

  it("hides coord overlays when showCoordOverlay is false", () => {
    const { container } = renderWithTheme(
      <DebugCanvas
        {...makeState({
          imageSrc: "tauri://localhost/fake.png?t=3",
          showCoordOverlay: false,
          visibleCoordGroups: ["battle"],
          coordinates: {
            groups: [
              {
                id: "battle",
                label: "战斗",
                points: [{ label: "tap", point: { x: 0.5, y: 0.5 } }],
                regions: [],
              },
            ],
          },
        })}
      />
    );
    expect(container.querySelector(".debug-coord-region")).toBeNull();
    // Coord-only points (no attack button etc.) should also be hidden.
    expect(container.querySelectorAll(".debug-coord-dot").length).toBe(0);
  });

  it("renders NP gauge and glow overlays from recognition results", () => {
    const { container } = renderWithTheme(
      <DebugCanvas
        {...makeState({
          imageSrc: "tauri://localhost/fake.png?t=4",
          npGaugeSlots: [
            {
              slot: 0,
              cardRegion: { x: 0.241, y: 0.097, w: 0.187, h: 0.396 },
              ready: true,
              edgeFrac: 0,
              stdBgr: 0,
              readySource: "glow",
              gaugeDigitCount: 3,
              cardReady: true,
              gaugeRegion: { x: 0.182, y: 0.913, w: 0.0297, h: 0.0278 },
              npGlowRegion: {
                x: 0.23529166666666668,
                y: 0.9398148148148148,
                w: 0.00625,
                h: 0.011111111111111112,
              },
              npGlowScore: 0.8,
              npGlowReady: true,
            },
          ],
        })}
      />
    );

    expect(container.querySelectorAll(".debug-overlay-np-gauge").length).toBe(1);
    expect(container.querySelectorAll(".debug-overlay-np-glow-slot.ready").length).toBe(1);
    expect(
      container.textContent?.includes(
        "NP1 · 卡 ready · 条 ready · 端帽 0.800 · gauge 3位"
      )
    ).toBe(true);
  });

  it("toggles configured battle digit and NP sequence regions", () => {
    const { container, rerender } = renderWithTheme(
      <DebugCanvas
        {...makeState({
          imageSrc: "tauri://localhost/fake.png?t=digits",
          showDigitRecognitionRegions: false,
        })}
      />
    );

    expect(container.querySelectorAll(".debug-overlay-digit-source")).toHaveLength(0);

    rerender(
      <DebugCanvas
        {...makeState({
          imageSrc: "tauri://localhost/fake.png?t=digits",
          showDigitRecognitionRegions: true,
          npGaugeSlots: [
            {
              slot: 0,
              cardRegion: { x: 0.241, y: 0.097, w: 0.187, h: 0.396 },
              ready: false,
              edgeFrac: 0,
              stdBgr: 0,
              gaugeDigitModelLabels: ["1", "0", "0"],
              turnCountModelLabel: "4",
            },
            {
              slot: 1,
              cardRegion: { x: 0.41, y: 0.097, w: 0.187, h: 0.396 },
              ready: false,
              edgeFrac: 0,
              stdBgr: 0,
              gaugeDigitModelLabels: ["未识别", "6", "0"],
              turnCountModelLabel: "4",
            },
            {
              slot: 2,
              cardRegion: { x: 0.603, y: 0.097, w: 0.187, h: 0.396 },
              ready: false,
              edgeFrac: 0,
              stdBgr: 0,
              gaugeDigitModelLabels: ["1", "9", "0"],
              turnCountModelLabel: "4",
            },
          ],
        })}
      />
    );

    expect(container.querySelectorAll(".debug-overlay-turn-count")).toHaveLength(1);
    expect(container.querySelectorAll(".debug-overlay-np-digit-source")).toHaveLength(9);
    expect(container.querySelectorAll(".debug-overlay-other-digit-source")).toHaveLength(8);
    expect(container.querySelectorAll(".debug-overlay-sequence-source")).toHaveLength(3);
    expect(container.textContent).toContain("回合数 4");
    expect(container.textContent).toContain("CTC 己方生命值1");
    expect(container.textContent).toContain("CTC 战斗场次");
    expect(container.textContent).toContain("CTC NP2");
    expect(container.textContent).toContain("NP1 百位 1");
    expect(container.textContent).toContain("NP2 十位 6");
    expect(container.textContent).toContain("NP3 个位 0");
  });

  it("renders command-card support badge overlays and labels", () => {
    const { container } = renderWithTheme(
      <DebugCanvas
        {...makeState({
          imageSrc: "tauri://localhost/fake.png?t=7",
          commandCards: [
            {
              slot: 1,
              x: 0.3,
              y: 0.6,
              cardRegion: { x: 0.2, y: 0.46, w: 0.2, h: 0.4 },
              faceRegion: { x: 0.24, y: 0.55, w: 0.1, h: 0.14 },
              suit: "a",
              servantId: 309,
              isSupport: true,
              isStunned: true,
              supportIconScore: 0.82,
              supportIconRegion: { x: 0.35, y: 0.56, w: 0.035, h: 0.058 },
            },
          ],
        })}
      />
    );

    expect(container.textContent).toContain("助战✓");
    expect(container.textContent).toContain("无法行动");
    expect(container.textContent).toContain("助战 ✓ 0.82");
  });

  it("colors each Grand-badge ROI by its own per-anchor score, not the aggregate flag", () => {
    // Mixed list: row 0 scored 0.71 (Grand), row 1 scored 0.31 (not
    // Grand). Aggregate flag is true because at least one row hit,
    // but the overlay must render row 1 as a miss — otherwise the
    // operator gets a false-positive 冠 ✓ over a non-Grand row.
    const { container: mixedContainer } = renderWithTheme(
      <DebugCanvas
        {...makeState({
          imageSrc: "tauri://localhost/fake.png?t=5",
          supportResult: {
            supports: [],
            diagnostics: {
              listRegion: { x: 0, y: 0, w: 1, h: 1 },
              nameCandidates: [],
              npCandidates: [],
              fragmentCount: 0,
              confirmButtonAnchors: [
                { x: 0.85, y: 0.43, w: 0.07, h: 0.06 },
                { x: 0.85, y: 0.71, w: 0.07, h: 0.06 },
              ],
              isGrandSectionVisible: true,
              grandRibbonAnchorScores: [0.71, 0.31],
            },
          },
        })}
      />
    );
    expect(
      mixedContainer.querySelectorAll(
        ".debug-overlay-support-grand-badge.hit"
      ).length
    ).toBe(1);
    expect(
      mixedContainer.querySelectorAll(
        ".debug-overlay-support-grand-badge.miss"
      ).length
    ).toBe(1);
    // Labels must include the numeric score so operator can spot
    // borderline matches sliding under the threshold over time.
    expect(mixedContainer.textContent).toContain("冠 ✓ 0.71");
    expect(mixedContainer.textContent).toContain("冠 ✗ 0.31");

    // All-miss list: aggregate flag false, every row's score below
    // threshold → every box rendered with .miss.
    const { container: missContainer } = renderWithTheme(
      <DebugCanvas
        {...makeState({
          imageSrc: "tauri://localhost/fake.png?t=6",
          supportResult: {
            supports: [],
            diagnostics: {
              listRegion: { x: 0, y: 0, w: 1, h: 1 },
              nameCandidates: [],
              npCandidates: [],
              fragmentCount: 0,
              confirmButtonAnchors: [
                { x: 0.85, y: 0.43, w: 0.07, h: 0.06 },
              ],
              isGrandSectionVisible: false,
              grandRibbonAnchorScores: [0.12],
            },
          },
        })}
      />
    );
    expect(
      missContainer.querySelectorAll(".debug-overlay-support-grand-badge.miss")
        .length
    ).toBe(1);
    expect(missContainer.textContent).toContain("冠 ✗ 0.12");

    // `null` (template not loaded) suppresses the overlay entirely
    // so the operator isn't shown an ambiguous box on JP captures.
    const { container: nullContainer } = renderWithTheme(
      <DebugCanvas
        {...makeState({
          imageSrc: "tauri://localhost/fake.png?t=7",
          supportResult: {
            supports: [],
            diagnostics: {
              listRegion: { x: 0, y: 0, w: 1, h: 1 },
              nameCandidates: [],
              npCandidates: [],
              fragmentCount: 0,
              confirmButtonAnchors: [
                { x: 0.85, y: 0.43, w: 0.07, h: 0.06 },
              ],
              isGrandSectionVisible: null,
              grandRibbonAnchorScores: [],
            },
          },
        })}
      />
    );
    expect(
      nullContainer.querySelector(".debug-overlay-support-grand-badge")
    ).toBeNull();
  });

  it("renders coord overlays only for groups in visibleCoordGroups", () => {
    const { container } = renderWithTheme(
      <DebugCanvas
        {...makeState({
          imageSrc: "tauri://localhost/fake.png?t=4",
          showCoordOverlay: true,
          visibleCoordGroups: ["battle"],
          coordinates: {
            groups: [
              {
                id: "battle",
                label: "战斗",
                points: [{ label: "tap", point: { x: 0.5, y: 0.5 } }],
                regions: [
                  {
                    label: "atk",
                    region: { x: 0.1, y: 0.1, w: 0.2, h: 0.2 },
                  },
                ],
              },
              {
                id: "menu",
                label: "菜单",
                points: [{ label: "back", point: { x: 0.1, y: 0.9 } }],
                regions: [],
              },
            ],
          },
        })}
      />
    );
    expect(container.querySelectorAll(".debug-coord-region").length).toBe(1);
    expect(container.textContent?.includes("战斗 · atk")).toBe(true);
    expect(container.textContent?.includes("back")).toBe(false);
  });
});
