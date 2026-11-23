import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { expect, it, vi } from "vitest";
import { renderWithTheme } from "../../../test/renderWithTheme";
import { SettingsMcpPage } from "../SettingsMcpPage";

it("toggles the session and copies the address, token, and Codex configuration separately", async () => {
  const stopped = { endpoint: null, allowBattleRuns: false };
  const endpoint = "http://127.0.0.1:44001/mcp";
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "start_mcp_server") return { endpoint, allowBattleRuns: true };
    if (command === "copy_mcp_client_config") return undefined;
    return stopped;
  });
  const user = userEvent.setup();
  renderWithTheme(<SettingsMcpPage />);

  const serviceSwitch = await screen.findByRole("switch", { name: "本地 MCP 服务" });
  const battleAccess = screen.getByRole("switch", { name: "允许运行项目" });
  expect(serviceSwitch).not.toBeChecked();
  expect(battleAccess).not.toBeChecked();
  await user.click(battleAccess);
  await user.click(serviceSwitch);

  expect(invoke).toHaveBeenCalledWith("start_mcp_server", { allowBattleRuns: true });
  expect(await screen.findByText(endpoint)).toBeInTheDocument();
  expect(serviceSwitch).toBeChecked();
  expect(battleAccess).toBeChecked();
  expect(screen.getByLabelText("令牌已隐藏")).toHaveTextContent(/^•+$/);

  await user.click(screen.getByRole("button", { name: "复制地址" }));
  expect(await screen.findByRole("status")).toHaveTextContent("已复制连接地址");
  expect(invoke).toHaveBeenLastCalledWith("copy_mcp_client_config", { part: "address" });
  await user.click(screen.getByRole("button", { name: "复制令牌" }));
  expect(await screen.findByRole("status")).toHaveTextContent("已复制访问令牌");
  expect(invoke).toHaveBeenLastCalledWith("copy_mcp_client_config", { part: "token" });
  await user.click(screen.getByRole("button", { name: "复制 Codex 配置" }));
  expect(await screen.findByRole("status")).toHaveTextContent("已复制 Codex 配置");
  expect(invoke).toHaveBeenLastCalledWith("copy_mcp_client_config", { part: "codex" });
  await user.click(serviceSwitch);
  expect(await screen.findByRole("switch", { name: "本地 MCP 服务", checked: false })).toBeEnabled();
  expect(invoke).toHaveBeenCalledWith("stop_mcp_server");
  expect(battleAccess).not.toBeChecked();
  expect(screen.queryByText(endpoint)).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "复制令牌" })).not.toBeInTheDocument();
});

it("updates battle permission without restarting the running service", async () => {
  const endpoint = "http://127.0.0.1:44001/mcp";
  vi.mocked(invoke).mockClear().mockImplementation(async (command, args) => {
    if (command === "set_mcp_battle_permission") {
      const { allowBattleRuns } = args as { allowBattleRuns: boolean };
      return { endpoint, allowBattleRuns };
    }
    return { endpoint, allowBattleRuns: false };
  });
  const user = userEvent.setup();
  renderWithTheme(<SettingsMcpPage />);
  const battleAccess = await screen.findByRole("switch", { name: "允许运行项目" });
  expect(battleAccess).not.toBeChecked();

  await user.click(battleAccess);
  expect(invoke).toHaveBeenLastCalledWith("set_mcp_battle_permission", { allowBattleRuns: true });
  expect(battleAccess).toBeChecked();

  await user.click(battleAccess);
  expect(invoke).toHaveBeenLastCalledWith("set_mcp_battle_permission", { allowBattleRuns: false });
  expect(battleAccess).not.toBeChecked();
  expect(screen.getByText(endpoint)).toBeInTheDocument();
  expect(screen.getByRole("switch", { name: "本地 MCP 服务" })).toBeChecked();
  expect(invoke).not.toHaveBeenCalledWith("stop_mcp_server");
});

it("keeps permission off when the backend rejects the update", async () => {
  const endpoint = "http://127.0.0.1:44001/mcp";
  vi.mocked(invoke).mockImplementation(async (command) => {
    if (command === "set_mcp_battle_permission") throw new Error("权限更新失败");
    return { endpoint, allowBattleRuns: false };
  });
  const user = userEvent.setup();
  renderWithTheme(<SettingsMcpPage />);
  const battleAccess = await screen.findByRole("switch", { name: "允许运行项目" });

  await user.click(battleAccess);

  expect(await screen.findByRole("alert")).toHaveTextContent("权限更新失败");
  expect(battleAccess).not.toBeChecked();
  expect(screen.getByRole("switch", { name: "本地 MCP 服务" })).toBeChecked();
});
