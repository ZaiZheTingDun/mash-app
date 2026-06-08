import { describe, expect, it } from "vitest";
import {
  appendCoalescedOperationLog,
  type OperationLogEntry,
} from "../operationLog";

function log(
  time: string,
  message: string,
  level: OperationLogEntry["level"] = "info",
): OperationLogEntry {
  return { time, message, level };
}

describe("appendCoalescedOperationLog", () => {
  it("updates repeated progress logs in place", () => {
    const logs = [log("23:08:35", "等待识别画面… (1/75)")];

    const next = appendCoalescedOperationLog(
      logs,
      log("23:08:36", "等待识别画面… (2/75)"),
    );

    expect(next).toEqual([
      log("23:08:36", "等待识别画面… (2/75)"),
    ]);
  });

  it("starts a new progress entry after another visible log", () => {
    const logs = [
      log("23:08:35", "等待识别画面… (1/75)"),
      log("23:08:35", "队伍就绪"),
    ];

    const next = appendCoalescedOperationLog(
      logs,
      log("23:08:36", "等待识别画面… (2/75)"),
    );

    expect(next).toEqual([
      log("23:08:35", "等待识别画面… (1/75)"),
      log("23:08:35", "队伍就绪"),
      log("23:08:36", "等待识别画面… (2/75)"),
    ]);
  });

  it("keeps ordinary repeated messages as separate entries", () => {
    const logs = [log("12:00:00", "已提交本轮选卡，等待攻击动画")];

    const next = appendCoalescedOperationLog(
      logs,
      log("12:00:01", "已提交本轮选卡，等待攻击动画"),
    );

    expect(next).toEqual([
      log("12:00:00", "已提交本轮选卡，等待攻击动画"),
      log("12:00:01", "已提交本轮选卡，等待攻击动画"),
    ]);
  });

  it("updates repeated waiting logs ending with an ellipsis", () => {
    const logs = [log("12:00:00", "等待战斗动作…")];

    const next = appendCoalescedOperationLog(
      logs,
      log("12:00:01", "等待战斗动作…"),
    );

    expect(next).toEqual([
      log("12:00:01", "等待战斗动作…"),
    ]);
  });

  it("keeps different waiting messages separate", () => {
    const logs = [log("12:00:00", "等待战斗动作…")];

    const next = appendCoalescedOperationLog(
      logs,
      log("12:00:01", "等待技能动画结束…"),
    );

    expect(next).toEqual([
      log("12:00:00", "等待战斗动作…"),
      log("12:00:01", "等待技能动画结束…"),
    ]);
  });

  it("updates the matching visible progress log across debug entries", () => {
    const logs = [
      log("23:08:35", "等待识别画面… (1/75)"),
      log("23:08:35", "结算页可能被弹窗遮挡，尝试点击跳过区域", "debug"),
    ];

    const next = appendCoalescedOperationLog(
      logs,
      log("23:08:36", "等待识别画面… (2/75)"),
    );

    expect(next).toEqual([
      log("23:08:36", "等待识别画面… (2/75)"),
      log("23:08:35", "结算页可能被弹窗遮挡，尝试点击跳过区域", "debug"),
    ]);
  });
});
