import type { CraftEssence } from "../../types/craftEssence";
import type { Servant } from "../../types/servant";

export interface SlotItem {
  id: string;
  type: "servant" | "support";
  servant: Servant | null;
  /**
   * Pinned craft essence for this slot. Persisted on the project but
   * only consumed by the runner for the support slot today.
   */
  craftEssence: CraftEssence | null;
  craftEssences?: CraftEssence[];
  craftEssenceMultiSelect?: boolean;
  craftEssenceMlbRequired?: boolean;
}
