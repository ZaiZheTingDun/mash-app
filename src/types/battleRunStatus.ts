import type { BattleApRecoveryItem } from "./project";

export type BattleRunPhase =
  | "starting"
  | "running"
  | "finished"
  | "stopped"
  | "error";

export type BattleRunApRecoveryUsage = Record<BattleApRecoveryItem, number>;

export interface BattleRunProgressEvent {
  completedRuns: number;
  maxRuns: number | null;
  apRecoveryUsage: BattleRunApRecoveryUsage;
}

export interface BattleRunStatus extends BattleRunProgressEvent {
  phase: BattleRunPhase;
  startedAtMs: number;
  endedAtMs: number | null;
  lastCompletedAtMs: number | null;
}

export interface BattleDailyStatistics {
  dayStartMs: number;
  calculatedAtMs: number;
  completedRuns: number;
  durationMs: number;
  apRecoveryUsage: BattleRunApRecoveryUsage;
}

export const EMPTY_AP_RECOVERY_USAGE: BattleRunApRecoveryUsage = {
  gold: 0,
  silver: 0,
  bronze: 0,
  copper: 0,
  rainbow: 0,
};

export function formatBattleRunDuration(
  durationMs: number,
  alwaysShowSeconds = true,
): string {
  const totalSeconds = Math.max(0, Math.floor(durationMs / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  const parts: string[] = [];
  if (hours > 0) parts.push(`${hours} 小时`);
  if (minutes > 0) parts.push(`${minutes} 分钟`);
  if (alwaysShowSeconds || parts.length === 0 || seconds > 0) {
    parts.push(`${seconds} 秒`);
  }
  return parts.join(" ");
}

export function battleRunRemainingMs(
  status: BattleRunStatus,
  nowMs: number,
): number | null {
  if (status.maxRuns == null || status.completedRuns === 0) return null;
  if (status.completedRuns >= status.maxRuns) return 0;
  if (status.phase !== "starting" && status.phase !== "running") return null;

  const lastCompletedAtMs = status.lastCompletedAtMs ?? nowMs;
  const elapsedAtLastCompletion = Math.max(
    0,
    lastCompletedAtMs - status.startedAtMs,
  );
  const averageRunMs = elapsedAtLastCompletion / status.completedRuns;
  const elapsedSinceLastCompletion = Math.max(0, nowMs - lastCompletedAtMs);
  return Math.max(
    0,
    averageRunMs * (status.maxRuns - status.completedRuns) - elapsedSinceLastCompletion,
  );
}

export function localBattleDayBounds(nowMs = Date.now()): {
  dayStartMs: number;
  dayEndMs: number;
} {
  const now = new Date(nowMs);
  const dayStartMs = new Date(
    now.getFullYear(),
    now.getMonth(),
    now.getDate(),
  ).getTime();
  const dayEndMs = new Date(
    now.getFullYear(),
    now.getMonth(),
    now.getDate() + 1,
  ).getTime();
  return { dayStartMs, dayEndMs };
}

export function displayedDailyBattleDurationMs(
  statistics: BattleDailyStatistics,
  status: BattleRunStatus | null,
  nowMs: number,
): number {
  const active =
    status?.phase === "starting" || status?.phase === "running";
  if (!active || nowMs <= statistics.calculatedAtMs) {
    return statistics.durationMs;
  }
  return statistics.durationMs + nowMs - statistics.calculatedAtMs;
}
