import { describe, expect, it } from "vitest";
import {
  battleRunRemainingMs,
  formatBattleRunDuration,
  type BattleRunStatus,
} from "../battleRunStatus";

function status(overrides: Partial<BattleRunStatus> = {}): BattleRunStatus {
  return {
    phase: "running",
    startedAtMs: 0,
    endedAtMs: null,
    lastCompletedAtMs: 20_000,
    completedRuns: 2,
    maxRuns: 5,
    apRecoveryUsage: {
      gold: 0,
      silver: 0,
      bronze: 0,
      copper: 0,
      rainbow: 0,
    },
    ...overrides,
  };
}

describe("battle run status formatting", () => {
  it("formats elapsed time with unbounded hours", () => {
    expect(formatBattleRunDuration(6_800_000)).toBe("1 小时 53 分钟 20 秒");
    expect(formatBattleRunDuration(27 * 3_600_000)).toBe("27 小时 0 秒");
  });

  it("estimates remaining time from completed-run average and current progress", () => {
    expect(battleRunRemainingMs(status(), 24_000)).toBe(26_000);
  });

  it("does not estimate before the first completion or after an early stop", () => {
    expect(battleRunRemainingMs(status({ completedRuns: 0 }), 5_000)).toBeNull();
    expect(battleRunRemainingMs(status({ phase: "stopped" }), 24_000)).toBeNull();
  });

  it("returns zero when the configured target is complete", () => {
    expect(battleRunRemainingMs(status({ completedRuns: 5 }), 50_000)).toBe(0);
  });
});
