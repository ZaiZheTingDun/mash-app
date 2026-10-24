import { describe, expect, it } from "vitest";
import { PendingProjectWrites } from "../pendingProjectWrites";

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => { resolve = done; });
  return { promise, resolve };
}

describe("pending project writes", () => {
  it("waits for every write in a formation batch and ignores unrelated projects", async () => {
    const writes = new PendingProjectWrites();
    const project = deferred();
    const scenes = deferred();
    void writes.track("p1", "project", () => project.promise);
    void writes.track("p1", "scenes", () => scenes.promise);
    void writes.track("p2", "project", () => new Promise(() => {}));
    let finished = false;
    const barrier = writes.wait("p1").then(() => { finished = true; });
    project.resolve();
    await Promise.resolve();
    expect(finished).toBe(false);
    scenes.resolve();
    await barrier;
    expect(finished).toBe(true);
  });

  it("blocks stale sharing until the failed operation is successfully retried", async () => {
    const writes = new PendingProjectWrites();
    await writes.track("p1", "formation", async () => { throw new Error("disk error"); }).catch(() => {});
    await expect(writes.wait("p1")).rejects.toThrow("保存失败");
    await writes.track("p1", "project", async () => {});
    await expect(writes.wait("p1")).rejects.toThrow("保存失败");
    await writes.track("p1", "formation", async () => {});
    await expect(writes.wait("p1")).resolves.toBeUndefined();
  });
});
