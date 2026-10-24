import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Theme } from "@radix-ui/themes";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { renderWithTheme } from "../../../test/renderWithTheme";
import type { TeamVerificationReport } from "../../../types/teamVerification";
import { TeamVerificationDialog } from "../TeamVerificationDialog";

const report: TeamVerificationReport = {
  runId: "run-1", checkId: 2,
  issues: [
    { slotIndex: 1, kind: "servant", expectedName: "阿尔托莉雅", reason: "未匹配到预期对象" },
    { slotIndex: 3, kind: "craftEssence", expectedName: "宝石翁", reason: "缺少识别图片，请更新资源" },
    { slotIndex: null, kind: "mysticCode", expectedName: "迦勒底战斗服", reason: "未匹配到预期对象" },
  ],
};
let onReport: EventCallback<TeamVerificationReport | null>;
function emit(payload: TeamVerificationReport | null) {
  act(() => onReport({ event: "team-verification-updated", id: 1, payload }));
}

describe("TeamVerificationDialog", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation(async (command) => command === "get_team_verification" ? report : undefined);
    vi.mocked(listen).mockImplementation(async (_event, handler) => {
      onReport = handler as EventCallback<TeamVerificationReport | null>;
      return () => {};
    });
  });

  it.each(["light", "dark"] as const)("shows every failure and prevents implicit dismissal in %s mode", async (theme) => {
    const user = userEvent.setup();
    renderWithTheme(<Theme appearance={theme}><TeamVerificationDialog /></Theme>);
    expect(await screen.findByRole("alertdialog")).toBeInTheDocument();
    expect(screen.getByText(/第 2 位从者：阿尔托莉雅/)).toBeInTheDocument();
    expect(screen.getByText(/第 4 位概念礼装：宝石翁/)).toBeInTheDocument();
    expect(screen.getByText(/御主礼装：迦勒底战斗服/)).toBeInTheDocument();
    await user.keyboard("{Escape}");
    // The modal disables pointer events on body; dispatch an outside event
    // directly to verify it cannot request dismissal.
    fireEvent.pointerDown(document.body);
    fireEvent.click(document.body);
    expect(screen.getByRole("alertdialog")).toBeInTheDocument();
    expect(screen.getAllByRole("button")).toHaveLength(3);
  });

  it.each([
    ["重新检查", "retry"], ["跳过检测并继续", "skip"], ["停止任务", "stop"],
  ])("dispatches %s only once and waits for the backend to dismiss", async (label, decision) => {
    const user = userEvent.setup();
    renderWithTheme(<TeamVerificationDialog />);
    await user.dblClick(await screen.findByRole("button", { name: label }));
    if (decision === "stop") {
      expect(invoke).toHaveBeenCalledWith("stop_automation");
    } else {
      expect(invoke).toHaveBeenCalledWith("resolve_team_verification", { runId: "run-1", checkId: 2, decision });
    }
    expect(vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd !== "get_team_verification")).toHaveLength(1);
    for (const button of screen.getAllByRole("button")) expect(button).toBeDisabled();
    emit(null);
    await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
  });

  it("uses the new check id after retry", async () => {
    const user = userEvent.setup();
    renderWithTheme(<TeamVerificationDialog />);
    await user.click(await screen.findByRole("button", { name: "重新检查" }));
    emit({ ...report, checkId: 3, issues: [report.issues[1]] });
    expect(screen.queryByText(/阿尔托莉雅/)).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "跳过检测并继续" }));
    expect(invoke).toHaveBeenLastCalledWith("resolve_team_verification", { runId: "run-1", checkId: 3, decision: "skip" });
  });

  it("ignores a slow initial query after a newer event", async () => {
    let resolveQuery!: (value: TeamVerificationReport) => void;
    vi.mocked(invoke).mockImplementation(() => new Promise((resolve) => { resolveQuery = resolve; }));
    renderWithTheme(<TeamVerificationDialog />);
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("get_team_verification"));
    emit(null);
    await act(async () => resolveQuery(report));
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });

  it("retains the report and enables actions after a command error", async () => {
    const user = userEvent.setup();
    renderWithTheme(<TeamVerificationDialog />);
    const button = await screen.findByRole("button", { name: "重新检查" });
    vi.mocked(invoke).mockRejectedValueOnce(new Error("检查已更新"));
    await user.click(button);
    expect(await screen.findByRole("alert")).toHaveTextContent("检查已更新");
    expect(button).toBeEnabled();
  });
});
