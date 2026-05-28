import { render, screen } from "@testing-library/react";
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

  it("loads the app immediately while startup migration runs in the background", () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "run_startup_migration") {
        return new Promise(() => {});
      }
      return Promise.resolve(null);
    });

    render(<AppThemeRoot isDebugCanvas={false} />);

    expect(screen.getByText("主界面已加载")).toBeInTheDocument();
    expect(screen.queryByText("正在从 com.mash.app 迁移数据...")).not.toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("run_startup_migration");
  });

  it("skips startup migration for the debug canvas window", () => {
    render(<AppThemeRoot isDebugCanvas={true} />);

    expect(screen.getByText("调试窗口已加载")).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith("run_startup_migration");
  });
});
