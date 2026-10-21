/**
 * One Craft Essence entry surfaced by the `get_craft_essences` Tauri
 * command. Mirrors the Rust `CraftEssenceInfo` struct. The wire payload
 * is intentionally minimal. Recommendation details are returned separately
 * by `get_craft_essence_picker_data`; `id` is the CE collectionNo.
 */
export type CraftEssenceCategory =
  | "normal"
  | "bond"
  | "manaExchange"
  | "event"
  | "eventReward"
  | "other";

export interface CraftEssence {
  id: number;
  rarity: number;
  category: CraftEssenceCategory;
  name: string;
  nameAliases?: string[];
  nameLink?: string;
}

export interface CraftEssenceBonusDetail {
  text: string;
  icons?: { path: string; name: string }[];
}

/** Backend-owned candidate list and recommendation order for one picker. */
export interface CraftEssencePickerData {
  craftEssences: CraftEssence[];
  recommendedIds: number[];
  bonusDetails: Record<string, CraftEssenceBonusDetail[]>;
  emptyReason: string | null;
}
