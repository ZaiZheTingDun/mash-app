export type LogLevel = "info" | "debug";

export interface OperationLogEntry {
  time: string;
  message: string;
  level: LogLevel;
}

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
  if (!coalesceKey) return [...logs, entry];

  let matchingIndex = -1;
  for (let index = logs.length - 1; index >= 0; index -= 1) {
    const log = logs[index];
    if (log.level !== entry.level) continue;
    if (operationLogCoalesceKey(log.message) === coalesceKey) {
      matchingIndex = index;
    }
    break;
  }
  if (matchingIndex === -1) return [...logs, entry];

  return logs.map((log, index) => (index === matchingIndex ? entry : log));
}
