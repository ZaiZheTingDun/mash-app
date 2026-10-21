import { describe, expect, it, vi } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { useCraftEssencePickerData } from "../useCraftEssencePickerData";
import type { CraftEssencePickerData } from "../../../types/craftEssence";

describe("useCraftEssencePickerData", () => {
  it("requests backend recommendations again on each dialog mount", async () => {
    const data: CraftEssencePickerData = {
      craftEssences: [{ id: 191, rarity: 4, category: "bond", name: "星之王冠" }],
      recommendedIds: [], bonusDetails: {}, emptyReason: null,
    };
    vi.mocked(invoke).mockResolvedValue(data);
    const first = renderHook(() => useCraftEssencePickerData([], true, 2));
    expect(first.result.current.loading).toBe(true);
    await waitFor(() => expect(first.result.current.craftEssences).toEqual(data.craftEssences));
    expect(invoke).toHaveBeenCalledWith("get_craft_essence_picker_data", {
      grandBondOnly: true, servantId: 2,
    });
    first.unmount();
    const second = renderHook(() => useCraftEssencePickerData([], true, 2));
    await waitFor(() => expect(second.result.current.loading).toBe(false));
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it("reports failures without exposing an unrestricted grand candidate list and retries on reopen", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error("读取失败"));
    const fallback = [{ id: 1, rarity: 5, category: "normal" as const, name: "万华镜" }];
    const first = renderHook(() => useCraftEssencePickerData(fallback, true, null));
    await waitFor(() => expect(first.result.current.error).toContain("读取失败"));
    expect(first.result.current.craftEssences).toEqual([]);
    first.unmount();
    vi.mocked(invoke).mockResolvedValue({ craftEssences: [], recommendedIds: [], bonusDetails: {}, emptyReason: "请先选择冠位从者" });
    const second = renderHook(() => useCraftEssencePickerData(fallback, true, null));
    await waitFor(() => expect(second.result.current.emptyReason).toBe("请先选择冠位从者"));
    expect(second.result.current.error).toBeNull();
  });
});
