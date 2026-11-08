import { describe, expect, it, vi } from "vitest";
import { projectThresholdScope, queueThresholdOperation, waitForThresholdWrites } from "../recognitionThresholdQueue";

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => { resolve = done; });
  return { promise, resolve };
}

describe("threshold persistence ordering", () => {
  it("waits for queued edits before sharing while other teams can save independently", async () => {
    const scope = projectThresholdScope("a");
    const first = deferred();
    const second = deferred();
    const other = vi.fn(async () => {});
    void queueThresholdOperation(scope, () => first.promise);
    void queueThresholdOperation(scope, () => second.promise);
    await queueThresholdOperation(projectThresholdScope("b"), other);
    expect(other).toHaveBeenCalledOnce();
    const readyToShare = vi.fn();
    const wait = waitForThresholdWrites(scope).then(readyToShare);
    first.resolve();
    await Promise.resolve();
    expect(readyToShare).not.toHaveBeenCalled();
    second.resolve();
    await wait;
    expect(readyToShare).toHaveBeenCalledOnce();
  });

  it("reports a failed save to its caller and continues with a later edit", async () => {
    const failed = queueThresholdOperation("global", async () => { throw new Error("保存失败"); });
    const retry = queueThresholdOperation("global", async () => 0.75);
    await expect(failed).rejects.toThrow("保存失败");
    await expect(retry).resolves.toBe(0.75);
    await expect(waitForThresholdWrites("global")).resolves.toBeUndefined();
  });
});
