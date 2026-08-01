import { describe, expect, it } from "vitest";
import { grandClassToServantClass } from "../supportSettingsModel";

describe("grandClassToServantClass", () => {
  it("maps lancer grand projects to the Lancer support filter", () => {
    expect(grandClassToServantClass("lancer", [
      { id: "lancer", label: "枪阶冠位", servantClass: "Lancer", roles: [], cardPriorityEnabled: false, autoOrderChangeRoles: [], validationMessage: "" },
    ])).toBe("Lancer");
  });

  it("maps Extra variants to their grouped servant filters", () => {
    expect(grandClassToServantClass("extra1Earth", [
      { id: "extra1Earth", label: "Extra1 · 地", servantClass: "Extra1", roles: [], cardPriorityEnabled: false, autoOrderChangeRoles: [], validationMessage: "" },
    ])).toBe("Extra1");
    expect(grandClassToServantClass("extra2Water", [
      { id: "extra2Water", label: "Extra2 · 水", servantClass: "Extra2", roles: [], cardPriorityEnabled: false, autoOrderChangeRoles: [], validationMessage: "" },
    ])).toBe("Extra2");
  });
});
