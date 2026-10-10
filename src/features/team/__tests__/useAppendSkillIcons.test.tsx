import { act, renderHook, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAppendSkillIcons } from "../useAppendSkillIcons";

const paths = Array.from({ length: 5 }, (_, index) => ({
  path: index === 4 ? null : `/resources/icons/append-${index}.png`,
  name: `追加技能 ${index + 1}`,
}));

describe("useAppendSkillIcons", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockClear();
  });

  it("loads resource paths and preserves slots for missing icons", async () => {
    vi.mocked(invoke).mockResolvedValue(paths);
    const { result, rerender } = renderHook(() => useAppendSkillIcons());
    await waitFor(() => expect(result.current[0].src).toBe("asset:///resources/icons/append-0.png"));
    expect(invoke).toHaveBeenCalledWith("get_append_skill_icon_paths");
    expect(result.current).toHaveLength(5);
    expect(result.current[4]).toEqual({ src: null, name: "追加技能 5" });
    rerender();
    expect(invoke).toHaveBeenCalledTimes(1);
  });

  it("preserves five numbered placeholders when resource lookup fails", async () => {
    vi.mocked(invoke).mockRejectedValue(new Error("missing resources"));
    const { result } = renderHook(() => useAppendSkillIcons());
    await act(async () => {});
    expect(result.current).toHaveLength(5);
    expect(result.current.every(icon => icon.src === null)).toBe(true);
  });
});
