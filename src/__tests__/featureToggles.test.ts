import { describe, expect, it } from "vitest";
import { createFeatureToggles } from "../featureToggles";

describe("feature toggles", () => {
  it("enables unfinished tools by default during development", () => {
    expect(createFeatureToggles({ DEV: true })).toEqual({
      servantEnhancement: true,
      cvDebug: true,
      grandCardPriority: true,
      settingsDebug: true,
    });
  });

  it("hides unfinished tools by default in production builds", () => {
    expect(createFeatureToggles({ DEV: false })).toEqual({
      servantEnhancement: false,
      cvDebug: false,
      grandCardPriority: false,
      settingsDebug: false,
    });
  });

  it("allows production builds to opt in per feature", () => {
    expect(
      createFeatureToggles({
        DEV: false,
        VITE_FEATURE_SERVANT_ENHANCEMENT: "true",
        VITE_FEATURE_CV_DEBUG: "1",
        VITE_FEATURE_GRAND_CARD_PRIORITY: "on",
      })
    ).toEqual({
      servantEnhancement: true,
      cvDebug: true,
      grandCardPriority: true,
      settingsDebug: false,
    });
  });
});
