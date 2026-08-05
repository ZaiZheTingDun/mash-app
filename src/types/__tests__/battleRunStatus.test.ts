import { describe, expect, it } from "vitest";
import {
  battleRunRemainingMs,
  displayedDailyBattleDurationMs,
  formatBattleRunDuration,
  localBattleDayBounds,
  type BattleDailyStatistics,
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

  it("uses local midnight boundaries", () => {
    const now = new Date(2026, 7, 5, 12, 30).getTime();
    expect(localBattleDayBounds(now)).toEqual({
      dayStartMs: new Date(2026, 7, 5).getTime(),
      dayEndMs: new Date(2026, 7, 6).getTime(),
    });
  });

  it("advances today's duration only while the current run is active", () => {
    const statistics: BattleDailyStatistics = {
      dayStartMs: 0,
      calculatedAtMs: 10_000,
      completedRuns: 3,
      durationMs: 8_000,
      apRecoveryUsage: {
        gold: 0,
        silver: 0,
        bronze: 0,
        copper: 0,
        rainbow: 0,
      },
    };
    expect(displayedDailyBattleDurationMs(statistics, status(), 12_000)).toBe(10_000);
    expect(
      displayedDailyBattleDurationMs(
        statistics,
        status({ phase: "finished" }),
        12_000,
      ),
    ).toBe(8_000);
  });
});
