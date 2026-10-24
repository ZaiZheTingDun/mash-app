/** Transient IPC coordination only; persisted state remains backend-owned. */
export class PendingProjectWrites {
  private sequence = 0;
  private pending = new Map<string, Set<Promise<unknown>>>();
  private latest = new Map<string, number>();
  private failures = new Map<string, Map<string, unknown>>();

  track<T>(projectId: string, operation: string, action: () => Promise<T>): Promise<T> {
    const sequence = ++this.sequence;
    const key = `${projectId}:${operation}`;
    this.latest.set(key, sequence);
    const task = Promise.resolve().then(action);
    const pending = this.pending.get(projectId) ?? new Set();
    this.pending.set(projectId, pending);
    pending.add(task);
    const finish = (failed: boolean, error?: unknown) => {
      pending.delete(task);
      if (pending.size === 0) this.pending.delete(projectId);
      if (this.latest.get(key) !== sequence) return;
      if (failed) {
        const failures = this.failures.get(projectId) ?? new Map();
        failures.set(operation, error);
        this.failures.set(projectId, failures);
      } else {
        this.failures.get(projectId)?.delete(operation);
      }
    };
    void task.then(() => finish(false), (error: unknown) => finish(true, error));
    return task;
  }

  async wait(projectId: string): Promise<void> {
    while (this.pending.get(projectId)?.size) {
      await Promise.allSettled([...this.pending.get(projectId)!]);
    }
    if (this.failures.get(projectId)?.size) {
      throw new Error("编队尚有保存失败的修改，请重新编辑保存后再分享。");
    }
  }
}
