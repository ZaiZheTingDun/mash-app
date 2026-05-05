import type { ProjectSlot } from "../types/project";

/**
 * Default 6-slot layout used when no project is active. Matches the Rust
 * `default_project_slots()` so the empty-project view in the frontend
 * renders the same arrangement a freshly-created project would have.
 */
export function createInitialProjectSlots(): ProjectSlot[] {
  return [
    {
      id: "slot-0",
      type: "servant",
      servantId: null,
      servantVariantKey: null,
      craftEssenceId: null,
    },
    {
      id: "slot-1",
      type: "servant",
      servantId: null,
      servantVariantKey: null,
      craftEssenceId: null,
    },
    {
      id: "slot-2",
      type: "support",
      servantId: null,
      servantVariantKey: null,
      craftEssenceId: null,
    },
    {
      id: "slot-3",
      type: "servant",
      servantId: null,
      servantVariantKey: null,
      craftEssenceId: null,
    },
    {
      id: "slot-4",
      type: "servant",
      servantId: null,
      servantVariantKey: null,
      craftEssenceId: null,
    },
    {
      id: "slot-5",
      type: "servant",
      servantId: null,
      servantVariantKey: null,
      craftEssenceId: null,
    },
  ];
}
