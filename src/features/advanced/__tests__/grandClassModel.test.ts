import { describe, expect, it } from "vitest";
import type { GrandClassDefinition } from "../../../types/project";
import {
  grandClassDefinition,
  validateGrandServants,
} from "../grandClassModel";

const testDefinition: GrandClassDefinition = {
  id: "test-class",
  label: "测试冠位",
  servantClass: "MoonCancer",
  roles: [
    { role: "alpha", label: "甲", required: true },
    { role: "beta", label: "乙", required: true },
  ],
  cardPriorityEnabled: false,
  autoOrderChangeRoles: ["beta"],
  validationMessage: "需要甲乙冠位",
};

describe("grandClassModel", () => {
  it("accepts a backend-defined class without frontend class branches", () => {
    expect(grandClassDefinition([testDefinition], "test-class")).toBe(testDefinition);
    expect(
      validateGrandServants(
        [
          { slotIndex: 0, role: "alpha" },
          { slotIndex: 4, role: "beta" },
        ],
        testDefinition,
      ),
    ).toBe(true);
  });

  it("rejects missing required roles and duplicate party slots", () => {
    expect(validateGrandServants([{ slotIndex: 0, role: "alpha" }], testDefinition)).toBe(false);
    expect(
      validateGrandServants(
        [
          { slotIndex: 0, role: "alpha" },
          { slotIndex: 0, role: "beta" },
        ],
        testDefinition,
      ),
    ).toBe(false);
  });
});
