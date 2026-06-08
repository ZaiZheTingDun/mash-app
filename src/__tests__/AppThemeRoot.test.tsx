import { render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AppThemeRoot } from "../AppThemeRoot";
import type { AppTheme, AppThemePreference } from "../types/theme";

const appProps: Array<{
  theme: AppTheme;
  themePreference: AppThemePreference;
  onThemeChange: (theme: AppThemePreference) => void;
}> = [];

vi.mock("../App", () => ({
  default: (props: {
    theme: AppTheme;
    themePreference: AppThemePreference;
    onThemeChange: (theme: AppThemePreference) => void;
  }) => {
    appProps.push(props);
    return <div>主界面已加载：{props.theme}/{props.themePreference}</div>;
  },
}));

vi.mock("../components/DebugCanvasWindow", () => ({
  DebugCanvasWindow: () => <div>调试窗口已加载</div>,
}));

describe("AppThemeRoot", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockClear();
    appProps.length = 0;
  });

  it("loads the app immediately while startup migration runs in the background", () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "run_startup_migration") {
        return new Promise(() => {});
      }
      if (cmd === "get_app_theme") {
        return Promise.resolve(null);
      }
      return Promise.resolve(null);
    });

    render(<AppThemeRoot isDebugCanvas={false} />);

    expect(screen.getByText(/主界面已加载/)).toBeInTheDocument();
    expect(screen.queryByText("正在从 com.mash.app 迁移数据...")).not.toBeInTheDocument();
    expect(invoke).toHaveBeenCalledWith("run_startup_migration");
  });

  it("skips startup migration for the debug canvas window", () => {
    render(<AppThemeRoot isDebugCanvas={true} />);

    expect(screen.getByText("调试窗口已加载")).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith("run_startup_migration");
  });

  it("restores the saved app theme after startup", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_app_theme") {
        return Promise.resolve("light");
      }
      return Promise.resolve(null);
    });

    render(<AppThemeRoot isDebugCanvas={false} />);

    expect(await screen.findByText("主界面已加载：light/light")).toBeInTheDocument();
  });

  it("uses the system theme when the saved app theme is system", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_app_theme") {
        return Promise.resolve("system");
      }
      return Promise.resolve(null);
    });

    render(<AppThemeRoot isDebugCanvas={false} />);

    expect(await screen.findByText("主界面已加载：light/system")).toBeInTheDocument();
  });

  it("persists theme changes from the app shell", async () => {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_app_theme") {
        return Promise.resolve(null);
      }
      return Promise.resolve(null);
    });

    render(<AppThemeRoot isDebugCanvas={false} />);

    appProps[appProps.length - 1]?.onThemeChange("system");

    expect(invoke).toHaveBeenCalledWith("set_app_theme", { theme: "system" });
  });
});
