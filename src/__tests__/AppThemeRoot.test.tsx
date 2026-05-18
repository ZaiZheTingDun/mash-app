import { act, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AppThemeRoot } from "../AppThemeRoot";

vi.mock("../App", () => ({
  default: () => <div>主界面已加载</div>,
}));

vi.mock("../components/DebugCanvasWindow", () => ({
  DebugCanvasWindow: () => <div>调试窗口已加载</div>,
}));

describe("AppThemeRoot", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockClear();
  });

  it("shows the startup migration page until migration finishes", async () => {
    let resolveMigration:
      | ((status: { migrated: boolean; from: string | null; to: string }) => void)
      | undefined;
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "run_startup_migration") {
        return new Promise((resolve) => {
          resolveMigration = resolve;
        });
      }
      return Promise.resolve(null);
    });

    render(<AppThemeRoot isDebugCanvas={false} />);

    expect(screen.getByText("正在从 com.mash.app 迁移数据...")).toBeInTheDocument();
    expect(screen.getByText("该操作只会进行一次。")).toBeInTheDocument();
    expect(screen.queryByText("主界面已加载")).not.toBeInTheDocument();

    await act(async () => {
      resolveMigration?.({ migrated: true, from: "/old", to: "/new" });
    });

    expect(await screen.findByText("主界面已加载")).toBeInTheDocument();
  });

  it("skips startup migration for the debug canvas window", () => {
    render(<AppThemeRoot isDebugCanvas={true} />);

    expect(screen.getByText("调试窗口已加载")).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith("run_startup_migration");
  });
});
