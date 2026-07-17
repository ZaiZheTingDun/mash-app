import { describe, expect, it } from "vitest";
import { grandClassToServantClass } from "../supportSettingsModel";

describe("grandClassToServantClass", () => {
  it("maps lancer grand projects to the Lancer support filter", () => {
    expect(grandClassToServantClass("lancer")).toBe("Lancer");
  });
});
