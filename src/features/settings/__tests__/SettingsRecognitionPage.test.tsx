import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { renderWithTheme } from "../../../test/renderWithTheme";
import type { RecognitionSettings } from "../../../types/recognition";
import { RecognitionThresholdSettings } from "../SettingsRecognitionPage";
import { DEFAULT_RECOGNITION_SETTINGS, type ThresholdKey } from "../recognitionSettingsModel";

const loadSettings = async () => DEFAULT_RECOGNITION_SETTINGS;

async function findInput() {
  const input = await screen.findByRole("spinbutton", { name: "助战礼装匹配阈值数值" });
  await waitFor(() => expect(input).not.toBeDisabled());
  return input;
}

describe("RecognitionThresholdSettings automatic saving", () => {
  it("places the scope explanation after all threshold controls as a footer note", async () => {
    const helpText = "此处为全局设置；若只想针对某个队伍进行设置，请前往队伍设置页面。";
    renderWithTheme(<RecognitionThresholdSettings scope="test" active helpText={helpText}
      loadSettings={loadSettings} saveThreshold={vi.fn()} />);
    await findInput();
    const note = screen.getByRole("note");
    expect(note).toHaveTextContent(helpText);
    for (const control of screen.getAllByRole("spinbutton")) {
      expect(control.compareDocumentPosition(note) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    }
  });

  it("saves valid edits without a save button and restores defaults from the button before the input", async () => {
    const save = vi.fn(async ({ key }: { key: ThresholdKey }, value: number) => ({ ...DEFAULT_RECOGNITION_SETTINGS, [key]: value }));
    renderWithTheme(<RecognitionThresholdSettings scope="test" active loadSettings={loadSettings} saveThreshold={save} />);
    const input = await findInput();
    expect(screen.queryByRole("button", { name: "保存" })).not.toBeInTheDocument();
    const reset = screen.getAllByRole("button", { name: "恢复默认" })[0];
    expect(reset.compareDocumentPosition(input) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    fireEvent.change(input, { target: { value: "0.65" } });
    expect(await screen.findByText("已保存")).toBeInTheDocument();
    expect(save).toHaveBeenCalledWith(expect.objectContaining({ key: "supportCeThreshold" }), 0.65, undefined);
    fireEvent.click(reset);
    await waitFor(() => expect(save).toHaveBeenLastCalledWith(
      expect.objectContaining({ key: "supportCeThreshold" }), 0.7, { restoreDefault: true }
    ));
    expect(input).toHaveValue(0.7);
  });

  it("serializes edits across fields and keeps newer drafts editable during slow saves", async () => {
    const pending: Array<{ key: string; value: number; resolve: (settings: RecognitionSettings) => void }> = [];
    const save = vi.fn(({ key }: { key: ThresholdKey }, value: number) => new Promise<RecognitionSettings>((resolve) => {
      pending.push({ key, value, resolve });
    }));
    renderWithTheme(<RecognitionThresholdSettings scope="test" active loadSettings={loadSettings} saveThreshold={save} />);
    const input = await findInput();
    const second = screen.getByRole("spinbutton", { name: "满破图标匹配阈值数值" });
    fireEvent.change(input, { target: { value: "0.65" } });
    await waitFor(() => expect(pending).toHaveLength(1));
    expect(input).not.toBeDisabled();
    expect(second).not.toBeDisabled();
    fireEvent.change(input, { target: { value: "0.66" } });
    fireEvent.change(second, { target: { value: "0.77" } });
    expect(save).toHaveBeenCalledTimes(1);
    await act(async () => pending[0].resolve({ ...DEFAULT_RECOGNITION_SETTINGS, supportCeThreshold: 0.65 }));
    expect(input).toHaveValue(0.66);
    expect(second).toHaveValue(0.77);
    expect(pending[1].value).toBe(0.66);
    await act(async () => pending[1].resolve({ ...DEFAULT_RECOGNITION_SETTINGS, supportCeThreshold: 0.66 }));
    expect(pending[2]).toMatchObject({ key: "supportMlbIconThreshold", value: 0.77 });
    await act(async () => pending[2].resolve({ ...DEFAULT_RECOGNITION_SETTINGS, supportMlbIconThreshold: 0.77 }));
    expect(input).toHaveValue(0.66);
    expect(second).toHaveValue(0.77);
  });

  it("does not save incomplete or out-of-range drafts and allows retrying after a failed save", async () => {
    const save = vi.fn().mockRejectedValueOnce(new Error("无法保存阈值"))
      .mockImplementation(async ({ key }: { key: ThresholdKey }, value: number) => ({ ...DEFAULT_RECOGNITION_SETTINGS, [key]: value }));
    renderWithTheme(<RecognitionThresholdSettings scope="test" active loadSettings={loadSettings} saveThreshold={save} />);
    const input = await findInput();
    for (const value of ["", "0", "0.59", "0.86"]) {
      fireEvent.change(input, { target: { value } });
    }
    expect(save).not.toHaveBeenCalled();
    fireEvent.change(input, { target: { value: "0.65" } });
    expect(await screen.findByRole("alert")).toHaveTextContent("无法保存阈值");
    expect(input).toHaveValue(0.65);
    fireEvent.change(input, { target: { value: "0.66" } });
    expect(await screen.findByText("已保存")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("keeps a default reset queued after a pending edit", async () => {
    let finish!: (settings: RecognitionSettings) => void;
    const save = vi.fn().mockImplementationOnce(() => new Promise<RecognitionSettings>((resolve) => { finish = resolve; }))
      .mockResolvedValue(DEFAULT_RECOGNITION_SETTINGS);
    renderWithTheme(<RecognitionThresholdSettings scope="test" active loadSettings={loadSettings} saveThreshold={save} />);
    const input = await findInput();
    fireEvent.change(input, { target: { value: "0.65" } });
    await waitFor(() => expect(save).toHaveBeenCalledOnce());
    fireEvent.click(screen.getAllByRole("button", { name: "恢复默认" })[0]);
    await act(async () => finish({ ...DEFAULT_RECOGNITION_SETTINGS, supportCeThreshold: 0.65 }));
    await waitFor(() => expect(save).toHaveBeenCalledTimes(2));
    expect(save).toHaveBeenLastCalledWith(expect.objectContaining({ key: "supportCeThreshold" }), 0.7, { restoreDefault: true });
    expect(input).toHaveValue(0.7);
  });

  it("finishes old page edits before reloading and saving from a reopened page", async () => {
    let stored = { ...DEFAULT_RECOGNITION_SETTINGS };
    let finish!: () => void;
    const persist = vi.fn(async ({ key }: { key: ThresholdKey }, value: number) => {
      stored = { ...stored, [key]: value };
      return stored;
    });
    const save = vi.fn().mockImplementationOnce(async (config, value) => {
      await new Promise<void>((resolve) => { finish = resolve; });
      return persist(config, value);
    }).mockImplementation(persist);
    const load = vi.fn(async () => stored);
    const first = renderWithTheme(<RecognitionThresholdSettings scope="global" active
      loadSettings={load} saveThreshold={save} />);
    const input = await findInput();
    fireEvent.change(input, { target: { value: "0.65" } });
    await waitFor(() => expect(save).toHaveBeenCalledOnce());
    fireEvent.change(input, { target: { value: "0.66" } });
    first.unmount();
    renderWithTheme(<RecognitionThresholdSettings scope="global" active
      loadSettings={load} saveThreshold={save} />);
    expect(load).toHaveBeenCalledOnce();
    await act(async () => finish());
    const reopenedInput = await findInput();
    expect(reopenedInput).toHaveValue(0.66);
    fireEvent.change(reopenedInput, { target: { value: "0.75" } });
    expect(await screen.findByText("已保存")).toBeInTheDocument();
    expect(stored.supportCeThreshold).toBe(0.75);
    expect(persist.mock.calls.map(([, value]) => value)).toEqual([0.65, 0.66, 0.75]);
  });
});
