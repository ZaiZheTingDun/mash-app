// Only coordinates IPC ordering; settings themselves remain backend-owned.
// Keep queues outside page instances so navigation cannot reorder writes.
const pending = new Map<string, Promise<void>>();

export const projectThresholdScope = (projectId: string) => `project:${projectId}`;

export function queueThresholdOperation<T>(scope: string, operation: () => Promise<T>): Promise<T> {
  const result = (pending.get(scope) ?? Promise.resolve()).then(operation);
  const settled = result.then(() => {}, () => {});
  pending.set(scope, settled);
  void settled.then(() => {
    if (pending.get(scope) === settled) pending.delete(scope);
  });
  return result;
}

export async function waitForThresholdWrites(scope: string): Promise<void> {
  while (pending.has(scope)) await pending.get(scope);
}
