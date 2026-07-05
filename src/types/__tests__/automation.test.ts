import { describe, expect, it } from "vitest";
import { isAutomationRunning, isAutomationTerminal } from "../automation";

describe("automation status helpers", () => {
  it("classifies structured runner statuses", () => {
    expect(isAutomationRunning({ status: "starting" })).toBe(true);
    expect(isAutomationRunning({ status: "running" })).toBe(true);
    expect(isAutomationRunning({ status: "error" })).toBe(false);
    expect(isAutomationTerminal({ status: "idle" })).toBe(true);
    expect(isAutomationTerminal({ status: "finished" })).toBe(true);
    expect(isAutomationTerminal({ status: "error" })).toBe(true);
    expect(isAutomationTerminal({ status: "running" })).toBe(false);
  });
});
