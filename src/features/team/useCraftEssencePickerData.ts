import { useEffect, useState } from "react";
import { invoke } from "../../tauri";
import type { CraftEssence, CraftEssencePickerData } from "../../types/craftEssence";

/** Mounted with the open dialog; Rust shares the parsed resource index. */
export function useCraftEssencePickerData(
  fallbackCatalog: CraftEssence[],
  grandBondOnly: boolean,
  servantId: number | null,
) {
  const [result, setResult] = useState<{
    data: CraftEssencePickerData | null;
    error: string | null;
  } | null>(null);

  useEffect(() => {
    let cancelled = false;
    invoke<CraftEssencePickerData>("get_craft_essence_picker_data", {
      grandBondOnly,
      servantId,
    }).then((data) => {
      if (!cancelled) setResult({ data, error: null });
    }).catch((error: unknown) => {
      if (!cancelled) setResult({ data: null, error: String(error) });
    });
    return () => { cancelled = true; };
  }, [grandBondOnly, servantId]);

  return {
    craftEssences: result?.data?.craftEssences ?? (grandBondOnly ? [] : fallbackCatalog),
    recommendedIds: result?.data?.recommendedIds ?? [],
    bonusDetails: result?.data?.bonusDetails ?? {},
    emptyReason: result?.data?.emptyReason ?? null,
    loading: result === null,
    error: result?.error ?? null,
  };
}
