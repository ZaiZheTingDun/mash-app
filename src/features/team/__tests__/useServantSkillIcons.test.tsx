import { renderHook, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, describe, expect, it, vi } from "vitest";
import { useServantSkillIcons } from "../useServantSkillIcons";
import type { Servant } from "../../../types/servant";

function makeServant(id: number, variantKey = `${id}:1`): Servant {
  return {
    id,
    variantKey,
    name_cn: `从者${id}`,
    name_jp: `从者${id}`,
    name_en: `Servant ${id}`,
    class: "saber",
    rarity: 5,
    noblePhantasmName: null,
    noblePhantasmCard: null,
    overWriteServantNames: [],
    faceId: null,
  };
}

describe("useServantSkillIcons", () => {
  afterEach(() => {
    vi.mocked(invoke).mockClear();
  });

  it("deduplicates servants by variant key and stores localized icon entries", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: Parameters<typeof invoke>[1]) => {
      if (cmd !== "get_skill_icon_paths") return null;
      const { servantId, variantKey } = args as { servantId: number; variantKey: string };
      return [
        { path: `/tmp/${servantId}-1.png`, name: `${variantKey}-一技` },
        { path: `/tmp/${servantId}-2.png`, name: `${variantKey}-二技` },
        { path: null, name: "" },
      ];
    });

    const servants = [makeServant(2, "2:1"), makeServant(1, "1:2"), makeServant(2, "2:1"), null];
    const { result } = renderHook(() => useServantSkillIcons(servants));

    await waitFor(() => {
      expect(Object.keys(result.current)).toEqual(["1:2", "2:1"]);
    });

    expect(vi.mocked(invoke).mock.calls).toEqual([
      ["get_skill_icon_paths", { servantId: 1, variantKey: "1:2" }],
      ["get_skill_icon_paths", { servantId: 2, variantKey: "2:1" }],
    ]);
    expect(result.current["1:2"]).toEqual([
      { src: "asset:///tmp/1-1.png", name: "1:2-一技" },
      { src: "asset:///tmp/1-2.png", name: "1:2-二技" },
      { src: null, name: "" },
    ]);
  });

  it("falls back to empty icons when the backend command fails", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_skill_icon_paths") throw new Error("boom");
      return null;
    });

    const { result } = renderHook(() => useServantSkillIcons([makeServant(7, "7:3")]));

    await waitFor(() => {
      expect(result.current["7:3"]).toEqual([
        { src: null, name: "" },
        { src: null, name: "" },
        { src: null, name: "" },
      ]);
    });
  });

  it("does not refetch icons that are already cached on rerender", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string, args?: Parameters<typeof invoke>[1]) => {
      if (cmd !== "get_skill_icon_paths") return null;
      const { servantId } = args as { servantId: number };
      return [
        { path: `/tmp/${servantId}-1.png`, name: "一技" },
        { path: `/tmp/${servantId}-2.png`, name: "二技" },
        { path: `/tmp/${servantId}-3.png`, name: "三技" },
      ];
    });

    const servants = [makeServant(1), makeServant(2)];
    const { result, rerender } = renderHook(
      ({ value }) => useServantSkillIcons(value),
      { initialProps: { value: servants } }
    );

    await waitFor(() => {
      expect(Object.keys(result.current)).toHaveLength(2);
    });

    expect(vi.mocked(invoke)).toHaveBeenCalledTimes(2);
    rerender({ value: [makeServant(2), makeServant(1), makeServant(2)] });

    await waitFor(() => {
      expect(Object.keys(result.current)).toHaveLength(2);
    });
    expect(vi.mocked(invoke)).toHaveBeenCalledTimes(2);
  });
});
