/**
 * One Craft Essence entry surfaced by the `get_craft_essences` Tauri
 * command. Mirrors the Rust `CraftEssenceInfo` struct. The wire payload
 * is intentionally minimal — full Atlas Academy metadata stays in
 * `assets/ces/{id}/craft-essence.json`, where `id` is the CE
 * collectionNo, and is loaded on demand by the runner only when needed.
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
