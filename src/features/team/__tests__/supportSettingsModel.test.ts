import { describe, expect, it } from "vitest";
import { grandClassToServantClass } from "../supportSettingsModel";

describe("grandClassToServantClass", () => {
  it("maps lancer grand projects to the Lancer support filter", () => {
    expect(grandClassToServantClass("lancer", [
      { id: "lancer", label: "枪阶冠位", servantClass: "Lancer", roles: [], cardPriorityEnabled: false, autoOrderChangeRoles: [], validationMessage: "" },
    ])).toBe("Lancer");
  });
});
