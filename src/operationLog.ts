export type LogLevel = "info" | "debug" | "localDebug";

export interface AttackLogCommandCard {
  slot: number;
  suit: string | null;
  servantId: number | null;
  isSupport: boolean;
}

export interface AttackLogSelectedPick {
  step: number;
  total: number;
  fromPriority: string | null;
  kind: "np" | "card";
  slot: number;
  suit?: string | null;
  servantId?: number | null;
}

export interface AttackLogMeta {
  frontServantIds: [number | null, number | null, number | null];
  candidateServantIds?: number[] | null;
  commandCards?: AttackLogCommandCard[] | null;
  readyNpSlots?: number[] | null;
  selectedPick?: AttackLogSelectedPick | null;
}

export type ActionLogMeta =
  | {
      kind: "servantSkill";
      servantId: number | null;
      skillIndex: number;
      targetServantId: number | null;
    }
  | {
      kind: "equipmentSkill";
      skillIndex: number;
      targetServantId: number | null;
    }
  | {
      kind: "commandSpell";
      spell: string;
      targetServantId: number | null;
    }
  | {
      kind: "orderChange";
      frontServantId: number | null;
      backServantId: number | null;
    }
  | {
      kind: "skippedAction";
      servantId: number | null;
    };

export interface OperationLogEntry {
  time: string;
  message: string;
  level: LogLevel;
  attack?: AttackLogMeta | null;
  action?: ActionLogMeta | null;
}

export const MAX_OPERATION_LOG_ENTRIES = 500;

const PROGRESS_LOG_RE = /^(.*…)\s+\((\d+)\/(\d+)\)$/;

function operationLogCoalesceKey(message: string): string | null {
  const match = message.match(PROGRESS_LOG_RE);
  if (match) return `progress:${match[1]}:${match[3]}`;
  if (message.endsWith("…")) return `waiting:${message}`;
  return null;
}

export function appendCoalescedOperationLog(
  logs: OperationLogEntry[],
  entry: OperationLogEntry,
): OperationLogEntry[] {
  const coalesceKey = operationLogCoalesceKey(entry.message);
  if (!coalesceKey) return trimOperationLogs([...logs, entry]);

  let matchingIndex = -1;
  for (let index = logs.length - 1; index >= 0; index -= 1) {
    const log = logs[index];
    if (log.level !== entry.level) continue;
    if (operationLogCoalesceKey(log.message) === coalesceKey) {
      matchingIndex = index;
    }
    break;
  }
  if (matchingIndex === -1) return trimOperationLogs([...logs, entry]);

  return trimOperationLogs(
    logs.map((log, index) => (index === matchingIndex ? entry : log))
  );
}

function trimOperationLogs(logs: OperationLogEntry[]): OperationLogEntry[] {
  if (logs.length <= MAX_OPERATION_LOG_ENTRIES) return logs;
  return logs.slice(logs.length - MAX_OPERATION_LOG_ENTRIES);
}
