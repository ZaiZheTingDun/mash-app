/**
 * One Craft Essence entry surfaced by the `get_craft_essences` Tauri
 * command. Mirrors the Rust `CraftEssenceInfo` struct. The wire payload
 * is intentionally minimal — full Atlas Academy metadata stays in
 * `assets/ces/{id}/craft-essence.json` and is loaded on demand by the
 * runner only when needed.
 */
export interface CraftEssence {
  id: number;
  name: string;
  nameLink?: string;
}
