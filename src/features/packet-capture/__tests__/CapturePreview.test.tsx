import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderWithTheme } from "../../../test/renderWithTheme";
import type { CapturePreview as Preview, CapturePreviewQuery } from "../../../types/packetCapture";
import { CapturePreview } from "../CapturePreview";
import { previewFixture } from "./previewFixture";

const read = vi.fn<(id: string, query: CapturePreviewQuery) => Promise<Preview>>();
const props = { recordId: "a", loading: false, recordError: false, waiting: false };
beforeEach(() => {
  read.mockReset().mockImplementation(async (id, query) => previewFixture(id, query.tab));
  vi.mocked(invoke).mockReset().mockImplementation((command, args) => {
    if (command === "get_packet_capture_preview") {
      const { recordId, query } = args as { recordId: string; query: CapturePreviewQuery };
      return read(recordId, query);
    }
    return Promise.resolve(null);
  });
});

describe("CapturePreview", () => {
  it("renders overview quantities and missing fields, and opens the corresponding list from a summary", async () => {
    const user = userEvent.setup();
    renderWithTheme(<CapturePreview {...props} />);
    expect(screen.getByRole("status", { name: "正在加载账户物品" })).toBeInTheDocument();
    expect(await screen.findByText("测试御主")).toBeInTheDocument();
    expect(screen.getByText("100,000")).toBeInTheDocument();
    expect(screen.getByText("001234567")).toBeInTheDocument();
    expect(screen.getByText("—")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "从者 1 种 / 2 份" }));
    expect(await screen.findByRole("button", { name: "查看玛修，实例 first" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "从者" })).toHaveAttribute("aria-selected", "true");
  });

  it("keeps duplicate servants separate and displays the selected instance's detail", async () => {
    const user = userEvent.setup();
    renderWithTheme(<CapturePreview {...props} />);
    await user.click(screen.getByRole("tab", { name: "从者" }));
    await user.click(await screen.findByRole("button", { name: "查看玛修，实例 second" }));
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByText("实例 second · 保管室")).toBeInTheDocument();
    expect(within(dialog).getByText("1 / 1")).toBeInTheDocument();
    expect(within(dialog).getByText("1 / — / —")).toBeInTheDocument();
    expect(within(dialog).queryByText("Lv.10")).not.toBeInTheDocument();
  });

  it("shows grouped CE instances and delegates the full-limit filter to Rust", async () => {
    const user = userEvent.setup();
    renderWithTheme(<CapturePreview {...props} />);
    await user.click(screen.getByRole("tab", { name: "礼装" }));
    await user.click(await screen.findByRole("button", { name: "查看顽强，2张" }));
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByText("实例 ce1")).toBeInTheDocument();
    expect(within(dialog).getByText("实例 ce2")).toBeInTheDocument();
    expect(within(dialog).getByText("保管室 · 未锁定")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "关闭" }));
    await user.click(screen.getByRole("checkbox", { name: "仅满破" }));
    await waitFor(() => expect(read).toHaveBeenLastCalledWith("a", expect.objectContaining({ tab: "craftEssences", limitBroken: true, page: 0 })));
  });

  it("debounces search drafts and renders the backend's unknown-item results and pagination", async () => {
    const user = userEvent.setup();
    read.mockImplementation(async (id, query) => ({ ...previewFixture(id, query.tab), page: query.page, pageCount: 3, total: 81 }));
    renderWithTheme(<CapturePreview {...props} />);
    await user.click(screen.getByRole("tab", { name: "物品" }));
    expect(await screen.findByText("物品 #999")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "下一页" }));
    await waitFor(() => expect(read).toHaveBeenLastCalledWith("a", expect.objectContaining({ page: 1 })));
    await user.type(screen.getByRole("textbox", { name: "搜索账户数据" }), "999");
    await waitFor(() => expect(read).toHaveBeenLastCalledWith("a", expect.objectContaining({ tab: "items", search: "999", page: 0 })));
    await user.click(screen.getByRole("checkbox", { name: "显示零数量" }));
    await waitFor(() => expect(read).toHaveBeenLastCalledWith("a", expect.objectContaining({ showZero: true })));
  });

  it.each(["resolve", "reject"])("retains the active tab on history changes and ignores a stale preview %s", async outcome => {
    const user = userEvent.setup();
    const { rerender } = renderWithTheme(<CapturePreview {...props} />);
    await user.click(screen.getByRole("tab", { name: "从者" }));
    await screen.findByRole("button", { name: "查看玛修，实例 first" });
    let finish!: (value: Preview) => void;
    let fail!: (reason: string) => void;
    read.mockImplementation((id, query) => id === "b" ? new Promise((resolve, reject) => { finish = resolve; fail = reject; }) : Promise.resolve(previewFixture(id, query.tab)));
    rerender(<CapturePreview {...props} recordId="b" />);
    await waitFor(() => expect(read).toHaveBeenLastCalledWith("b", expect.objectContaining({ tab: "servants" })));
    expect(screen.queryByRole("button", { name: "查看玛修，实例 first" })).not.toBeInTheDocument();
    expect(screen.getByRole("status", { name: "正在加载账户物品" })).toBeInTheDocument();
    rerender(<CapturePreview {...props} recordId="c" />);
    await screen.findByRole("button", { name: "查看玛修，实例 first" });
    await act(async () => outcome === "resolve" ? finish({ ...previewFixture("b", "servants"), servants: [] }) : fail("旧预览失败"));
    expect(screen.getByRole("button", { name: "查看玛修，实例 first" })).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "从者" })).toHaveAttribute("aria-selected", "true");
  });

  it("allows retrying a projection error without changing or reloading the raw response", async () => {
    const user = userEvent.setup();
    read.mockRejectedValueOnce("缺少汇总资源");
    renderWithTheme(<CapturePreview {...props} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("缺少汇总资源");
    await user.click(screen.getByRole("button", { name: "重新解析" }));
    expect(await screen.findByText("测试御主")).toBeInTheDocument();
    expect(read).toHaveBeenCalledTimes(2);
    expect(vi.mocked(invoke).mock.calls.every(([cmd]) => cmd === "get_packet_capture_preview")).toBe(true);
  });
});
